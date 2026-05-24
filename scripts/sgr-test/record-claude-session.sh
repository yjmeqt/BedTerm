#!/usr/bin/env bash
# Record a real Claude Code interactive session and render to PNG.
#
# Usage:
#   ./record-claude-session.sh "fix the auth bug"
#   ./record-claude-session.sh "say hello" iphone17-promax
#   echo -e "prompt\n/exit" | ./record-claude-session.sh --stdin
#
# The --device flag ensures recording and rendering share the same
# viewport (pt), scale factor, font metrics, and cols×rows. A .meta.json
# sidecar is written alongside the .bin so bedterm-render can replay
# faithfully without re-deriving terminal dimensions.
#
# Output: /tmp/claude-session-<device>-<timestamp>.png

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE="$(cd "$SCRIPT_DIR/../../rust-core" && pwd)"
BIN_RECORD="$WORKSPACE/target/debug/bedterm-record"
BIN_RENDER="$WORKSPACE/target/debug/bedterm-render"
FONT_SIZE=14
TIMEOUT=180

device="iphone17"
stdin_mode=false
prompt=""
palette="tokyo-night"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --stdin) stdin_mode=true; shift ;;
        --device) device="$2"; shift 2 ;;
        --timeout) TIMEOUT="$2"; shift 2 ;;
        --font-size) FONT_SIZE="$2"; shift 2 ;;
        --palette) palette="$2"; shift 2 ;;
        iphone17|iphone17-promax|ipad-mini|ipad-pro13|mac) device="$1"; shift ;;
        *) prompt="$1"; shift ;;
    esac
done

timestamp=$(date +%H%M%S)
bin_file="/tmp/claude-session-${device}-${timestamp}.bin"
meta_file="/tmp/claude-session-${device}-${timestamp}.meta.json"
png_file="/tmp/claude-session-${device}-${timestamp}.png"

# ── Build if needed ────────────────────────────────────────────────────
if [[ ! -x "$BIN_RECORD" ]] || [[ ! -x "$BIN_RENDER" ]]; then
    echo "=== Building tools ==="
    cargo build --manifest-path "$WORKSPACE/Cargo.toml" -p bedterm-record -p bedterm_core 2>&1 | tail -2
fi

# ── Query actual cell size from the renderer ───────────────────────────
# We still query cell-size to report the actual font metrics, but the
# record tool now derives cols/rows itself from --device.
case "$device" in
    iphone17) scale=3.0 ;;
    iphone17-promax) scale=3.0 ;;
    ipad-mini) scale=2.0 ;;
    ipad-pro13) scale=2.0 ;;
    mac) scale=2.0 ;;
    *) scale=2.0 ;;
esac

read -r cell_w cell_h < <("$BIN_RENDER" cell-size --font-size "$FONT_SIZE" --scale "$scale" 2>/dev/null) || true
if [[ -n "$cell_w" && -n "$cell_h" ]]; then
    echo "=== Cell metrics: ${cell_w}x${cell_h}px (font=${FONT_SIZE}pt, scale=${scale}x) ==="
else
    echo "=== WARNING: could not query cell size ==="
fi

# Derive exact cols/rows from actual cell metrics so the recorded PTY
# wraps at the same column boundaries the renderer will use.
if [[ -n "$cell_w" && -n "$cell_h" ]]; then
    case "$device" in
        iphone17) vp_pt_w=400; vp_pt_h=850 ;;
        iphone17-promax) vp_pt_w=430; vp_pt_h=930 ;;
        ipad-mini) vp_pt_w=744; vp_pt_h=1133 ;;
        ipad-pro13) vp_pt_w=1024; vp_pt_h=1366 ;;
        mac) vp_pt_w=1200; vp_pt_h=800 ;;
        *) vp_pt_w=400; vp_pt_h=850 ;;
    esac
    vp_px_w=$(awk "BEGIN { printf \"%d\", $vp_pt_w * $scale }")
    vp_px_h=$(awk "BEGIN { printf \"%d\", $vp_pt_h * $scale }")
    exact_cols=$(awk "BEGIN { printf \"%d\", $vp_px_w / $cell_w }")
    exact_rows=$(awk "BEGIN { printf \"%d\", $vp_px_h / $cell_h }")
    echo "=== Exact terminal: ${exact_cols}×${exact_rows} (${vp_px_w}×${vp_px_h}px / ${cell_w}×${cell_h}px cell) ==="
fi

# ── Record ─────────────────────────────────────────────────────────────
echo "=== Recording Claude Code ==="
echo "  Device:    $device"
echo "  Font:      ${FONT_SIZE}pt"
echo "  Scale:     ${scale}x"
echo "  Palette:   $palette"
echo "  Timeout:   ${TIMEOUT}s"
echo "  Output:    $bin_file"
echo ""

record_args=(--cmd "claude" --stdin
    --device "$device"
    --font-size "$FONT_SIZE"
    --palette "$palette"
    --timeout "$TIMEOUT" -o "$bin_file")

# Pass exact cols/rows from cell metrics when available (overrides heuristic).
if [[ -n "${exact_cols:-}" && -n "${exact_rows:-}" ]]; then
    record_args+=(--cols "$exact_cols" --rows "$exact_rows")
fi

if $stdin_mode; then
    "$BIN_RECORD" "${record_args[@]}"
else
    if [[ -z "$prompt" ]]; then
        echo "error: provide a prompt or use --stdin"
        exit 1
    fi
    printf '%s\n/exit\n' "$prompt" | "$BIN_RECORD" "${record_args[@]}"
fi

echo ""
echo "=== Recorded: $(du -h "$bin_file" | cut -f1) ==="

# ── Render using the sidecar for faithful dimensions ───────────────────
echo "=== Rendering to PNG ==="
cat "$bin_file" | "$BIN_RENDER" blocks \
    --context "$meta_file" \
    --palette "$palette" \
    > "$png_file" 2>/dev/null

echo "  PNG: $png_file ($(du -h "$png_file" | cut -f1))"

# ── Verify ─────────────────────────────────────────────────────────────
python3 -c "
import struct, zlib
with open('$png_file','rb') as f:
    f.read(8); l,=struct.unpack('>I',f.read(4)); f.read(4+l+4)
    idat=b''
    while True:
        l,=struct.unpack('>I',f.read(4));ct=f.read(4);d=f.read(l);f.read(4)
        if ct==b'IDAT':idat+=d
        elif ct==b'IEND':break
    raw=zlib.decompress(idat)
    w,=struct.unpack('>I',f.read(4)[:4]) if False else (0,)
    uniq=len({bytes(raw[i:i+3]) for i in range(1,len(raw),4)})
    print(f'  Valid PNG: {len(raw)} bytes, {uniq} unique colors, bg=({raw[1]},{raw[2]},{raw[3]})')
"

# ── Open ───────────────────────────────────────────────────────────────
open "$png_file"
echo ""
echo "=== Done: $png_file ==="
