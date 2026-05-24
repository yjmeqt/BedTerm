#!/usr/bin/env bash
# Cross-platform render snapshot harness.
#
# Runs the same fixtures through both the macOS bedterm-render CLI and
# the iOS Simulator Metal renderer, producing comparable PNG outputs.
#
# Usage:
#   bash render-all-platforms.sh              # both platforms
#   bash render-all-platforms.sh --macos-only # macOS only
#   bash render-all-platforms.sh --ios-only   # iOS Simulator only
#
# Output: /tmp/renders-cross-platform/macos/ and .../ios/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
RUST_DIR="$PROJECT_DIR/rust-core"
OUTDIR_BASE="/tmp/renders-cross-platform"

MAC_OUTDIR="$OUTDIR_BASE/macos"
IOS_OUTDIR="$OUTDIR_BASE/ios"

RENDER_BIN="$RUST_DIR/target/debug/bedterm-render"

DEVICES=("iphone17" "ipad-pro13")
PALETTES=("bedterm-dark" "bedterm-light")
FONT_SIZE=14

mode_both=true
mode_macos=false
mode_ios=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --macos-only) mode_both=false; mode_macos=true; shift ;;
        --ios-only) mode_both=false; mode_ios=true; shift ;;
        *) echo "unknown flag: $1"; exit 1 ;;
    esac
done

if $mode_both; then
    mode_macos=true
    mode_ios=true
fi

echo "=== Cross-Platform Render Snapshots ==="
echo "Output: $OUTDIR_BASE"
echo ""

# ── Clean ────────────────────────────────────────────────────────────
rm -rf "$OUTDIR_BASE"
mkdir -p "$MAC_OUTDIR" "$IOS_OUTDIR"

# ══════════════════════════════════════════════════════════════════════
# macOS (bedterm-render CLI)
# ══════════════════════════════════════════════════════════════════════
if $mode_macos; then
    echo "─── macOS (bedterm-render) ───"

    (cd "$RUST_DIR" && cargo build --bin bedterm-render 2>&1 | tail -1)

    total_mac=0
    for script in "$SCRIPT_DIR"/sgr-*.sh; do
        name="$(basename "$script" .sh)"
        case "$name" in
            render-all-matrix|render-all-platforms|record-claude-session)
                continue ;;
        esac
        for device in "${DEVICES[@]}"; do
            for palette in "${PALETTES[@]}"; do
                out="$MAC_OUTDIR/${name}-${device}-${palette}-grid.png"
                printf '  -> %s\n' "$(basename "$out")"
                bash "$script" 2>/dev/null \
                    | "$RENDER_BIN" grid \
                        --device "$device" --palette "$palette" \
                        --font-size "$FONT_SIZE" \
                    > "$out" 2>/dev/null \
                    || echo "    FAILED: $name $device $palette"
                total_mac=$((total_mac + 1))
            done
        done
    done

    echo ""
    echo "macOS: $(ls "$MAC_OUTDIR"/*.png 2>/dev/null | wc -l | tr -d ' ') / $total_mac PNGs"
    echo ""
fi

# ══════════════════════════════════════════════════════════════════════
# iOS Simulator
# ══════════════════════════════════════════════════════════════════════
if $mode_ios; then
    echo "─── iOS Simulator ───"

    # Regenerate SGR .bin fixtures into the test bundle's Fixtures directory
    # so they're available via Bundle.module on the simulator.
    TEST_FIXTURES_DIR="$PROJECT_DIR/BedTermKit/Tests/BedTermKitTests/Fixtures/byte_streams"
    mkdir -p "$TEST_FIXTURES_DIR"

    for script in "$SCRIPT_DIR"/sgr-*.sh; do
        name="$(basename "$script" .sh)"
        case "$name" in
            render-all-matrix|render-all-platforms|record-claude-session)
                continue ;;
        esac
        bash "$script" 2>/dev/null > "$TEST_FIXTURES_DIR/${name}.bin" \
            || echo "    FAILED: gen fixture $name"
    done
    echo "Fixtures: $(ls "$TEST_FIXTURES_DIR"/sgr-*.bin 2>/dev/null | wc -l | tr -d ' ') SGR + $(ls "$TEST_FIXTURES_DIR"/0*.bin 2>/dev/null | wc -l | tr -d ' ') base"

    # Run iOS snapshot tests
    export RENDER_OUTPUT_DIR="$IOS_OUTDIR"

    if command -v worktree-ios-dev-tool &>/dev/null; then
        echo "Running via worktree-ios-dev-tool..."
        worktree-ios-dev-tool test \
            --only-testing "BedTermKitTests/TerminalRendererSnapshotTests" \
            2>&1 | tail -20
    else
        echo "Running via xcodebuild..."
        xcodebuild test \
            -scheme BedTerm \
            -destination 'platform=iOS Simulator,name=iPhone 17' \
            -only-testing "BedTermKitTests/TerminalRendererSnapshotTests" \
            2>&1 | tail -20
    fi

    echo ""
    ios_count=$(ls "$IOS_OUTDIR"/*.png 2>/dev/null | wc -l | tr -d ' ')
    echo "iOS: $ios_count PNGs"
    if [[ "$ios_count" -gt 0 ]]; then
        echo "  Sample: $(ls "$IOS_OUTDIR"/*.png 2>/dev/null | head -3 | xargs -n1 basename)"
    fi
    echo ""
fi

# ══════════════════════════════════════════════════════════════════════
# Summary
# ══════════════════════════════════════════════════════════════════════
echo "=== Done ==="
if $mode_macos; then
    echo "macOS: $(ls "$MAC_OUTDIR"/*.png 2>/dev/null | wc -l | tr -d ' ') PNGs → $MAC_OUTDIR"
fi
if $mode_ios; then
    echo "iOS:   $(ls "$IOS_OUTDIR"/*.png 2>/dev/null | wc -l | tr -d ' ') PNGs → $IOS_OUTDIR"
fi
echo ""
echo "Compare:"
echo "  open $MAC_OUTDIR/*.png"
echo "  open $IOS_OUTDIR/*.png"
