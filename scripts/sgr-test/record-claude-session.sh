#!/usr/bin/env bash
# Record a real Claude Code interactive session and render to PNG.
#
# Usage:
#   ./record-claude-session.sh "fix the auth bug"
#   ./record-claude-session.sh "say hello" iphone17-promax
#   echo -e "prompt\n/exit" | ./record-claude-session.sh --stdin
#
# Output: /tmp/claude-session-<device>-<timestamp>.png

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE="$(cd "$SCRIPT_DIR/../../rust-core" && pwd)"
BIN_RECORD="$WORKSPACE/target/debug/bedterm-record"
BIN_RENDER="$WORKSPACE/target/debug/bedterm-render"
FONT_SIZE=14
TIMEOUT=180

device_viewport() {
    case "${1:-iphone17}" in
        iphone17)         echo "400x850" ;;
        iphone17-promax)  echo "430x930" ;;
        ipad-mini)        echo "750x1130" ;;
        ipad-pro13)       echo "1030x1380" ;;
        *)                echo "400x850" ;;
    esac
}

device="iphone17"
stdin_mode=false
prompt=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --stdin) stdin_mode=true; shift ;;
        --device) device="$2"; shift 2 ;;
        --timeout) TIMEOUT="$2"; shift 2 ;;
        --font-size) FONT_SIZE="$2"; shift 2 ;;
        iphone17|iphone17-promax|ipad-mini|ipad-pro13) device="$1"; shift ;;
        *) prompt="$1"; shift ;;
    esac
done

viewport="$(device_viewport "$device")"
timestamp=$(date +%H%M%S)
bin_file="/tmp/claude-session-${device}-${timestamp}.bin"
png_file="/tmp/claude-session-${device}-${timestamp}.png"

# ── Build if needed ────────────────────────────────────────────────────
if [[ ! -x "$BIN_RECORD" ]] || [[ ! -x "$BIN_RENDER" ]]; then
    echo "=== Building tools ==="
    cargo build --manifest-path "$WORKSPACE/Cargo.toml" -p bedterm-record -p bedterm_core 2>&1 | tail -2
fi

# ── Query actual cell size from the renderer ───────────────────────────
read -r cell_w cell_h < <("$BIN_RENDER" cell-size --font-size "$FONT_SIZE" 2>/dev/null)
if [[ -n "$cell_w" && -n "$cell_h" ]]; then
    cols=$(awk "BEGIN { printf \"%d\", ($viewport%x) / $cell_w }" 2>/dev/null || echo "80")
    rows=$(awk "BEGIN { printf \"%d\", (${viewport#*x}) / $cell_h }" 2>/dev/null || echo "24")
    echo "=== Cell metrics: ${cell_w}x${cell_h}px → ${cols}×${rows} ==="
else
    cols=80
    rows=24
    echo "=== WARNING: could not query cell size, falling back to ${cols}x${rows} ==="
fi

# ── Record ─────────────────────────────────────────────────────────────
echo "=== Recording Claude Code ==="
echo "  Device:    $device ($viewport)"
echo "  Font:      ${FONT_SIZE}px"
echo "  Timeout:   ${TIMEOUT}s"
echo "  Output:    $bin_file"
echo ""

record_args=(--cmd "claude" --stdin
    --cols "$cols" --rows "$rows"
    --font-size "$FONT_SIZE" --viewport "$viewport"
    --timeout "$TIMEOUT" -o "$bin_file")

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

# ── Render ─────────────────────────────────────────────────────────────
echo "=== Rendering to PNG ==="
cat "$bin_file" | "$BIN_RENDER" blocks \
    --palette tokyo-night --font-size "$FONT_SIZE" --viewport "$viewport" \
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
    uniq=len({bytes(raw[i:i+3]) for i in range(1,len(raw),4)})
    print(f'  Valid PNG: {len(raw)} bytes, {uniq} unique colors, bg=({raw[1]},{raw[2]},{raw[3]})')
"

# ── Open ───────────────────────────────────────────────────────────────
open "$png_file"
echo ""
echo "=== Done: $png_file ==="
