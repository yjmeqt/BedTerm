#!/usr/bin/env bash
# Builds bedterm_core for iOS device + iOS sim + macOS, then assembles
# BedTermCore.xcframework consumed by BedTermKit as a binary target.
#
# Run from repo root:  ./scripts/build-rust-xcframework.sh [debug|release]
set -euo pipefail

PROFILE="${1:-release}"
PROFILE="${PROFILE#--}"
CARGO_PROFILE_ARG=()
PROFILE_DIR="debug"
if [ "$PROFILE" = "release" ]; then
  CARGO_PROFILE_ARG=(--release)
  PROFILE_DIR="release"
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUST_DIR="$REPO_ROOT/rust-core"
OUT_DIR="$REPO_ROOT/BedTermKit/BinaryFrameworks"
FW_NAME="BedTermCore"
LIB_NAME="libbedterm_core.a"
HEADER="$RUST_DIR/bedterm_core/include/bedterm_core.h"

TARGETS=(
  "aarch64-apple-ios"
  "aarch64-apple-ios-sim"
  "aarch64-apple-darwin"
)

cd "$RUST_DIR"
for t in "${TARGETS[@]}"; do
  echo "==> cargo build --target $t"
  cargo build -p bedterm_core "${CARGO_PROFILE_ARG[@]+"${CARGO_PROFILE_ARG[@]}"}" --target "$t"
done

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

rm -rf "$OUT_DIR/$FW_NAME.xcframework"
mkdir -p "$OUT_DIR"

XCFW_ARGS=()
for t in "${TARGETS[@]}"; do
  XCFW_ARGS+=( -library "$STAGE/$t/$LIB_NAME" -headers "$STAGE/$t/Headers" )
done

xcodebuild -create-xcframework "${XCFW_ARGS[@]}" -output "$OUT_DIR/$FW_NAME.xcframework"

# Keep the tracked header copy in sync so `import BedTermCoreC` stays correct.
SWIFT_INCLUDE="$REPO_ROOT/BedTermKit/Sources/BedTermCoreC/include"
mkdir -p "$SWIFT_INCLUDE"
cp "$HEADER" "$SWIFT_INCLUDE/bedterm_core.h"
echo "==> updated $SWIFT_INCLUDE/bedterm_core.h"

echo "==> built $OUT_DIR/$FW_NAME.xcframework"
