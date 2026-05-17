#!/bin/bash
# Build terminal_emu for iOS targets.
# Requires: rustup target add aarch64-apple-ios aarch64-apple-ios-sim
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Ensure iOS targets are installed
rustup target list --installed | grep -q aarch64-apple-ios || {
    echo "Installing aarch64-apple-ios target..."
    rustup target add aarch64-apple-ios
}
rustup target list --installed | grep -q aarch64-apple-ios-sim || {
    echo "Installing aarch64-apple-ios-sim target..."
    rustup target add aarch64-apple-ios-sim
}

TARGETS=(
    "aarch64-apple-ios"
    "aarch64-apple-ios-sim"
)

echo "Building terminal_emu for iOS..."
for target in "${TARGETS[@]}"; do
    echo "  → $target"
    cargo build --release --target "$target" 2>&1 | tail -1
done

# Output directories
HEADER_DIR="$SCRIPT_DIR/../../Sources/BedTermKit/CHeaders"
LIBS_DIR="$SCRIPT_DIR/../../libs"

mkdir -p "$HEADER_DIR"
mkdir -p "$LIBS_DIR"

# Generate C header (requires cbindgen: cargo install cbindgen)
if command -v cbindgen &>/dev/null; then
    cbindgen --config cbindgen.toml --output "$HEADER_DIR/terminal_emu.h"
    echo "  → header written to $HEADER_DIR/terminal_emu.h"
else
    echo "⚠  cbindgen not installed — skipping header generation."
    echo "   Install with: cargo install cbindgen"
fi

# Copy static libs
cp target/aarch64-apple-ios/release/libterminal_emu.a "$LIBS_DIR/libterminal_emu-ios.a"
cp target/aarch64-apple-ios-sim/release/libterminal_emu.a "$LIBS_DIR/libterminal_emu-ios-sim.a"
echo "  → libs written to $LIBS_DIR/"

echo "Done."
