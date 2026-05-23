#!/usr/bin/env bash
# Simulate a realistic Claude Code session with multiple command blocks,
# then switch to Codex, then /exit. Uses DCS shell-integration events so
# bedterm-render blocks mode can render it as a block list.
#
# Generate block-list PNG:
#   bash claude-code-session.sh | cargo run -p bedterm_core --bin bedterm-render blocks --palette tokyo-night > /tmp/claude-session.png
#
# Generate terminal-grid PNG (raw bytes without blocks):
#   bash claude-code-session.sh | cargo run -p bedterm_core --bin bedterm-render grid --palette tokyo-night > /tmp/claude-session-grid.png

esc=$'\033'

# ── DCS helper ─────────────────────────────────────────────────────────
dcs() {
    # Format: ESC P $ d <hex JSON> 0x9C
    local json="$1"
    local hex
    hex=$(printf '%s' "$json" | xxd -p | tr -d '\n')
    printf '\033P$d%s\234' "$hex"
}

# ── Shell startup ──────────────────────────────────────────────────────
printf '%s╭─%s bedterm %s—%s ssh %salice@devbox%s %s—%s ~/project%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 1: ls ────────────────────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"ls --color"}}'
printf '%s\n' 'ls --color'
printf 'src/  tests/  %sCargo.toml%s  %sREADME.md%s  %sCargo.lock%s\n' \
    "${esc}[1;31m" "${esc}[0m" \
    "${esc}[1;37m" "${esc}[0m" \
    "${esc}[1;37m" "${esc}[0m"
printf '%sdrwxr-xr-x%s  3 alice  staff   96 May 24 10:00 %ssrc%s\n' \
    "${esc}[01;34m" "${esc}[0m" \
    "${esc}[01;34m" "${esc}[0m"
printf '%sdrwxr-xr-x%s  2 alice  staff   64 May 24 09:55 %stests%s\n' \
    "${esc}[01;34m" "${esc}[0m" \
    "${esc}[01;34m" "${esc}[0m"
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'

# ── Prompt ─────────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %son %sfeature/cli%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 2: claude ────────────────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"claude"}}'
printf '%s\n' 'claude'
printf '\n'
printf '%s⏺(pwd: ~/project)%s\n' "${esc}[1;35m" "${esc}[0m"
printf '\n'
printf '%s⏺%s fix the authentication bug in src/auth.rs. The\n' "${esc}[1;35m" "${esc}[0m"
printf '  login endpoint returns 500 when users have 2FA enabled.\n'
printf '\n'
printf '%s⏺%s I'"'"'ll investigate the auth flow. Let me start by\n' "${esc}[1;35m" "${esc}[0m"
printf '%s  %s reading the relevant files.%s\n' "${esc}[2m" "${esc}[22m" "${esc}[0m"
printf '\n'
printf '%s⏺%s %sTool: Read%s\n' "${esc}[1;35m" "${esc}[0m" "${esc}[1;36m" "${esc}[0m"
printf '%s  %s src/auth.rs%s\n' "${esc}[2m" "${esc}[0m"
printf '\n'
printf '%s⏺%s %sTool: Grep%s\n' "${esc}[1;35m" "${esc}[0m" "${esc}[1;36m" "${esc}[0m"
printf '  Pattern: %s2fa_enabled%s | %senforce_2fa%s\n' \
    "${esc}[33m" "${esc}[0m" "${esc}[33m" "${esc}[0m"
printf '\n'
printf '%s⏺%s I can see the issue. The %sverify_2fa_token()%s\n' \
    "${esc}[1;35m" "${esc}[0m" \
    "${esc}[1;37m" "${esc}[0m"
