#!/usr/bin/env bash
# Render all SGR test scripts × device size matrix via bedterm-render CLI.
#
# Output: /tmp/renders/<script>-<device>-<mode>.png
#
# Usage:
#   bash render-all-matrix.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RENDER_BIN="$(cd "$SCRIPT_DIR/../../rust-core" && pwd)/target/debug/bedterm-render"
OUTDIR="/tmp/renders"

# Device size matrix (width×height in pixels, font_size=14 → cell_w=7, cell_h=14)
declare -A DEVICES=(
    ["iphone17"]="400x850"
    ["iphone17-promax"]="430x930"
    ["ipad-mini"]="750x1130"
    ["ipad-pro13"]="1030x1380"
)

PALETTES=("tokyo-night" "dracula" "gruvbox-dark")

mkdir -p "$OUTDIR"
rm -f "$OUTDIR"/*.png

render_one() {
    local script="$1"
    local device="$2"
    local dims="$3"
    local palette="$4"
    local mode="${5:-grid}"
    local basename
    basename="$(basename "$script" .sh)"

    local extra_args=()
    if [ "$mode" = "blocks" ]; then
        extra_args=(--palette "$palette")
    else
        extra_args=(--palette "$palette")
    fi

    local out="$OUTDIR/${basename}-${device}-${palette}-${mode}.png"
    echo "  → $out"

    if [ "$mode" = "blocks" ]; then
        bash "$script" 2>/dev/null | "$RENDER_BIN" blocks \
            --font-size 14 --viewport "$dims" "${extra_args[@]}" \
            > "$out" 2>/dev/null || echo "    (blocks mode failed, may need DCS events)"
    else
        bash "$script" 2>/dev/null | "$RENDER_BIN" grid \
            --font-size 14 --viewport "$dims" "${extra_args[@]}" \
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
    echo ""
    echo "--- $name ---"

    # Determine mode: claude-code-session uses blocks, others use grid
    mode="grid"
    if [ "$name" = "claude-code-session" ]; then
        mode="blocks"
    fi

    for device in "${!DEVICES[@]}"; do
        dims="${DEVICES[$device]}"
        # Use first palette only for speed; all-palette rendering is a separate matrix
        palette="${PALETTES[0]}"
        render_one "$script" "$device" "$dims" "$palette" "$mode"
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
