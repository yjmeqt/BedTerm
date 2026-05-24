#!/usr/bin/env bash
# Render all SGR test scripts × device size matrix via bedterm-render CLI.
#
# Output: /tmp/renders/<script>-<device>-<palette>-<mode>.png
#
# Usage:
#   bash render-all-matrix.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RENDER_BIN="$(cd "$SCRIPT_DIR/../../rust-core" && pwd)/target/debug/bedterm-render"
OUTDIR="/tmp/renders"

# Device matrix — viewports in points, scale applied automatically.
# Each device produces a PNG at viewport_pt × scale pixels.
DEVICES=(
    "iphone17"
    "iphone17-promax"
    "ipad-mini"
    "ipad-pro13"
)

PALETTES=("tokyo-night" "dracula" "gruvbox-dark")

mkdir -p "$OUTDIR"
rm -f "$OUTDIR"/*.png

render_one() {
    local script="$1"
    local device="$2"
    local palette="$3"
    local mode="${4:-grid}"
    local basename
    basename="$(basename "$script" .sh)"

    local out="$OUTDIR/${basename}-${device}-${palette}-${mode}.png"
    echo "  → $out"

    if [ "$mode" = "blocks" ]; then
        bash "$script" 2>/dev/null | "$RENDER_BIN" blocks \
            --device "$device" --palette "$palette" \
            > "$out" 2>/dev/null || echo "    (blocks mode failed, may need DCS events)"
    else
        bash "$script" 2>/dev/null | "$RENDER_BIN" grid \
            --device "$device" --palette "$palette" \
            > "$out" 2>/dev/null || echo "    (grid render failed)"
    fi
}

echo "=== Rendering all scripts × all devices × palettes ==="
echo "Output directory: $OUTDIR"
echo "Binary: $RENDER_BIN"
echo ""

# Build CLI first
(cd "$SCRIPT_DIR/../../rust-core" && cargo build --bin bedterm-render 2>&1 | tail -1)

for script in "$SCRIPT_DIR"/*.sh; do
    name="$(basename "$script" .sh)"
    # Skip wrapper scripts that aren't test generators.
    case "$name" in
        record-claude-session|render-all-matrix) continue ;;
    esac
    echo ""
    echo "--- $name ---"

    # Determine mode: claude-code-session uses blocks, others use grid
    mode="grid"
    if [ "$name" = "claude-code-session" ]; then
        mode="blocks"
    fi

    for device in "${DEVICES[@]}"; do
        # Use first palette only for speed.
        palette="${PALETTES[0]}"
        render_one "$script" "$device" "$palette" "$mode"
    done
done

echo ""
echo "=== Done ==="
echo ""
echo "Rendered files:"
ls -lh "$OUTDIR"/*.png 2>/dev/null | awk '{print "  " $NF " (" $5 ")"}'
echo ""
echo "Open all:"
echo "  open $OUTDIR/*.png"
