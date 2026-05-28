#!/usr/bin/env bash
# Alternate screen buffer test — all lines ≤31 visible chars.
# Exercises ESC[?1049h/l (smcup/rmcup) used by vim/less/top.
#
#   bash sgr-altscreen.sh | cargo run -p bedterm-core --bin bedterm-render grid > /tmp/alt.png

esc=$'\033'

# ── Before alt-screen ──────────────────────────────────────────────
printf '%s╭─%s ~/project\n' "${esc}[1;36m" "${esc}[0m"
printf '%s╰─%s$ %svim src/auth.rs%s\n' \
    "${esc}[36m" "${esc}[0m" "${esc}[1;37m" "${esc}[0m"

# ── Enter alt-screen ───────────────────────────────────────────────
printf '\033[?1049h\033[2J\033[H'

# Header bar
printf '\033[7m src/auth.rs                   \033[0m\n'

# File content
printf '\033[34m 1\033[0m fn \033[33mverify_2fa\033[0m(''code: \033[36m&str\033[0m)\n'
printf '\033[34m 2\033[0m     \033[35mlet\033[0m c = \033[33mget_db\033[0m()?;\n'
printf '\033[34m 3\033[0m     \033[35mlet\033[0m row = c\n'
printf '\033[34m 4\033[0m       .\033[33mquery\033[0m(\033[32m\"SELECT\"\033[0m)\n'
printf '\033[34m 5\033[0m       .\033[33mbind\033[0m(&[code])?\n'
printf '\033[34m 6\033[0m       .\033[33mfetch\033[0m()?\n'
printf '\033[34m 7\033[0m \n'
printf '\033[34m 8\033[0m     \033[35mif\033[0m \033[35mlet\033[0m \033[36mSome\033[0m(t)=row\n'
printf '\033[34m 9\033[0m       \033[35mreturn\033[0m \033[36mOk\033[0m(t)\n'
printf '\033[34m10\033[0m     \033[1;31mpanic!(\033[0m\"not found\"\033[1;31m)\033[0m\n'

# Empty lines
printf '\033[36m~\033[0m\n'
printf '\033[36m~\033[0m\n'
printf '\033[36m~\033[0m\n'

# Status bar
printf '\033[7m src/auth.rs        Rust  11/42  \033[0m'

# Command line
printf '\n\033[1m:s/panic/Err gc\033[0m'

# ── Exit alt-screen ────────────────────────────────────────────────
printf '\033[?1049l\n'

# ── After alt-screen ───────────────────────────────────────────────
printf '%s\"src/auth.rs\" 11L%s\n' "${esc}[2m" "${esc}[0m"
printf '  Compiling myproject\n'
printf '  %sFinished test%s\n' "${esc}[1;32m" "${esc}[0m"
printf '\n%s╭─%s ~/project %smain%s %s✓%s\n' \
    "${esc}[1;36m" "${esc}[0m" "${esc}[1;33m" "${esc}[0m" \
    "${esc}[32m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"
printf '\n'
