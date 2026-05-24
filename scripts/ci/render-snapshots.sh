#!/usr/bin/env bash
# CI render snapshot harness.
#
# Generates PNG renders from SGR fixture scripts on macOS (bedterm-render CLI)
# and iOS Simulator (xcodebuild test), then collects them into a single artifact
# directory.
#
# Usage:
#   bash scripts/ci/render-snapshots.sh           # both platforms
#   bash scripts/ci/render-snapshots.sh --macos    # macOS only
#   bash scripts/ci/render-snapshots.sh --ios      # iOS only
#
# Output: /tmp/ci-renders/<platform>/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
RUST_DIR="$PROJECT_DIR/rust-core"
OUTDIR_BASE="/tmp/ci-renders"

MAC_OUTDIR="$OUTDIR_BASE/macos"
IOS_OUTDIR="$OUTDIR_BASE/ios"

RENDER_BIN="$RUST_DIR/target/debug/bedterm-render"
FIXTURES_DIR="$PROJECT_DIR/BedTermKit/Tests/BedTermKitTests/Fixtures/byte_streams"
SGR_SCRIPTS_DIR="$PROJECT_DIR/scripts/sgr-test"

DEVICES=("iphone17" "ipad-pro13")
PALETTES=("bedterm-dark" "bedterm-light")
FONT_SIZE=14

mode_macos=false
mode_ios=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --macos) mode_macos=true; shift ;;
        --ios) mode_ios=true; shift ;;
        *) mode_macos=true; mode_ios=true; shift ;;
    esac
done

echo "=== CI Render Snapshots ==="
echo ""

# ── Clean ────────────────────────────────────────────────────────────
rm -rf "$OUTDIR_BASE"
mkdir -p "$MAC_OUTDIR" "$IOS_OUTDIR"

# ── Regenerate SGR fixtures ──────────────────────────────────────────
echo "--- Regenerating SGR fixtures ---"
mkdir -p "$FIXTURES_DIR"

for script in "$SGR_SCRIPTS_DIR"/sgr-*.sh; do
    name="$(basename "$script" .sh)"
    case "$name" in
        render-all-matrix|render-all-platforms|record-claude-session) continue ;;
    esac
    bash "$script" > "$FIXTURES_DIR/${name}.bin" 2>/dev/null \
        || echo "  WARNING: fixture gen failed for $name"
done
echo "Fixtures: $(ls "$FIXTURES_DIR"/*.bin 2>/dev/null | wc -l | tr -d ' ') .bin files"
echo ""

# ══════════════════════════════════════════════════════════════════════
# macOS
# ══════════════════════════════════════════════════════════════════════
if $mode_macos; then
    echo "--- macOS (bedterm-render) ---"

    (cd "$RUST_DIR" && cargo build --bin bedterm-render 2>&1 | tail -1)

    total=0
    failed=0
    for script in "$SGR_SCRIPTS_DIR"/sgr-*.sh; do
        name="$(basename "$script" .sh)"
        case "$name" in
            render-all-matrix|render-all-platforms|record-claude-session)
                continue ;;
        esac
        for device in "${DEVICES[@]}"; do
            for palette in "${PALETTES[@]}"; do
                out="$MAC_OUTDIR/${name}-${device}-${palette}-grid.png"
                if bash "$script" 2>/dev/null \
                    | "$RENDER_BIN" grid \
                        --device "$device" --palette "$palette" \
                        --font-size "$FONT_SIZE" \
                    > "$out" 2>/dev/null; then
                    total=$((total + 1))
                else
                    echo "  FAILED: $name $device $palette"
                    failed=$((failed + 1))
                fi
            done
        done
    done

    echo "macOS: $total rendered, $failed failed"
    echo ""
fi

# ══════════════════════════════════════════════════════════════════════
# iOS Simulator
# ══════════════════════════════════════════════════════════════════════
if $mode_ios; then
    echo "--- iOS Simulator ---"

    # Check if simulator runtime is available
    if ! xcrun simctl list runtimes 2>/dev/null | grep -q "iOS"; then
        echo "SKIPPED: no iOS simulator runtime available"
    elif command -v worktree-ios-dev-tool &>/dev/null; then
        export RENDER_OUTPUT_DIR="$IOS_OUTDIR"
        worktree-ios-dev-tool test \
            --only-testing "BedTermKitTests/TerminalRendererSnapshotTests" \
            2>&1 | tail -5
        echo "iOS: $(ls "$IOS_OUTDIR"/*.png 2>/dev/null | wc -l | tr -d ' ') PNGs"
    else
        echo "SKIPPED: worktree-ios-dev-tool not available"
    fi
    echo ""
fi

# ── Summary ──────────────────────────────────────────────────────────
echo "=== Done ==="
echo "macOS: $(ls "$MAC_OUTDIR"/*.png 2>/dev/null | wc -l | tr -d ' ') PNGs"
echo "iOS:   $(ls "$IOS_OUTDIR"/*.png 2>/dev/null | wc -l | tr -d ' ') PNGs"
echo "Output: $OUTDIR_BASE"
