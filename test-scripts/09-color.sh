#!/usr/bin/env bash
# Three palette tiers — basic / 256 / 24-bit. Each row should render with
# distinct foreground colours; a regression in palette resolution will
# collapse swaths of cells to the default fg.
set -eu

printf '16-color ANSI:\n  '
for c in 30 31 32 33 34 35 36 37 90 91 92 93 94 95 96 97; do
    printf '\033[%dmA\033[0m' "$c"
done
printf '\n\n256-color palette (16-231 cube):\n'
for ((i=16; i<232; i++)); do
    printf '\033[38;5;%dmA\033[0m' "$i"
    if (( (i - 15) % 36 == 0 )); then printf '\n'; fi
done
printf '\n\n24-bit truecolor sweep:\n'
for ((r=0; r<256; r+=32)); do
    for ((g=0; g<256; g+=32)); do
        printf '\033[38;2;%d;%d;%dmA\033[0m' "$r" "$g" "$((255-r))"
    done
    printf '\n'
done
