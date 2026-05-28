#!/usr/bin/env bash
# Emoji + styles + mixed-width rendering test.
#
#   bash sgr-emoji.sh | cargo run -p bedterm-core --bin bedterm-render grid > /tmp/sgr-emoji.png

esc=$'\033'

# ── Plain emoji ────────────────────────────────────────────────────────
printf '=== emoji plain ===\n'
printf '🔴🟢🔵🟡🟣🟠🟤⚫⚪\n'
printf '✅❌⚠️ℹ️🔥💯🎉🚀💡\n'
printf '😀😃😄😁😆😅🤣😂🙂🙃\n'
printf '⌨️🖥️📱🖱️💾📀🔌🔋\n'

# ── Emoji with SGR styles ──────────────────────────────────────────────
printf '\n=== emoji + styles ===\n'
printf '%sbold  %s🚀 rocket bold%s\n' "${esc}[1m" "" "${esc}[0m"
printf '%sitalic%s  🔥 fire italic%s\n' "${esc}[3m" "" "${esc}[0m"
printf '%sunderline%s  ✅ check underline%s\n' "${esc}[4m" "" "${esc}[0m"
printf '%sinverse%s  ⚠️ warning inverse%s\n' "${esc}[7m" "" "${esc}[0m"
printf '%sdim%s  💡 idea dim%s\n' "${esc}[2m" "" "${esc}[0m"

# ── Mixed ascii + emoji on same line ────────────────────────────────────
printf '\n=== mixed ascii + emoji ===\n'
printf 'build %s✅%s  test %s❌%s  lint %s⚠️%s  deploy %s🚀%s\n' \
    "${esc}[32m" "${esc}[0m" \
    "${esc}[31m" "${esc}[0m" \
    "${esc}[33m" "${esc}[0m" \
    "${esc}[34m" "${esc}[0m"
printf '%sfatal error:%s %s🔥 process exited with code 1%s\n' \
    "${esc}[1;31m" "${esc}[0m" \
    "${esc}[7m" "${esc}[0m"

# ── Wide CJK + narrow ascii side by side ───────────────────────────────
printf '\n=== CJK + ascii mix ===\n'
printf '日本語のテスト %s太字%s %s斜体%s %s下線%s %s反転%s\n' \
    "${esc}[1m" "${esc}[0m" "${esc}[3m" "${esc}[0m" \
    "${esc}[4m" "${esc}[0m" "${esc}[7m" "${esc}[0m"
printf '你好世界 Hello世界 こんにちはWorld\n'
printf '%sbold日本語%s normalEnglish %sitalic中文%s\n' \
    "${esc}[1m" "${esc}[0m" "${esc}[3m" "${esc}[0m"

# ── Emoji in colored backgrounds ───────────────────────────────────────
printf '\n=== emoji on colored backgrounds ===\n'
for c in 1 2 3 4 5 6; do
    printf '%s 🔴 %s' "${esc}[48;5;${c}m" "${esc}[0m"
done
printf '\n'
for c in 196 202 208 214 220 226; do
    printf '%s 🟢 %s' "${esc}[48;5;${c}m" "${esc}[0m"
done
printf '\n'
# True color bg behind emoji
printf '%s 🔥 on gradient bg %s\n' \
    "${esc}[48;2;40;40;60m" "${esc}[0m"

# ── Zero-width joiners and modifiers ───────────────────────────────────
printf '\n=== skin tones + ZWJ ===\n'
printf '👋 👋🏻 👋🏼 👋🏽 👋🏾 👋🏿  (wave + fitzpatrick)\n'
printf '👨‍💻 👩‍💻 🧑‍💻  (technologist ZWJ)\n'
printf '👨‍👩‍👧‍👦 👨‍👨‍👧  (family ZWJ)\n'

printf '\n'
