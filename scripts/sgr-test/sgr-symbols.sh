#!/usr/bin/env bash
# Visual check for Unicode symbol / geometric-shape / block-element rendering.
# Exercises the glyph-rasterizer's font-fallback cascade for codepoints
# beyond ASCII: Miscellaneous Technical, Geometric Shapes, Block Elements.
#
# Run remotely:
#   bash sgr-symbols.sh
#
# Expected:
#   - Every character cell shows its glyph (no tofu ▢).
#   - Block elements (▘▝▜▛▟▙█▀▄▌▐) tile cleanly without gaps or overlaps.
#   - Geometric shapes (◼◻◯●○) sit in their cells with visible margins.
#   - Misc Technical (⏵⏺✻⎿) render from fallback fonts when Menlo lacks
#     coverage — macOS picks up STIXTwoMath / Hiragino; iOS may show tofu
#     for ⏵ ⏺ ⎿ because the sandbox hides system symbol fonts from fontdb.
#   - SGR colour + underline / inverse on symbols work the same as on text.
#   - Combined patterns (Warp-style block separators) render as contiguous
#     shapes without visible seams.

esc=$'\033'

# ── Individual glyph check ────────────────────────────────────────────
printf '\n%s── Block Elements ──%s\n' "${esc}[1m" "${esc}[0m"
printf 'U+2588 full block       %s█%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+2580 upper half       %s▀%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+2584 lower half       %s▄%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+258C left half        %s▌%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+2590 right half       %s▐%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+2598 quadrant UL      %s▘%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+259D quadrant UR      %s▝%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+2597 quadrant LR      %s▗%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+2596 quadrant LL      %s▖%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+259C quad UL+UR+LR    %s▜%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+259B quad UL+UR+LL    %s▛%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+259F quad all four    %s▟%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+259A quad UL+LR       %s▚%s\n'                     "${esc}[0m" "${esc}[0m"
printf 'U+2599 quad UR+UL+LL+LR %s▙%s\n'                     "${esc}[0m" "${esc}[0m"

printf '\n%s── Geometric Shapes ──%s\n' "${esc}[1m" "${esc}[0m"
printf 'U+25FC black med square %s◼%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+25FB white med square %s◻%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+25A0 black square     %s■%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+25A1 white square     %s□%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+25EF large circle     %s◯%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+25CF black circle     %s●%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+25CB white circle     %s○%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+25B6 black rt triang  %s▶%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+25C0 black lt triang  %s◀%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+25B2 black up triang  %s▲%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+25BC black dn triang  %s▼%s\n'                    "${esc}[0m" "${esc}[0m"

printf '\n%s── Miscellaneous Technical ──%s\n' "${esc}[1m" "${esc}[0m"
printf 'U+23F5 black med rt tri %s⏵%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+23F4 black med lt tri %s⏴%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+23F0 alarm clock       %s⏰%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+23FA black circle rec %s⏺%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+23FB white circle rec %s⏻%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+23BF dentistry vert    %s⎿%s\n'                    "${esc}[0m" "${esc}[0m"

printf '\n%s── Dingbats ──%s\n' "${esc}[1m" "${esc}[0m"
printf 'U+273B teardrop asterisk %s✻%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+2713 check mark        %s✓%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+2717 ballot x          %s✗%s\n'                    "${esc}[0m" "${esc}[0m"
printf 'U+2726 black star        %s✦%s\n'                    "${esc}[0m" "${esc}[0m"

# ── Block-element adjacency (no gap / overlap between cells) ──────────
printf '\n%s── Adjacency check ──%s\n' "${esc}[1m" "${esc}[0m"
printf '%sSolid bar:%s         ' "${esc}[3m" "${esc}[0m"
printf '████████████████████\n'
printf '%sChecker half:%s      ' "${esc}[3m" "${esc}[0m"
printf '▀▄▀▄▀▄▀▄▀▄▀▄▀▄▀▄▀▄\n'
printf '%sChecker quad:%s      ' "${esc}[3m" "${esc}[0m"
printf '▘▝▘▝▘▝▘▝▘▝▘▝▘▝▘▝▘▝\n'
printf '%sStair UL→LR:%s       ' "${esc}[3m" "${esc}[0m"
printf '▘▚▙█\n'
printf '%sStair UR→LL:%s       ' "${esc}[3m" "${esc}[0m"
printf '▝▚▛█\n'
printf '%sLeft half bar:%s     ' "${esc}[3m" "${esc}[0m"
printf '▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌▌\n'
printf '%sRight half bar:%s    ' "${esc}[3m" "${esc}[0m"
printf '▐▐▐▐▐▐▐▐▐▐▐▐▐▐▐▐▐▐▐▐\n'

