#!/usr/bin/env bash
# Quick visual check for SGR rendering. Prints each style on its own
# line so it's obvious which one is broken if a row looks wrong.
#
# Run remotely:
#   bash sgr-basic.sh
#
# Expected:
#   - bold row glyphs are visibly heavier than regular
#   - italic row glyphs are slanted (or near-identical if italic face missing)
#   - underline row has a hairline under each glyph, full-cell width
#   - inverse row has fg / bg swapped (text bg becomes fg's colour)
#   - bold+italic combines both
#   - colours interact with bold (bright vs heavy is the only difference)

esc=$'\033'

printf '%sregular%s   the quick brown fox\n' "${esc}[0m" "${esc}[0m"
printf '%sbold%s      the quick brown fox\n' "${esc}[1m" "${esc}[0m"
printf '%sitalic%s    the quick brown fox\n' "${esc}[3m" "${esc}[0m"
printf '%sunderline%s the quick brown fox\n' "${esc}[4m" "${esc}[0m"
printf '%sinverse%s   the quick brown fox\n' "${esc}[7m" "${esc}[0m"
printf '%sstrike%s    the quick brown fox\n' "${esc}[9m" "${esc}[0m"
printf '%sbold+ital%s the quick brown fox\n' "${esc}[1;3m" "${esc}[0m"
printf '%sbold+und%s  the quick brown fox\n' "${esc}[1;4m" "${esc}[0m"
printf '%sbold+inv%s  the quick brown fox\n' "${esc}[1;7m" "${esc}[0m"
printf '%sbold+strk%s the quick brown fox\n' "${esc}[1;9m" "${esc}[0m"

printf '\n--- colour x style matrix ---\n'
for code in 31 32 33 34 35 36; do
    printf '%sfg %d regular%s | %sfg %d bold%s | %sfg %d italic%s | %sfg %d underline%s | %sfg %d inverse%s\n' \
        "${esc}[${code}m" "$code" "${esc}[0m" \
        "${esc}[1;${code}m" "$code" "${esc}[0m" \
        "${esc}[3;${code}m" "$code" "${esc}[0m" \
        "${esc}[4;${code}m" "$code" "${esc}[0m" \
        "${esc}[7;${code}m" "$code" "${esc}[0m"
done

printf '\n--- reset between attrs (SGR 22 / 23 / 24 / 27) ---\n'
printf '%sbold-on %s%snormal-weight %s%sbold-on-again%s\n' \
    "${esc}[1m" "${esc}[22m" "" "${esc}[1m" "" "${esc}[0m"
printf '%sitalic-on %s%supright %s%sitalic-on-again%s\n' \
    "${esc}[3m" "${esc}[23m" "" "${esc}[3m" "" "${esc}[0m"
printf '%sunder-on %s%snone %s%sunder-on-again%s\n' \
    "${esc}[4m" "${esc}[24m" "" "${esc}[4m" "" "${esc}[0m"
printf '%sinverse-on %s%snormal %s%sinverse-on-again%s\n' \
    "${esc}[7m" "${esc}[27m" "" "${esc}[7m" "" "${esc}[0m"
printf '%sstrike-on %s%snone %s%sstrike-on-again%s\n' \
    "${esc}[9m" "${esc}[29m" "" "${esc}[9m" "" "${esc}[0m"
