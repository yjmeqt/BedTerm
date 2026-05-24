#!/usr/bin/env bash
# CJK + style combinations. Bold/italic over CJK exercises the
# fallback cascade: Menlo-Bold doesn't have 你好 glyphs, so cosmic-text
# should still fall through to the bundled Noto Sans Mono CJK for the
# codepoints but apply the bold/italic attribute on the matched face
# (which will look unstyled if Noto doesn't ship a bold cut — that's
# the "correct broken" outcome, not a regression).

esc=$'\033'

printf '%sregular%s   你好世界 こんにちは 안녕하세요\n' "${esc}[0m" "${esc}[0m"
printf '%sbold%s      你好世界 こんにちは 안녕하세요\n' "${esc}[1m" "${esc}[0m"
printf '%sitalic%s    你好世界 こんにちは 안녕하세요\n' "${esc}[3m" "${esc}[0m"
printf '%sunderline%s 你好世界 こんにちは 안녕하세요\n' "${esc}[4m" "${esc}[0m"
printf '%sinverse%s   你好世界 こんにちは 안녕하세요\n' "${esc}[7m" "${esc}[0m"

printf '\nmixed ascii + cjk in one row:\n'
printf 'cd %s/Users/%syi.jiang%s/项目/bedterm%s && ls\n' \
    "${esc}[1m" "${esc}[3m" "${esc}[23m" "${esc}[0m"
