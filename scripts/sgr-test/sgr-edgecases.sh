#!/usr/bin/env bash
# Edge cases: nested styles, rapid toggling, zero-width, long lines,
# incomplete escapes, boundary conditions.
#
#   bash sgr-edgecases.sh | cargo run -p bedterm-core --bin bedterm-render grid > /tmp/sgr-edge.png

esc=$'\033'

# ── Deeply nested styles ───────────────────────────────────────────────
printf '=== nested style stack ===\n'
# Bold on, italic on, underline on, then peel back
printf 'plain '
printf '%sbold-on ' "${esc}[1m"
printf '%sitalic-on ' "${esc}[3m"
printf '%sunderline-on ' "${esc}[4m"
printf '%sred-on ' "${esc}[31m"
printf 'ALL FOUR '
printf '%sred-off ' "${esc}[39m"
printf '%sunder-off ' "${esc}[24m"
printf '%sitalic-off ' "${esc}[23m"
printf '%sbold-off%s' "${esc}[22m" "${esc}[0m"
printf ' plain-again\n'

# ── Rapid style toggling (stress test) ─────────────────────────────────
printf '\n=== rapid toggle ===\n'
for i in $(seq 1 20); do
    if [ $((i % 2)) -eq 0 ]; then
        printf '%s█%s' "${esc}[1;31m" "${esc}[0m"
    else
        printf '%s█%s' "${esc}[1;32m" "${esc}[0m"
    fi
done
printf '\n'
# Rapid attribute flip
for i in $(seq 1 40); do
    printf '%s%s%s' "${esc}[${i}m" "$i" "${esc}[0m"
done
printf '\n'

# ── SGR 0 (hard reset) at boundaries ───────────────────────────────────
printf '\n=== SGR reset boundaries ===\n'
printf '%sbold%s %s\033[0mplain%s should-be-plain\n' \
    "${esc}[1m" "" "" ""
printf '%sred-bold%s\033[0m plain after reset %sbold-again%s\n' \
    "${esc}[1;31m" "" "${esc}[1m" "${esc}[0m"
printf '\033[0mfully-reset\033[0m\n'

# ── Empty / null bytes ─────────────────────────────────────────────────
printf '\n=== zero-width and control chars ===\n'
printf 'before\000after-null\n'
printf 'before\033after-esc\n'
printf 'tab:\tbetween\ttabs\n'
printf 'bell:\007between\007bells\n'

# ── Very long line (no wrap, just render) ──────────────────────────────
printf '\n=== long line (200 chars) ===\n'
for i in $(seq 1 200); do
    if [ $((i % 10)) -eq 0 ]; then printf '%s█%s' "${esc}[31m" "${esc}[0m]"; else printf '.'; fi
done
printf '\n'

# ── Trailing style (no reset) ──────────────────────────────────────────
printf '\n=== trailing style (no reset) ===\n'
printf '%sbold-no-reset...' "${esc}[1m"
# Deliberately no reset — this should NOT bleed into PNG metadata ;)
printf '\n%snext line should be plain%s\n' "" ""

# ── CSI sequences at line boundaries ───────────────────────────────────
printf '\n=== CSI at boundaries ===\n'
printf '%s' "${esc}[1;31m"
printf 'red-bold right after CSI'
printf '%s\n' "${esc}[0m"
printf '%sstart-of-line-bold%s end\n' "${esc}[1m" "${esc}[0m"

printf '\n'
