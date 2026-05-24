#!/usr/bin/env bash
# Comprehensive render matrix — all scripts × all devices × both palettes
# × both modes (grid + blocks). Exercises every rendering path.
#
# Output: /tmp/renders/<script>-<device>-<palette>-<mode>.png
#
# Usage:
#   bash render-all-matrix.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RENDER_BIN="$(cd "$SCRIPT_DIR/../../rust-core" && pwd)/target/debug/bedterm-render"
OUTDIR="/tmp/renders"

DEVICES=(
    "iphone17"
    "iphone17-promax"
    "iphone16"
    "iphone15"
    "iphone14"
    "iphone-se3"
    "ipad-pro13"
    "ipad-mini"
)

PALETTES=("bedterm-dark" "bedterm-light")

# Scripts that use DCS events → always blocks mode
DCS_SCRIPTS=("claude-code-session" "long-running-block")

mkdir -p "$OUTDIR"
rm -f "$OUTDIR"/*.png

render_one() {
    local script="$1"
    local device="$2"
    local palette="$3"
    local mode="$4"
    local basename
    basename="$(basename "$script" .sh)"

    local out="$OUTDIR/${basename}-${device}-${palette}-${mode}.png"
    echo "  → $(basename "$out")"

    bash "$script" 2>/dev/null | "$RENDER_BIN" "$mode" \
        --device "$device" --palette "$palette" \
        > "$out" 2>/dev/null || echo "    FAILED"
}

echo "=== Render matrix ==="
echo "Devices:  ${#DEVICES[@]}"
echo "Palettes: ${PALETTES[*]}"
echo "Output:   $OUTDIR"
echo ""

# Build CLI
(cd "$SCRIPT_DIR/../../rust-core" && cargo build --bin bedterm-render 2>&1 | tail -1)

total=0
passed=0

for script in "$SCRIPT_DIR"/*.sh; do
    name="$(basename "$script" .sh)"
    # Skip wrapper / non-test scripts
    case "$name" in
        record-claude-session|render-all-matrix) continue ;;
    esac

    echo ""
    echo "─── $name ───"

    # Determine which modes to run
    is_dcs=false
    for dcs_name in "${DCS_SCRIPTS[@]}"; do
        [[ "$name" == "$dcs_name" ]] && is_dcs=true
    done

    modes=("grid")
    if $is_dcs; then
        modes=("blocks")             # DCS-only scripts → blocks only
    else
        modes=("grid" "blocks")      # SGR scripts → both modes
    fi

    for device in "${DEVICES[@]}"; do
        for palette in "${PALETTES[@]}"; do
            for mode in "${modes[@]}"; do
                # Blocks mode for non-DCS SGR scripts: wrap in a single block
                if [[ "$mode" == "blocks" && "$is_dcs" != true ]]; then
                    # Use --wrap to package the SGR output as a block
                    bash "$script" 2>/dev/null | "$RENDER_BIN" blocks \
                        --device "$device" --palette "$palette" --wrap \
                        --command "$name" --exit-code 0 \
                        > "$OUTDIR/${name}-${device}-${palette}-blocks.png" 2>/dev/null \
                        || echo "    FAILED: ${name}-${device}-${palette}-blocks"
                else
                    bash "$script" 2>/dev/null | "$RENDER_BIN" "$mode" \
                        --device "$device" --palette "$palette" \
                        > "$OUTDIR/${name}-${device}-${palette}-${mode}.png" 2>/dev/null \
                        || echo "    FAILED: ${name}-${device}-${palette}-${mode}"
                fi
                total=$((total + 1))
            done
        done
    done
done

# ── Sticky header test (scroll into 2nd block of Claude session) ──────
echo ""
echo "─── sticky-header ───"
for device in "${DEVICES[@]}"; do
    for palette in "${PALETTES[@]}"; do
        out="$OUTDIR/sticky-header-${device}-${palette}-blocks.png"
        echo "  → $(basename "$out")"
        bash "$SCRIPT_DIR/claude-code-session.sh" 2>/dev/null | \
            "$RENDER_BIN" blocks --device "$device" --palette "$palette" \
            --scroll-y-pt 140 \
            > "$out" 2>/dev/null || echo "    FAILED"
        total=$((total + 1))
    done
done

echo ""
echo "=== Done: $total renders ==="
echo ""
ls -lh "$OUTDIR"/*.png 2>/dev/null | wc -l | xargs echo "Files:"
echo ""
# Show a few representative sizes
echo "Sample sizes:"
ls -lh "$OUTDIR"/*.png 2>/dev/null | awk '{sum+=$5; count++} END {print "  total", count, "files,", sum/1024/1024, "MB"}'
echo ""
echo "Open all:"
echo "  open $OUTDIR/*.png"
