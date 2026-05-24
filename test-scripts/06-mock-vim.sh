#!/usr/bin/env bash
# Mock vim — exercises the alt-screen lifecycle without depending on vim
# being installed remotely. On entry: switch to alt-screen, hide cursor,
# paint a tilde column + status bar. On exit (q): restore main screen.
#
# What to verify in BedTerm while it runs:
#   1. Block list collapses, classic full-grid takes over.
#   2. mode chip in the HUD reads `alt`.
#   3. Composer disappears (alt-screen launcher mode).
#   4. Cursor-positioning escapes redraw in place, no scroll bleed.
#   5. On `q` we exit cleanly: HUD chip back to `block`, prompt restored,
#      block sealed with exit 0.
set -eu

cleanup() {
    # Show cursor, leave alt-screen, reset attributes.
    printf '\033[?25h\033[?1049l\033[0m'
}
trap cleanup EXIT INT TERM

# Enter alt-screen + hide cursor + clear.
printf '\033[?1049h\033[?25l\033[2J'

rows=$(tput lines 2>/dev/null || echo 24)
cols=$(tput cols 2>/dev/null || echo 80)
mode="NORMAL"
counter=0

draw() {
    # Tilde column down the left edge — vim's classic empty-buffer look.
    for r in $(seq 1 $(( rows - 2 ))); do
        printf '\033[%d;1H\033[34m~\033[0m' "$r"
    done
    # Title near the top.
    printf '\033[1;%dH\033[1;33m-- Mock Vim --\033[0m' \
        $(( cols / 2 - 7 ))
    printf '\033[3;%dH\033[2mPress i/Esc to toggle mode, j/k to bump counter, q to quit\033[0m' \
        $(( cols / 2 - 28 ))
    printf '\033[5;%dH\033[2mcounter = %d\033[0m' $(( cols / 2 - 6 )) "$counter"
    # Status line at the bottom.
    printf '\033[%d;1H\033[7m' "$rows"
    printf '%-*s' "$cols" " $mode | mock-vim | ${cols}x${rows} "
    printf '\033[0m'
    # Cursor parked just under the title.
    printf '\033[7;1H'
}

draw
while IFS= read -r -n1 -s key; do
    case "$key" in
        q) break ;;
        i) mode="INSERT"; draw ;;
        $'\e') mode="NORMAL"; draw ;;
        j) counter=$(( counter + 1 )); draw ;;
        k) counter=$(( counter - 1 )); draw ;;
    esac
done
