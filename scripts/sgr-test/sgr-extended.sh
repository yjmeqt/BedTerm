#!/usr/bin/env bash
# Extended SGR: 256-color, true color, background colors, dim, blink, concealed,
# overline, double underline.
#
# Run locally and pipe to bedterm-render:
#   bash sgr-extended.sh | cargo run -p bedterm_core --bin bedterm-render grid --palette tokyo-night > /tmp/sgr-extended.png

esc=$'\033'

# ── 256-color foreground palette ───────────────────────────────────────
printf '\n=== 256-color foreground (38;5;N) ===\n'
for ((c=0; c<16; c++)); do
    printf '%sfg-%-3d%s ' "${esc}[38;5;${c}m" "$c" "${esc}[0m"
done
printf '\n'
for ((c=16; c<52; c++)); do
    printf '%sfg-%-3d%s ' "${esc}[38;5;${c}m" "$c" "${esc}[0m"
done
printf '\n'
for ((c=52; c<88; c++)); do
    printf '%sfg-%-3d%s ' "${esc}[38;5;${c}m" "$c" "${esc}[0m"
done
printf '\n'
# Grayscale stripe
for ((c=232; c<256; c++)); do
    printf '%sfg-%-3d%s ' "${esc}[38;5;${c}m" "$c" "${esc}[0m"
done
printf '\n'

# ── 256-color background palette ───────────────────────────────────────
printf '\n=== 256-color background (48;5;N) ===\n'
for ((c=0; c<16; c++)); do
    printf '%s bg-%-3d %s' "${esc}[48;5;${c}m" "$c" "${esc}[0m"
done
printf '\n'
for ((c=16; c<52; c++)); do
    printf '%s%-3d %s' "${esc}[48;5;${c}m" "$c" "${esc}[0m"
done
printf '\n'
for ((c=52; c<88; c++)); do
    printf '%s%-3d %s' "${esc}[48;5;${c}m" "$c" "${esc}[0m"
done
printf '\n'

# ── 24-bit true color ──────────────────────────────────────────────────
printf '\n=== 24-bit true color (38;2;R;G;B) ===\n'
printf '%sred gradient:%s ' "${esc}[1m" "${esc}[0m"
for r in 55 95 135 175 215 255; do
    printf '%s█%s' "${esc}[38;2;${r};50;50m" "${esc}[0m"
done
printf '\n'
printf '%sgreen gradient:%s ' "${esc}[1m" "${esc}[0m"
for g in 55 95 135 175 215 255; do
    printf '%s█%s' "${esc}[38;2;50;${g};50m" "${esc}[0m"
done
printf '\n'
printf '%sblue gradient:%s ' "${esc}[1m" "${esc}[0m"
for b in 55 95 135 175 215 255; do
    printf '%s█%s' "${esc}[38;2;50;50;${b}m" "${esc}[0m"
done
printf '\n'
# Rainbow text
printf '%smillions of colors%stest\n' \
    "${esc}[38;2;255;100;50m" "${esc}[0m"

# ── Foreground + background combinations ────────────────────────────────
printf '\n=== fg + bg combos ===\n'
printf '%s black on white   %s' "${esc}[30;47m" "${esc}[0m]"
printf '%s white on blue    %s' "${esc}[37;44m" "${esc}[0m]"
printf '%s yellow on red    %s' "${esc}[33;41m" "${esc}[0m]"
printf '%s cyan on green    %s' "${esc}[36;42m" "${esc}[0m]\n"
printf '%s 256-on-true: %stext%s\n' \
    "${esc}[48;5;235m" "${esc}[38;2;100;200;255m" "${esc}[0m"
printf '%s true-on-256: %stext%s\n' \
    "${esc}[48;2;30;30;50m" "${esc}[38;5;226m" "${esc}[0m"

# ── Dim / faint ────────────────────────────────────────────────────────
printf '\n=== dim (SGR 2) ===\n'
printf 'regular  %sdimmed%s  regular\n' "${esc}[2m" "${esc}[0m"
printf '%sbold dim%s  should be lighter than bold\n' "${esc}[1;2m" "${esc}[0m"
for code in 31 32 33 34; do
    printf '%sfg%d regular%s | %sfg%d dim%s\n' \
        "${esc}[${code}m" "$code" "${esc}[0m" \
        "${esc}[2;${code}m" "$code" "${esc}[0m"
done

# ── Blink / concealed ──────────────────────────────────────────────────
printf '\n=== blink (SGR 5) + concealed (SGR 8) ===\n'
printf '%sBLINKING TEXT%s (should be visible, slow blink in real terminal)\n' \
    "${esc}[5m" "${esc}[0m"
printf 'visible %sHIDDEN%s visible (concealed text should be invisible)\n' \
    "${esc}[8m" "${esc}[0m"
printf '%sbold blink%s combined\n' "${esc}[1;5m" "${esc}[0m"

# ── Double underline + overline ────────────────────────────────────────
printf '\n=== double underline (SGR 21) + overline (SGR 53) ===\n'
printf '%sdouble underline%s  regular\n' "${esc}[21m" "${esc}[0m"
printf '%soverline%s  regular\n' "${esc}[53m" "${esc}[0m"
printf '%soverline+underline%s  combined\n' "${esc}[53;4m" "${esc}[0m"

printf '\n'