# ── Warp-style block separators ───────────────────────────────────────
printf '\n%s── Warp-style block separators ──%s\n' "${esc}[1m" "${esc}[0m"
printf '%sSolid bar (full):%s        %s██████████████████████████████████████%s\n' \
    "${esc}[3m" "${esc}[0m" "${esc}[1m" "${esc}[0m"
printf '%sDim bar (full):%s          %s██████████████████████████████████████%s\n' \
    "${esc}[3m" "${esc}[0m" "${esc}[2m" "${esc}[0m"
printf '%sThick-thin upper:%s       %s▛▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▜%s\n' \
    "${esc}[3m" "${esc}[0m" "${esc}[1m" "${esc}[0m"
printf '%sThick-thin lower:%s       %s▙▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▟%s\n' \
    "${esc}[3m" "${esc}[0m" "${esc}[1m" "${esc}[0m"
printf '%sAngle start (bold):%s      %s▛▀▜%s  %s▛▀▀▀▜%s\n' \
    "${esc}[3m" "${esc}[0m" "${esc}[1m" "${esc}[0m" "${esc}[1m" "${esc}[0m"
printf '%sAngle end (bold):%s        %s▙▄▟%s  %s▙▄▄▄▟%s\n' \
    "${esc}[3m" "${esc}[0m" "${esc}[1m" "${esc}[0m" "${esc}[1m" "${esc}[0m"
# The exact pattern the user observed:
printf '%sObserved pattern:%s       ' "${esc}[3m" "${esc}[0m"
printf '⏵⏵ ⏺ ✻ ◼ ◻ ⏺ ◯ ▘▘ ▝▝ ▝▜█████▛▘ ⎿\n'

# ── Color × symbol matrix ─────────────────────────────────────────────
printf '\n%s── Colour × symbol ──%s\n' "${esc}[1m" "${esc}[0m"
for ch in █ ◼ ⏵ ✻; do
    printf '%s %sregular%s  ' "$ch" "${esc}[0m" "${esc}[0m"
    for code in 31 32 33 34 35 36; do
        printf '%s %s %s' "${esc}[${code}m" "$ch" "${esc}[0m"
    done
    printf '\n'
done

# ── SGR attributes over symbols ───────────────────────────────────────
printf '\n%s── SGR attributes over symbols ──%s\n' "${esc}[1m" "${esc}[0m"
printf '%sbold%s       %s◼◻◯ █▀▄ ⏵⏺ ✻%s\n' \
    "${esc}[1m" "${esc}[0m" "${esc}[1m" "${esc}[0m"
printf '%sitalic%s     %s◼◻◯ █▀▄ ⏵⏺ ✻%s\n' \
    "${esc}[3m" "${esc}[0m" "${esc}[3m" "${esc}[0m"
printf '%sunderline%s  %s◼◻◯ █▀▄ ⏵⏺ ✻%s\n' \
    "${esc}[4m" "${esc}[0m" "${esc}[4m" "${esc}[0m"
printf '%sinverse%s    %s◼◻◯ █▀▄ ⏵⏺ ✻%s\n' \
    "${esc}[7m" "${esc}[0m" "${esc}[7m" "${esc}[0m"
printf '%sstrike%s     %s◼◻◯ █▀▄ ⏵⏺ ✻%s\n' \
    "${esc}[9m" "${esc}[0m" "${esc}[9m" "${esc}[0m"
printf '%sbold+fg%s   %s◼◻◯ █▀▄ ⏵⏺ ✻%s\n' \
    "${esc}[1;31m" "${esc}[0m" "${esc}[1;31m" "${esc}[0m"
printf '%sbold+und%s  %s◼◻◯ █▀▄ ⏵⏺ ✻%s\n' \
    "${esc}[1;4m" "${esc}[0m" "${esc}[1;4m" "${esc}[0m"

# ── Dim / faint symbols ───────────────────────────────────────────────
printf '\n%s── Dim (SGR 2) symbols ──%s\n' "${esc}[1m" "${esc}[0m"
printf '%sdim full bar:%s   ' "${esc}[3m" "${esc}[0m"
printf '%s████████████████████%s\n' "${esc}[2m" "${esc}[0m"
printf '%sdim shapes:%s     ' "${esc}[3m" "${esc}[0m"
printf '%s◼◻◯ ⏵⏺ ✻%s\n' "${esc}[2m" "${esc}[0m"

# ── Inverse + colour: bg fill check ───────────────────────────────────
printf '\n%s── Inverse + colour (bg fills cell) ──%s\n' "${esc}[1m" "${esc}[0m"
for code in 31 32 33 34 35 36; do
    printf '%sfg %d%s %s██████████%s %s▌▌▌▌▌%s %s◼◼◼%s\n' \
        "${esc}[7;${code}m" "$code" "${esc}[0m" \
        "${esc}[7;${code}m" "${esc}[0m" \
        "${esc}[7;${code}m" "${esc}[0m" \
        "${esc}[7;${code}m" "${esc}[0m"
done
printf '\n'
