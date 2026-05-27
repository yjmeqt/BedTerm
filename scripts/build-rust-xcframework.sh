#!/usr/bin/env bash
# Builds bedterm_ios (the Swift-facing crate, which links bedterm_core in
# transitively as a workspace dep) for iOS device + iOS sim + macOS, then
# assembles BedTermCore.xcframework consumed by BedTermKit as a binary target.
#
# The xcframework keeps its legacy `BedTermCore.xcframework` name and its
# legacy `BedTermCoreC` SwiftPM module / `bedterm_core.h` header name to
# avoid churning every `import BedTermCoreC` call site in Swift. The
# contents, however, are now produced from `bedterm_ios`: the static
# library is `libbedterm_ios.a` (which transitively contains all of
# `bedterm_core`'s symbols by Rust staticlib linkage), and the staged
# header is `cbindgen(bedterm_core)` + the hand-written `bedterm_ios.h`
# FFI fragment appended before the include-guard closer.
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
# The xcframework slice library is now `libbedterm_ios.a`. Because
# `bedterm_ios` depends on `bedterm_core` (path dep) and is built as a
# `staticlib`, Rust links bedterm_core's object files into the produced
# `.a` automatically, so the single staticlib carries every `bt_*` /
# `bedterm_*` symbol Swift currently calls. We keep the staged file name
# inside the xcframework as `libbedterm_core.a` (the legacy name the
# committed xcframework Info.plist references) to avoid editing
# Info.plist + Package.swift in this wave; logically it is just a copy
# of `libbedterm_ios.a`.
LIB_NAME="libbedterm_core.a"
IOS_LIB_NAME="libbedterm_ios.a"
HEADER="$RUST_DIR/bedterm_core/include/bedterm_core.h"
# Supplemental header with declarations for symbols `bedterm_ios` itself
# exports (the hand-written `bt_ios_*` C ABI fragment). cbindgen
# regenerates HEADER on every bedterm_core build, so we keep extra
# declarations in a separate file and cat them together into every
# staged copy.
IOS_UI_HEADER="$RUST_DIR/bedterm_ios/include/bedterm_ios.h"

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
# Building `bedterm_ios` transitively builds `bedterm_core` (path dep)
# and triggers its `build.rs` cbindgen step, which keeps
# `bedterm_core/include/bedterm_core.h` up to date.
for t in "${TARGETS[@]}"; do
  echo "==> cargo build -p bedterm_ios --target $t ($PROFILE)"
  cargo build -p bedterm_ios \
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
  ios_src="$RUST_DIR/target/${TARGETS[$i]}/$PROFILE_DIR/$IOS_LIB_NAME"
  digest_file="$FW_DIR/${SLICE_IDS[$i]}/.bedterm_input_digest"
  staged="$FW_DIR/${SLICE_IDS[$i]}/$LIB_NAME"
  if [ ! -f "$staged" ] || [ ! -f "$digest_file" ]; then
    need_rebuild=1
    break
  fi
  current_digest="$(shasum -a 256 "$ios_src" 2>/dev/null | awk '{print $1}')"
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
  # `libbedterm_ios.a` already contains all of bedterm_core's object
  # files (Rust staticlib linkage), so we copy it straight in under the
  # legacy `libbedterm_core.a` slice filename — no libtool merge needed.
  cp "$RUST_DIR/target/$t/$PROFILE_DIR/$IOS_LIB_NAME" "$slice_dir/$LIB_NAME"
  # Compose the staged header: cbindgen-generated bedterm_core.h plus the
  # hand-written bedterm_ios supplement (appended before the final #endif).
  python3 - "$HEADER" "$IOS_UI_HEADER" "$slice_dir/Headers/bedterm_core.h" <<'PYEOF'
import sys, re
core_h = open(sys.argv[1]).read()
ios_h   = open(sys.argv[2]).read()
out_path = sys.argv[3]
# Strip the header guard closer from core_h, append ios_h body, re-close.
core_h = re.sub(r'\s*#endif\s*/\*\s*BEDTERM_CORE_H\s*\*/\s*$', '\n', core_h)
with open(out_path, 'w') as f:
    f.write(core_h)
    f.write('\n')
    f.write(ios_h)
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
  ios_src="$RUST_DIR/target/${TARGETS[$i]}/$PROFILE_DIR/$IOS_LIB_NAME"
  digest_file="$FW_DIR/${SLICE_IDS[$i]}/.bedterm_input_digest"
  shasum -a 256 "$ios_src" 2>/dev/null | awk '{print $1}' > "$digest_file"
done

# Keep the tracked header copy in sync so `import BedTermCoreC` stays correct.
# Use the composed (core + ios-ui) header, same as the xcframework slices.
SWIFT_INCLUDE="$REPO_ROOT/BedTermKit/Sources/BedTermCoreC/include"
mkdir -p "$SWIFT_INCLUDE"
cp "$FW_DIR/ios-arm64-simulator/Headers/bedterm_core.h" "$SWIFT_INCLUDE/bedterm_core.h"
echo "==> updated $SWIFT_INCLUDE/bedterm_core.h"

echo "==> built $FW_DIR"