printf '%s  %s function panics when the token is valid — it should%s\n' "${esc}[2m" "${esc}[22m" "${esc}[0m"
printf '%s  %s return Ok(()) instead.%s\n' "${esc}[2m" "${esc}[22m" "${esc}[0m"
printf '\n'
printf '%s⏺%s Let me apply the fix.%s\n' "${esc}[1;35m" "${esc}[0m" "${esc}[2m" "${esc}[0m"
printf '\n'
printf '%s⏺%s %sTool: Edit%s%s  src/auth.rs%s\n' \
    "${esc}[1;35m" "${esc}[0m" \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[2m" "${esc}[0m"
printf '\n'
printf '%s  %s- panic!("{}", ...);%s\n' "${esc}[31m" "${esc}[1;31m" "${esc}[0m"
printf '%s  %s+ Ok(());%s\n' "${esc}[32m" "${esc}[1;32m" "${esc}[0m"
printf '\n'
printf '%s⏺%s The fix is in place. %s✓%s verify_2fa_token now%s\n' \
    "${esc}[1;35m" "${esc}[0m" \
    "${esc}[32m" "${esc}[0m" \
    "${esc}[2m" "${esc}[22m" "${esc}[0m"
printf '%s  %s returns Ok(()). Build passed, 65 tests green.%s\n' "${esc}[2m" "${esc}[22m" "${esc}[0m"
printf '\n'
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'

# ── Prompt ─────────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %son %sfeature/cli%s %s+1 fix%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m" \
    "${esc}[32m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 3: /exit ─────────────────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"exit"}}'
printf '%s\n' '/exit'
printf '%s⏺%s %sBye!%s %s(exit code: 0)%s\n' \
    "${esc}[1;35m" "${esc}[0m" \
    "${esc}[2m" "${esc}[22m" \
    "${esc}[2m" "${esc}[22m" \
    "${esc}[2m" "${esc}[0m"
printf '\n'
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'

# ── Prompt ─────────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %son %sfeature/cli%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 4: codex ─────────────────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"codex exec 'fix the build'"}}'
printf '%s\n' 'codex exec "fix the build"'
printf '\n'
printf '%s┌%s Codex %s┐%s\n' "${esc}[1;32m" "${esc}[0m" "${esc}[32m" "${esc}[0m"
printf ' Reading workspace...\n'
printf ' %s✓%s src/auth.rs (modified)%s\n' "${esc}[32m" "${esc}[0m" "${esc}[2m" "${esc}[0m"
printf ' %s✓%s Cargo.toml%s\n' "${esc}[32m" "${esc}[0m" "${esc}[2m" "${esc}[0m"
printf ' %s✓%s tests/auth_tests.rs%s\n' "${esc}[32m" "${esc}[0m" "${esc}[2m" "${esc}[0m"
printf '\n'
printf ' Task: %sfix the build%s\n' "${esc}[1;37m" "${esc}[0m"
printf '\n'
printf ' %sAction: ShellCommand%s\n' "${esc}[1;33m" "${esc}[0m"
printf '   %scargo check 2>&1%s\n' "${esc}[2m" "${esc}[0m"
printf '   %serror[E0308]: mismatched types%s\n' "${esc}[1;31m" "${esc}[0m"
printf '   %s  expected String, found &str%s\n' "${esc}[31m" "${esc}[0m"
printf '   %s  --> src/auth.rs:42:5%s\n' "${esc}[2m" "${esc}[0m"
printf '\n'
printf ' %sAction: Edit%s\n' "${esc}[1;33m" "${esc}[0m"
printf '   %s- let token = verify_2fa_token(code);%s\n' "${esc}[31m" "${esc}[0m"
printf '   %s+ let token = verify_2fa_token(code).to_string();%s\n' \
    "${esc}[32m" "${esc}[0m"
printf '\n'
printf ' %sAction: ShellCommand%s\n' "${esc}[1;33m" "${esc}[0m"
printf '   %scargo build%s\n' "${esc}[2m" "${esc}[0m"
printf '   %s   Compiling myproject v0.1.0%s\n' "${esc}[1;32m" "${esc}[0m"
printf '   %s    Finished dev [unoptimized] in 1.3s%s\n' "${esc}[1;32m" "${esc}[0m"
printf '\n'
printf ' %s✓ Build fixed. Tests pass.%s\n' "${esc}[32m" "${esc}[0m"
printf '\n'
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'

# ── Prompt ─────────────────────────────────────────────────────────────
printf '%s╭─%s ~/project %son %sfeature/cli%s\n' \
    "${esc}[1;36m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[1;33m" "${esc}[0m"
printf '%s╰─%s$ %s' "${esc}[36m" "${esc}[0m" "${esc}[1m"

# ── Block 5: /exit again ───────────────────────────────────────────────
dcs '{"hook":"Precmd","value":{"pwd":"/home/alice/project"}}'
dcs '{"hook":"Preexec","value":{"command":"/exit"}}'
printf '%s\n' '/exit'
printf '%s⏺%s %sSession ended.%s\n' \
    "${esc}[1;35m" "${esc}[0m" \
    "${esc}[2m" "${esc}[0m"
dcs '{"hook":"CommandFinished","value":{"exit_code":0}}'
printf '\n'
