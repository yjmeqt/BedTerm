#!/usr/bin/env bash
# Builds bedterm_core for iOS device + iOS sim + macOS, then assembles
# BedTermCore.xcframework consumed by BedTermKit as a binary target.
#
# Run from repo root or via Xcode pre-action / build phase:
#     ./scripts/build-rust-xcframework.sh [debug|release|Debug|Release]
#
# Idempotent: if the staged .a in the xcframework is byte-identical to the
# fresh cargo output for every slice, we skip the `xcodebuild
# -create-xcframework` step (which always rewrites the directory).
set -euo pipefail

# Xcode invokes pre-actions / build phases with a minimal PATH. Ensure
# rustup/cargo are reachable from the standard install location and Homebrew.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

# TODO: remove once rustup stops warning about Xcode-set env vars it doesn't
# recognize. Xcode 26 exports SWIFT_DEBUG_INFORMATION_{FORMAT,VERSION}; rustup's
# proxy shim (>=1.28) prints "Warning: unknown environment variable …" once per
# rustc invocation, producing dozens of duplicate lines. Cosmetic only.
unset SWIFT_DEBUG_INFORMATION_FORMAT SWIFT_DEBUG_INFORMATION_VERSION

# Accept Debug/Release (Xcode `$CONFIGURATION`) and debug/release.
PROFILE_RAW="${1:-release}"
PROFILE_RAW="${PROFILE_RAW#--}"
PROFILE="$(printf '%s' "$PROFILE_RAW" | tr '[:upper:]' '[:lower:]')"
CARGO_PROFILE_ARG=()
PROFILE_DIR="debug"
if [ "$PROFILE" = "release" ] || [ "$PROFILE" = "testflight" ]; then
  CARGO_PROFILE_ARG=(--release)
  PROFILE_DIR="release"
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUST_DIR="$REPO_ROOT/rust-core"
OUT_DIR="$REPO_ROOT/BedTermKit/BinaryFrameworks"
FW_DIR="$OUT_DIR/BedTermCore.xcframework"
LIB_NAME="libbedterm_core.a"
RS_LIB_NAME="libbedterm_rs_terminal.a"
HEADER="$RUST_DIR/bedterm_core/include/bedterm_core.h"
# Supplemental header with declarations for crates outside bedterm_core.
# cbindgen regenerates HEADER on every bedterm_core build, so we keep extra
# declarations in a separate file and cat them together into every staged copy.
RS_TERMINAL_HEADER="$RUST_DIR/bedterm-rs-terminal/include/bedterm_rs_terminal.h"

# rust-target → xcframework slice id (matches the Info.plist committed alongside).
TARGETS=(
  "aarch64-apple-ios"
  "aarch64-apple-ios-sim"
  "aarch64-apple-darwin"
)
SLICE_IDS=(
  "ios-arm64"
  "ios-arm64-simulator"
  "macos-arm64"
)

cd "$RUST_DIR"
for t in "${TARGETS[@]}"; do
  echo "==> cargo build -p bedterm_core --target $t ($PROFILE)"
  cargo build -p bedterm_core \
    "${CARGO_PROFILE_ARG[@]+"${CARGO_PROFILE_ARG[@]}"}" \
    --target "$t"
done

for t in "${TARGETS[@]}"; do
  echo "==> cargo build -p bedterm-rs-terminal --target $t ($PROFILE)"
  cargo build -p bedterm-rs-terminal \
    "${CARGO_PROFILE_ARG[@]+"${CARGO_PROFILE_ARG[@]}"}" \
    --target "$t"
done

