#!/usr/bin/env bash
# Carriage-return progress bar — must overwrite in place, not stack rows.
# The block body should end up showing only "Done!" + a final 100% bar,
# not 30 stacked intermediate lines. Bar width is deliberately narrower
# than the PTY's column count so the line never auto-wraps (which on
# alacritty would push later content to a fresh row regardless of CR).
set -eu
total=30
for i in $(seq 1 $total); do
    pct=$(( i * 100 / total ))
    bar=$(printf '%*s' "$i" '' | tr ' ' '#')
    pad=$(printf '%*s' "$(( total - i ))" '')
    printf '\r\033[2K[%s%s] %3d%%' "$bar" "$pad" "$pct"
    sleep 0.05
done
printf '\nDone!\n'
