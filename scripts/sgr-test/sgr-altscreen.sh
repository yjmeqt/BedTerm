#!/usr/bin/env bash
# Alternate screen buffer test. Exercises ESC[?1049h / ESC[?1049l
# (smcup / rmcup) used by vim, less, top, and other full-screen TUIs.
#
#   bash sgr-altscreen.sh | cargo run -p bedterm_core --bin bedterm-render grid > /tmp/sgr-altscreen.png

esc=$'\033'

# ── Before alt-screen: normal terminal content ──────────────────────
printf '%s╭─%s ~/project %son %smain%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %svim src/auth.rs%s\n' "${esc}[36m" "${esc}[0m" "${esc}[1;37m" "${esc}[0m"

# ── Enter alt-screen ─────────────────────────────────────────────────
printf '\033[?1049h'
printf '\033[2J'       # clear screen
printf '\033[H'        # cursor home

# Header bar (vim-style)
printf '\033[7m'       # inverse
printf '  src/auth.rs                           1,1            All  '
printf '\033[0m\n'

# File content with line numbers
printf '\033[34m  1 \033[0mfn \033[33mverify_2fa_token\033[0m(code: \033[36m&str\033[0m) -> \033[36mResult\033[0m<\033[36mString\033[0m> {\n'
printf '\033[34m  2 \033[0m    \033[35mlet\033[0m mut conn = \033[33mget_db_conn\033[0m()?;\n'
printf '\033[34m  3 \033[0m    \033[35mlet\033[0m row = conn\n'
printf '\033[34m  4 \033[0m        .\033[33mquery\033[0m(\033[32m\"SELECT token FROM 2fa WHERE code = ?1\"\033[0m)\n'
printf '\033[34m  5 \033[0m        .\033[33mbind\033[0m(&[code])?\n'
printf '\033[34m  6 \033[0m        .\033[33mfetch_one\033[0m()?;\n'
printf '\033[34m  7 \033[0m\n'
printf '\033[34m  8 \033[0m    \033[35mif\033[0m \033[35mlet\033[0m \033[36mSome\033[0m(token) = row {\n'
printf '\033[34m  9 \033[0m        \033[35mreturn\033[0m \033[36mOk\033[0m(token);\n'
printf '\033[34m 10 \033[0m    }\n'
printf '\033[34m 11 \033[0m    \033[1;31mpanic!(\033[0m\033[1;31m\"2FA token not found\"\033[0m\033[1;31m)\033[0m\033[1;31m;\033[0m  \033[31m// ← BUG: should return Err\n'
printf '\033[0m\n'
printf '\033[36m  ~                                                                               \n'
printf '\033[36m  ~                                                                               \n'
printf '\033[36m  ~                                                                               \n'

# Status bar
printf '\033[7m'
printf 'src/auth.rs                                       Rust      11/42    26%%     '
printf '\033[0m'

# Command line (vim command mode)
printf '\n'
printf '\033[1m:s/panic!/Err/|w|!cargo test\033[0m'

# ── Exit alt-screen (restore previous buffer) ─────────────────────────
printf '\033[?1049l'

# ── After alt-screen: original content restored ──────────────────────
printf '\n'
printf '%s\"src/auth.rs\" 11L, 342B written%s\n' "${esc}[2m" "${esc}[0m"
printf '   Compiling myproject v0.1.0\n'
printf '   %sFinished test [unoptimized] in 0.42s%s\n' "${esc}[1;32m" "${esc}[0m"
printf '\n'
printf '%s╭─%s ~/project %son %smain%s %s✓ tests pass%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m" \
    "${esc}[32m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"
printf '\n'