# Fast-path: skip the expensive xcframework rebuild when the staged merged .a
# is current. Because the staged .a is produced by libtool (not a direct copy
# of a cargo artifact), we track freshness via a sidecar SHA256 digest file
# stored next to the staged .a. The digest covers both input .a files so any
# change to either crate triggers a rebuild.
need_rebuild=0
for i in "${!TARGETS[@]}"; do
  src="$RUST_DIR/target/${TARGETS[$i]}/$PROFILE_DIR/$LIB_NAME"
  rs_src="$RUST_DIR/target/${TARGETS[$i]}/$PROFILE_DIR/$RS_LIB_NAME"
  digest_file="$FW_DIR/${SLICE_IDS[$i]}/.bedterm_input_digest"
  staged="$FW_DIR/${SLICE_IDS[$i]}/$LIB_NAME"
  if [ ! -f "$staged" ] || [ ! -f "$digest_file" ]; then
    need_rebuild=1
    break
  fi
  current_digest="$(shasum -a 256 "$src" "$rs_src" 2>/dev/null | shasum -a 256 | awk '{print $1}')"
  stored_digest="$(cat "$digest_file" 2>/dev/null || true)"
  if [ "$current_digest" != "$stored_digest" ]; then
    need_rebuild=1
    break
  fi
done

if [ "$need_rebuild" = 0 ]; then
  echo "==> xcframework up to date, skipping"
  exit 0
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

for t in "${TARGETS[@]}"; do
  slice_dir="$STAGE/$t"
  mkdir -p "$slice_dir/Headers"
  # Merge bedterm_core and bedterm-rs-terminal into a single .a per slice so
  # the xcframework remains a single-library bundle (as before).
  libtool -static -o "$slice_dir/$LIB_NAME" \
    "$RUST_DIR/target/$t/$PROFILE_DIR/libbedterm_core.a" \
    "$RUST_DIR/target/$t/$PROFILE_DIR/libbedterm_rs_terminal.a"
  # Compose the staged header: cbindgen-generated bedterm_core.h plus the
  # hand-written rs-terminal supplement (appended before the final #endif).
  python3 - "$HEADER" "$RS_TERMINAL_HEADER" "$slice_dir/Headers/bedterm_core.h" <<'PYEOF'
import sys, re
core_h = open(sys.argv[1]).read()
rs_h   = open(sys.argv[2]).read()
out_path = sys.argv[3]
# Strip the header guard closer from core_h, append rs_h body, re-close.
core_h = re.sub(r'\s*#endif\s*/\*\s*BEDTERM_CORE_H\s*\*/\s*$', '\n', core_h)
with open(out_path, 'w') as f:
    f.write(core_h)
    f.write('\n')
    f.write(rs_h)
    f.write('\n#endif  /* BEDTERM_CORE_H */\n')
PYEOF
  # No module.modulemap in the xcframework: the BedTermCoreC SwiftPM C target
  # in Sources/BedTermCoreC/include/ provides the module for Swift importers.
done

# Preserve the committed Info.plist by rebuilding into a staging dir and
# moving it into place atomically.
TMP_FW="$STAGE/BedTermCore.xcframework"
XCFW_ARGS=()
for t in "${TARGETS[@]}"; do
  XCFW_ARGS+=( -library "$STAGE/$t/$LIB_NAME" -headers "$STAGE/$t/Headers" )
done
xcodebuild -create-xcframework "${XCFW_ARGS[@]}" -output "$TMP_FW"

mkdir -p "$OUT_DIR"
rm -rf "$FW_DIR"
mv "$TMP_FW" "$FW_DIR"

# Write per-slice digest files so the idempotency check works on the next run.
for i in "${!TARGETS[@]}"; do
  src="$RUST_DIR/target/${TARGETS[$i]}/$PROFILE_DIR/$LIB_NAME"
  rs_src="$RUST_DIR/target/${TARGETS[$i]}/$PROFILE_DIR/$RS_LIB_NAME"
  digest_file="$FW_DIR/${SLICE_IDS[$i]}/.bedterm_input_digest"
  shasum -a 256 "$src" "$rs_src" 2>/dev/null | shasum -a 256 | awk '{print $1}' > "$digest_file"
done

# Keep the tracked header copy in sync so `import BedTermCoreC` stays correct.
# Use the composed (core + rs-terminal) header, same as the xcframework slices.
SWIFT_INCLUDE="$REPO_ROOT/BedTermKit/Sources/BedTermCoreC/include"
mkdir -p "$SWIFT_INCLUDE"
cp "$FW_DIR/ios-arm64-simulator/Headers/bedterm_core.h" "$SWIFT_INCLUDE/bedterm_core.h"
echo "==> updated $SWIFT_INCLUDE/bedterm_core.h"

echo "==> built $FW_DIR"
