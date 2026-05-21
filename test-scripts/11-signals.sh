#!/usr/bin/env bash
# SIGINT handling: a sleep-loop that catches Ctrl-C and reports the
# count. Use BedTerm's Ctrl latch chip (tap Ctrl, then 'c') to send
# the interrupt while the composer is in passthrough.
set -u

count=0
trap 'echo " [INT received after ${count} ticks]"; exit 0' INT

echo "Looping — tap composer Ctrl then 'c' to interrupt."
while :; do
    count=$(( count + 1 ))
    printf '\r  tick %d' "$count"
    sleep 0.25
done
