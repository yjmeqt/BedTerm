#!/usr/bin/env bash
# Builds bedterm-ios (the Swift-facing crate, which links bedterm-core in
# transitively as a workspace dep) for iOS device + iOS sim + macOS, then
# assembles BedTermIOS.xcframework consumed by BedTermKit as a binary target.
#
# The xcframework and its SwiftPM module are named `BedTermIOS`, matching
# the `bedterm-ios` crate. The static library is `libbedterm_ios.a` (which
# transitively contains all of `bedterm-core`'s symbols by Rust staticlib
# linkage), and the staged header is a straight copy of cbindgen's
# `bedterm_ios.h` output — cbindgen on `bedterm-ios` generates declarations
# for both crates (its `parse.parse_deps` + `parse.extra_bindings`
# whitelist walks into `bedterm-core`).
#
# Run from repo root or via Xcode pre-action / build phase:
#     ./scripts/build-rust-xcframework.sh [debug|release|Debug|Release] [PLATFORM_NAME]
#
# Second arg is Xcode's `$PLATFORM_NAME` (iphonesimulator | iphoneos | macosx).
# When given, the script builds ONLY the matching slice — sim-only test runs
# (e.g. CI) skip the device + macOS slices entirely. Empty / unrecognized
# value falls back to building all three slices (release / archive path).
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
FW_DIR="$OUT_DIR/BedTermIOS.xcframework"
LIB_NAME="libbedterm_ios.a"
# Single source of truth: cbindgen, configured on `bedterm-ios`, walks
# into the `bedterm-core` path-dep and emits both `bt_term_*` (core) and
# `bt_ios_*` (ui) declarations into one file.
HEADER="$RUST_DIR/bedterm-ios/include/bedterm_ios.h"

# rust-target → xcframework slice id.
ALL_TARGETS=(
  "aarch64-apple-ios"
  "aarch64-apple-ios-sim"
  "aarch64-apple-darwin"
)
ALL_SLICE_IDS=(
  "ios-arm64"
  "ios-arm64-simulator"
  "macos-arm64"
)

# Narrow the build set to one slice when Xcode tells us which platform it's
# building for. Empty / unrecognized value → keep all three (release path).
PLATFORM_NAME_RAW="${2:-}"
case "$PLATFORM_NAME_RAW" in
  iphonesimulator) TARGETS=("aarch64-apple-ios-sim"); SLICE_IDS=("ios-arm64-simulator") ;;
  iphoneos)        TARGETS=("aarch64-apple-ios");      SLICE_IDS=("ios-arm64") ;;
  macosx)          TARGETS=("aarch64-apple-darwin");   SLICE_IDS=("macos-arm64") ;;
  *)               TARGETS=("${ALL_TARGETS[@]}");      SLICE_IDS=("${ALL_SLICE_IDS[@]}") ;;
esac
echo "==> building slices: ${SLICE_IDS[*]} (PLATFORM_NAME='${PLATFORM_NAME_RAW}')"

cd "$RUST_DIR"
# Building `bedterm-ios` triggers its `build.rs` cbindgen step, which
# walks both this crate and the `bedterm-core` path-dep and regenerates
# `bedterm-ios/include/bedterm_ios.h` (consumed below as $HEADER).
for t in "${TARGETS[@]}"; do
  echo "==> cargo build -p bedterm-ios --target $t ($PROFILE)"
  cargo build -p bedterm-ios \
    "${CARGO_PROFILE_ARG[@]+"${CARGO_PROFILE_ARG[@]}"}" \
    --target "$t"
done

# Fast-path: skip the expensive xcframework rebuild when the staged .a
# is current. We track freshness via a sidecar SHA256 digest file stored
# next to the staged .a. The digest covers the input .a so any change
# triggers a rebuild.
need_rebuild=0
for i in "${!TARGETS[@]}"; do
  src="$RUST_DIR/target/${TARGETS[$i]}/$PROFILE_DIR/$LIB_NAME"
  digest_file="$FW_DIR/${SLICE_IDS[$i]}/.bedterm_input_digest"
  staged="$FW_DIR/${SLICE_IDS[$i]}/$LIB_NAME"
  if [ ! -f "$staged" ] || [ ! -f "$digest_file" ]; then
    need_rebuild=1
    break
  fi
  current_digest="$(shasum -a 256 "$src" 2>/dev/null | awk '{print $1}')"
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
  cp "$RUST_DIR/target/$t/$PROFILE_DIR/$LIB_NAME" "$slice_dir/$LIB_NAME"
  cp "$HEADER" "$slice_dir/Headers/bedterm_ios.h"
  # Module map so Swift can `import BedTermIOS` directly — no wrapper C target.
  cat > "$slice_dir/Headers/module.modulemap" <<EOF
module BedTermIOS {
    header "bedterm_ios.h"
    export *
}
EOF
done

# Preserve the committed Info.plist by rebuilding into a staging dir and
# moving it into place atomically.
TMP_FW="$STAGE/BedTermIOS.xcframework"
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
  digest_file="$FW_DIR/${SLICE_IDS[$i]}/.bedterm_input_digest"
  shasum -a 256 "$src" 2>/dev/null | awk '{print $1}' > "$digest_file"
done

echo "==> built $FW_DIR"
