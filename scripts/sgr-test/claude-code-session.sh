#!/usr/bin/env bash
# Simulate a Claude Code session with lines ≤34 chars (iPhone 17 width).
# Uses DCS shell-integration events so bedterm-render blocks mode can
# render it as a block list.
#
#   bash claude-code-session.sh | cargo run -p bedterm-core --bin bedterm-render blocks > /tmp/claude.png

esc=$'\033'

dcs() {
    local json="$1"
    local hex
    hex=$(printf '%s' "$json" | xxd -p | tr -d '\n')
    printf '\033P$d%s\234' "$hex"
}

# ── Shell startup ──────────────────────────────────────────────────
printf '%s╭─%s ~/project%s\n' "${esc}[1;36m" "${esc}[0m" "${esc}[36m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 1: ls ────────────────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"ls --color"}}'
printf 'ls --color\n'
printf '%ssrc/%s  %stests/%s\n' "${esc}[1;34m" "${esc}[0m" "${esc}[1;34m" "${esc}[0m"
printf '%sCargo.toml%s\n' "${esc}[1;31m" "${esc}[0m"
printf '%sREADME.md%s\n' "${esc}[1;37m" "${esc}[0m"
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'

# ── Prompt ─────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %sfeature/cli%s\n' \
    "${esc}[1;36m" "${esc}[0m" "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 2: claude ────────────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"claude"}}'
printf 'claude\n\n'
printf '%s⏺(pwd: ~/project)%s\n\n' "${esc}[1;35m" "${esc}[0m"
printf '%s⏺%s fix the auth bug\n' "${esc}[1;35m" "${esc}[0m"
printf '  in src/auth.rs\n\n'
printf '%s⏺%s investigating...\n\n' "${esc}[1;35m" "${esc}[0m"
printf '%s⏺%s %sTool: Read%s\n' "${esc}[1;35m" "${esc}[0m" "${esc}[1;36m" "${esc}[0m"
printf '  src/auth.rs\n\n'
printf '%s⏺%s %sTool: Grep%s\n' "${esc}[1;35m" "${esc}[0m" "${esc}[1;36m" "${esc}[0m"
printf '  %s2fa_enabled%s\n\n' "${esc}[33m" "${esc}[0m"
printf '%s⏺%s found it. %sverify_2fa%s\n' \
    "${esc}[1;35m" "${esc}[0m" "${esc}[1;37m" "${esc}[0m"
printf '  panics on valid token\n\n'
printf '%s⏺%s %sTool: Edit%s\n' "${esc}[1;35m" "${esc}[0m" "${esc}[1;36m" "${esc}[0m"
printf '  src/auth.rs\n'
printf '  %s- panic!(\"...\");%s\n' "${esc}[31m" "${esc}[0m"
printf '  %s+ Ok(());%s\n\n' "${esc}[32m" "${esc}[0m"
printf '%s⏺%s %s✓%s verify_2fa_token\n' \
    "${esc}[1;35m" "${esc}[0m" "${esc}[32m" "${esc}[0m"
printf '  returns Ok(()).\n'
printf '  build passed.\n\n'
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'

# ── Prompt ─────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %sfeature/cli%s %s+1%s\n' \
    "${esc}[1;36m" "${esc}[0m" "${esc}[1;33m" "${esc}[0m" \
    "${esc}[32m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 3: /exit ─────────────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"exit"}}'
printf '/exit\n'
printf '%s⏺%s %sBye!%s (exit: 0)\n\n' \
    "${esc}[1;35m" "${esc}[0m" "${esc}[2m" "${esc}[0m"
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'

# ── Prompt ─────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %sfeature/cli%s\n' \
    "${esc}[1;36m" "${esc}[0m" "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 4: codex ─────────────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"codex exec"}}'
printf 'codex exec\n\n'
printf '%s┌%s Codex %s┐%s\n' "${esc}[1;32m" "${esc}[0m" "${esc}[32m" "${esc}[0m"
printf ' Reading workspace\n'
printf ' %s✓%s src/auth.rs (mod)\n' "${esc}[32m" "${esc}[0m"
printf ' %s✓%s Cargo.toml\n' "${esc}[32m" "${esc}[0m"
printf '\n Task: %sfix the build%s\n' "${esc}[1;37m" "${esc}[0m"
printf '\n %sAction: Shell%s\n' "${esc}[1;33m" "${esc}[0m"
printf '  %scargo check%s\n' "${esc}[2m" "${esc}[0m"
printf '  %serror[E0308]%s\n' "${esc}[1;31m" "${esc}[0m"
printf '  mismatched types\n'
printf '\n %sAction: Edit%s\n' "${esc}[1;33m" "${esc}[0m"
printf '  %s- verify(code);%s\n' "${esc}[31m" "${esc}[0m"
printf '  %s+ verify(&code);%s\n' "${esc}[32m" "${esc}[0m"
printf '\n %sAction: Shell%s\n' "${esc}[1;33m" "${esc}[0m"
printf '  %scargo build%s\n' "${esc}[2m" "${esc}[0m"
printf '  %s    Finished%s\n' "${esc}[1;32m" "${esc}[0m"
printf '\n %s✓ Build fixed.%s\n\n' "${esc}[32m" "${esc}[0m"
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'

# ── Prompt ─────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %sfeature/cli%s\n' \
    "${esc}[1;36m" "${esc}[0m" "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 5: /exit ─────────────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"/exit"}}'
printf '/exit\n'
printf '%s⏺%s %sSession ended.%s\n\n' \
    "${esc}[1;35m" "${esc}[0m" "${esc}[2m" "${esc}[0m"
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'
printf '\n'
