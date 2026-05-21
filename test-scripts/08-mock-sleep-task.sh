#!/usr/bin/env bash
# Long-running task with live timer + non-blocking control keys read from
# stdin. Validates that:
#   1. Composer goes into passthrough as soon as the script starts.
#   2. Single-character keys (p / r / q) really reach the running process
#      while the editor stays locked.
#   3. The timer's `\r` overwrites in place, not by scroll.
#   4. q exits gracefully with the correct elapsed time printed.
#
# `read -t` only takes integers on plain bash, so we work with whole-
# second timer ticks rather than 200ms polling. The trade-off is the
# script reacts to key presses on the next second boundary — still fast
# enough for a manual test.
set -eu

printf 'Controls: p=pause  r=resume  q=quit\n'
printf 'Working...\n'

paused=0
start=$SECONDS
pause_offset=0

cleanup() {
    elapsed=$(( SECONDS - start - pause_offset ))
    printf '\nStopped after %ds\n' "$elapsed"
}
trap cleanup EXIT

while :; do
    if IFS= read -r -t 1 -n1 -s key; then
        case "$key" in
            q) break ;;
            p)
                if [ $paused -eq 0 ]; then
                    paused=1
                    pause_start=$SECONDS
                    printf '\r\033[2K[paused] press r to resume'
                fi
                ;;
            r)
                if [ $paused -eq 1 ]; then
                    pause_offset=$(( pause_offset + SECONDS - pause_start ))
                    paused=0
                fi
                ;;
        esac
    fi
    if [ $paused -eq 0 ]; then
        elapsed=$(( SECONDS - start - pause_offset ))
        printf '\r\033[2K  elapsed=%ds' "$elapsed"
    fi
done
