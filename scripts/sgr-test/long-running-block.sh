#!/usr/bin/env bash
# Long-running block test. Exercises the "live" block grid path
# (Preexec → streaming output → optional CommandFinished).
#
# Block 1: sealed block (has CommandFinished) — exercises frozen_snapshot
# Block 2: running block (no CommandFinished) — exercises block.grid
# Block 3: growing block — output after a "delay", then CommandFinished
#
#   bash long-running-block.sh | cargo run -p bedterm_core --bin bedterm-render blocks > /tmp/long-running.png

esc=$'\033'

dcs() {
    local json="$1"
    local hex
    hex=$(printf '%s' "$json" | xxd -p | tr -d '\n')
    printf '\033P$d%s\234' "$hex"
}

# ── Prompt ─────────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %son %smain%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 1: cargo check (sealed, completes) ────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"cargo check"}}'
printf 'cargo check\n'
printf '    %sChecking%s myproject v0.1.0\n' "${esc}[1;32m" "${esc}[0m"
printf '    %sChecking%s auth v0.1.0\n' "${esc}[1;32m" "${esc}[0m"
printf '%serror[E0308]%s: mismatched types\n' "${esc}[1;31m" "${esc}[0m"
printf '  %s-->%s src/auth.rs:11:9\n' "${esc}[1;34m" "${esc}[0m"
printf '   |\n'
printf '11 |         panic!(\"2FA token not found\");\n'
printf '   |         %s^^^^^^ expected Result<...>, found !%s\n' "${esc}[1;31m" "${esc}[0m"
printf '\n'
printf '%serror[E0599]%s: no method named `as_str`\n' "${esc}[1;31m" "${esc}[0m"
printf '  %s-->%s src/auth.rs:42:43\n' "${esc}[1;34m" "${esc}[0m"
printf '\n'
printf '%sSome errors have detailed explanations%s\n' "${esc}[2m" "${esc}[0m"
printf '%srun `cargo explain E0308` for more info%s\n' "${esc}[2m" "${esc}[0m"
dcs '{"hook":"CommandFinished","value":{"exit_code":101}}'

# ── Prompt ─────────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %son %smain%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 2: cargo build (RUNNING — no CommandFinished) ─────────────────
# This block stays in the "running" state. The renderer must use
# block.grid (live grid path) rather than frozen_snapshot.
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"cargo build --release"}}'
printf 'cargo build --release\n'
printf '   %sCompiling%s myproject v0.1.0\n' "${esc}[1;32m" "${esc}[0m"
printf '   %sCompiling%s auth v0.1.0\n' "${esc}[1;32m" "${esc}[0m"
printf '   %sCompiling%s db v0.1.0\n' "${esc}[1;32m" "${esc}[0m"
printf '   %sCompiling%s http v0.1.0\n' "${esc}[1;32m" "${esc}[0m"
printf '   %sCompiling%s cli v0.1.0\n' "${esc}[1;32m" "${esc}[0m"
printf '\n'
printf '   Building [=======>           ] 37/89\n'
# No CommandFinished — this block remains "running"

# ── Prompt ─────────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %son %smain%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 3: npm install (grows, then finishes) ─────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project/frontend"}}'
dcs '{"hook":"Preexec","value":{"command":"npm install"}}'
printf 'npm install\n'
printf '\n'
printf 'added 147 packages, removed 23 packages, changed 8 packages,\n'
printf 'and audited 512 packages in 12s\n'
printf '\n'
printf '98 packages are looking for funding\n'
printf '  run `npm fund` for details\n'
printf '\n'
printf '%sfound 0 vulnerabilities%s\n' "${esc}[32m" "${esc}[0m"
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'

# ── Final prompt ────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %son %smain%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"
printf '\n'
