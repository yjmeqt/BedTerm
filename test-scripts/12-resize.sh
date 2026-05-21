#!/usr/bin/env bash
# SIGWINCH reactivity: print live cols x rows whenever the window
# resizes. Toggle the keyboard / DPad in BedTerm to trigger resizes;
# the printed geometry should match what the HUD chip shows.
#
# `$COLUMNS` / `$LINES` aren't reliably set in script subshells, and
# `read -t` only takes integers on some bash builds, so we shell out to
# `tput` for geometry and poll on whole-second boundaries.
set -eu

report() {
    local c r
    c=$(tput cols 2>/dev/null || echo '?')
    r=$(tput lines 2>/dev/null || echo '?')
    printf '\r\033[2K geom=%sx%s (PID=%d)' "$c" "$r" "$$"
}
trap report WINCH

echo "Watching for SIGWINCH. Press q to exit."
report
while IFS= read -r -n1 -s -t 1 key 2>/dev/null || true; do
    [ "${key:-}" = "q" ] && break
    # Re-print every tick so a freshly arrived WINCH that fires between
    # reads still gets noticed quickly.
    report
done
echo
