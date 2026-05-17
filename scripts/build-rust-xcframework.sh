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

# Accept Debug/Release (Xcode `$CONFIGURATION`) and debug/release.
PROFILE_RAW="${1:-release}"
PROFILE_RAW="${PROFILE_RAW#--}"
PROFILE="$(printf '%s' "$PROFILE_RAW" | tr '[:upper:]' '[:lower:]')"
CARGO_PROFILE_ARG=()
PROFILE_DIR="debug"
if [ "$PROFILE" = "release" ]; then
  CARGO_PROFILE_ARG=(--release)
  PROFILE_DIR="release"
fi

# Debug builds expose the in-process mock TTY (`bt_mock_tty_*`) used by the
# Swift `RustMockTTYClient` in `#if DEBUG`. Release builds omit the symbols.
CARGO_FEATURE_ARGS=()
if [ "$PROFILE" != "release" ]; then
  CARGO_FEATURE_ARGS+=( --features mock-tty )
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUST_DIR="$REPO_ROOT/rust-core"
OUT_DIR="$REPO_ROOT/BedTermKit/BinaryFrameworks"
FW_DIR="$OUT_DIR/BedTermCore.xcframework"
LIB_NAME="libbedterm_core.a"
HEADER="$RUST_DIR/bedterm_core/include/bedterm_core.h"

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
  echo "==> cargo build --target $t ($PROFILE)"
  cargo build -p bedterm_core \
    "${CARGO_PROFILE_ARG[@]+"${CARGO_PROFILE_ARG[@]}"}" \
    "${CARGO_FEATURE_ARGS[@]+"${CARGO_FEATURE_ARGS[@]}"}" \
    --target "$t"
done

# Fast-path: if every slice's staged .a matches the just-built one, skip the
# expensive xcframework rebuild. cargo is already incremental; this avoids the
# `-create-xcframework` step rewriting the directory on every Xcode build.
need_rebuild=0
for i in "${!TARGETS[@]}"; do
  src="$RUST_DIR/target/${TARGETS[$i]}/$PROFILE_DIR/$LIB_NAME"
  staged="$FW_DIR/${SLICE_IDS[$i]}/$LIB_NAME"
  if [ ! -f "$staged" ] || ! cmp -s "$src" "$staged"; then
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
  cp "$RUST_DIR/target/$t/$PROFILE_DIR/$LIB_NAME" "$slice_dir/"
  cp "$HEADER" "$slice_dir/Headers/"
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

# Keep the tracked header copy in sync so `import BedTermCoreC` stays correct.
SWIFT_INCLUDE="$REPO_ROOT/BedTermKit/Sources/BedTermCoreC/include"
mkdir -p "$SWIFT_INCLUDE"
cp "$HEADER" "$SWIFT_INCLUDE/bedterm_core.h"
echo "==> updated $SWIFT_INCLUDE/bedterm_core.h"

echo "==> built $FW_DIR"
