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
  cat > "$slice_dir/Headers/module.modulemap" <<EOF
module BedTermCoreC {
    header "bedterm_core.h"
    export *
}
EOF
done

rm -rf "$OUT_DIR/$FW_NAME.xcframework"
mkdir -p "$OUT_DIR"

XCFW_ARGS=()
for t in "${TARGETS[@]}"; do
  XCFW_ARGS+=( -library "$STAGE/$t/$LIB_NAME" -headers "$STAGE/$t/Headers" )
done

xcodebuild -create-xcframework "${XCFW_ARGS[@]}" -output "$OUT_DIR/$FW_NAME.xcframework"

echo "==> built $OUT_DIR/$FW_NAME.xcframework"
