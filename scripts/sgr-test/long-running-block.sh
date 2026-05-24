#!/usr/bin/env bash
# Long-running block test — all lines ≤31 visible chars.
# Block 1: sealed (cargo check, exit 101)
# Block 2: running (cargo build, no CommandFinished)
# Block 3: sealed (cargo test, exit 0)
#
#   bash long-running-block.sh | cargo run -p bedterm_core --bin bedterm-render blocks > /tmp/long.png

esc=$'\033'

dcs() {
    local json="$1"
    local hex
    hex=$(printf '%s' "$json" | xxd -p | tr -d '\n')
    printf '\033P$d%s\234' "$hex"
}

# ── Prompt ─────────────────────────────────────────────────────────
printf '%s╭─%s ~/project\n' "${esc}[1;36m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 1: cargo check ──────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"cargo check"}}'
printf 'cargo check\n'
printf ' %sChecking%s myproject\n' "${esc}[1;32m" "${esc}[0m"
printf ' %sChecking%s auth crate\n' "${esc}[1;32m" "${esc}[0m"
printf '%serror[E0308]%s\n' "${esc}[1;31m" "${esc}[0m"
printf ' mismatched types\n'
printf ' %s-->%s src/auth.rs:11\n' "${esc}[1;34m" "${esc}[0m"
printf ' panic!(\"2FA not found\")\n'
printf ' %s^^^^^^ expected Result%s\n' "${esc}[1;31m" "${esc}[0m"
printf '\n%srun cargo explain%s\n' "${esc}[2m" "${esc}[0m"
dcs '{"hook":"CommandFinished","value":{"exit_code":101}}'

# ── Prompt ─────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %smain%s\n' \
    "${esc}[1;36m" "${esc}[0m" "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 2: cargo build (RUNNING) ─────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"cargo build --release"}}'
printf 'cargo build --release\n'
printf ' %sCompiling%s myproject\n' "${esc}[1;32m" "${esc}[0m"
printf ' %sCompiling%s auth\n' "${esc}[1;32m" "${esc}[0m"
printf ' %sCompiling%s db\n' "${esc}[1;32m" "${esc}[0m"
printf ' %sCompiling%s http\n' "${esc}[1;32m" "${esc}[0m"
printf ' %sCompiling%s cli\n' "${esc}[1;32m" "${esc}[0m"
printf '\n Building [====>  ] 37/89\n'
# No CommandFinished — stays running (live grid path)

# ── Prompt ─────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %smain%s\n' \
    "${esc}[1;36m" "${esc}[0m" "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 3: cargo test ───────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"cargo test"}}'
printf 'cargo test\n'
printf ' %sCompiling%s myproject\n' "${esc}[1;32m" "${esc}[0m"
printf '\nrunning 65 tests\n'
printf 'test auth::verify_2fa ... %sok%s\n' "${esc}[32m" "${esc}[0m"
printf 'test auth::login ... %sok%s\n' "${esc}[32m" "${esc}[0m"
printf 'test db::connect ... %sok%s\n' "${esc}[32m" "${esc}[0m"
printf '...\n'
printf '\ntest result: %sok%s. 65 passed\n' "${esc}[32m" "${esc}[0m"
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'

# ── Final prompt ───────────────────────────────────────────────────
printf '%s╭─%s ~/project %smain%s\n' \
    "${esc}[1;36m" "${esc}[0m" "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"
printf '\n'
