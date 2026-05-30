# BedTerm shell integration — Warp-compatible DCS+JSON block boundaries.
#
# Wire format on every event:
#
#   ESC P $ d <hex-of-JSON> ESC \
#
# `$` is the DCS intermediate, `d` the final byte ("hex-encoded JSON" in
# Warp's spec). The trailer is the 7-bit ST (`ESC \`); the Rust-side
# parser also accepts the 8-bit single-byte ST (`0x9C`).
#
# Three event kinds, dispatched by the JSON `hook` field (Warp-compatible
# tagged enum):
#
#   {"hook":"Precmd",          "value":{"pwd":"<path>"}}
#   {"hook":"Preexec",         "value":{"command":"<cmdline>"}}
#   {"hook":"CommandFinished", "value":{"exit_code":<int>}}
#
# Per-prompt flow:
#
#   precmd  → emit CommandFinished (if a command actually ran) then Precmd
#   preexec → emit Preexec, mark that a command ran
#
# Command duration is computed on the Rust side from the wall-clock delta
# between Preexec and CommandFinished — the shell does not ship a duration
# field (and neither does Warp's wire format).
#
# Sourced by BedTerm immediately after SSH session start. Idempotent — safe
# to source twice. Works in bash 4+ and zsh 5+. Other shells: silent no-op.
#
# Note: this is an independent re-implementation of the Warp wire format.
# No code is copied from Warp; only the on-the-wire bytes are reproduced
# so a Warp-style remote shell could in principle drive BedTerm too.
#
# This file is the literal payload pushed to the remote shell, so keep it
# small and side-effect-free. Anything that fails should fail silently — a
# broken integration must not break the user's session.

if [ -z "${__BEDTERM_INTEGRATION_INSTALLED:-}" ]; then

# JSON-escape stdin → stdout (no surrounding quotes). Handles `\`, `"`, and
# every control byte 0x00-0x1F (BS/TAB/LF/FF/CR get short forms; rest go to
# \uXXXX). Bytes >= 0x80 pass through verbatim, so UTF-8 input remains
# UTF-8 in the output (the surrogate pair case never arises because we
# never emit a bare codepoint above U+007F).
__bedterm_json_escape() {
    LC_ALL=C awk '
    BEGIN {
        for (i = 0; i < 256; i++) lut[sprintf("%c", i)] = i
        for (i = 0; i < 32; i++)  esc[i] = sprintf("\\u%04x", i)
        esc[8]  = "\\b"
        esc[9]  = "\\t"
        esc[10] = "\\n"
        esc[12] = "\\f"
        esc[13] = "\\r"
        esc[34] = "\\\""
        esc[92] = "\\\\"
    }
    {
        if (NR > 1) printf "\\n"
        n = length($0)
        for (i = 1; i <= n; i++) {
            c = substr($0, i, 1)
            o = lut[c]
            if (o in esc) printf "%s", esc[o]
            else          printf "%s", c
        }
    }
    '
}

# Hex-encode stdin → stdout, lowercase, no whitespace. `od -An -v -tx1` is
# in every POSIX environment; `LC_ALL=C` keeps the byte interpretation
# stable under exotic locales.
__bedterm_hex() {
    LC_ALL=C od -An -v -tx1 | tr -d ' \n'
}

# JSON body on stdin → wrapped DCS on stdout. `ESC P $ d <hex> ESC \`.
__bedterm_emit_dcs() {
    printf '\033P$d'
    __bedterm_hex
    printf '\033\\'
}

__bedterm_emit_precmd() {
    # $1 = pwd, $2 = git branch (may be empty)
    local pwd_esc branch_esc
    pwd_esc=$(printf '%s' "$1" | __bedterm_json_escape)
    branch_esc=$(printf '%s' "$2" | __bedterm_json_escape)
    printf '{"hook":"Precmd","value":{"pwd":"%s","git_branch":"%s"}}' \
        "$pwd_esc" "$branch_esc" | __bedterm_emit_dcs
}

# Cheap branch lookup. Empty when cwd is outside a repo or git is
# missing. Suppress all errors so a borked repo never breaks the prompt.
__bedterm_git_branch() {
    command -v git >/dev/null 2>&1 || return 0
    git symbolic-ref --quiet --short HEAD 2>/dev/null \
        || git rev-parse --short HEAD 2>/dev/null \
        || true
}

__bedterm_emit_preexec() {
    # $1 = command line
    local esc
    esc=$(printf '%s' "$1" | __bedterm_json_escape)
    printf '{"hook":"Preexec","value":{"command":"%s"}}' "$esc" | __bedterm_emit_dcs
}

__bedterm_emit_finished() {
    # $1 = exit code (integer)
    printf '{"hook":"CommandFinished","value":{"exit_code":%d}}' "$1" | __bedterm_emit_dcs
}

if [ -n "${ZSH_VERSION:-}" ]; then
    autoload -Uz add-zsh-hook

    __bedterm_precmd() {
        local exit_code=$?
        if [ -n "${__BEDTERM_RAN:-}" ]; then
            __bedterm_emit_finished "$exit_code"
            unset __BEDTERM_RAN
        fi
        __bedterm_emit_precmd "$PWD" "$(__bedterm_git_branch)"
    }

    __bedterm_preexec() {
        # $1 is the expanded command line.
        __bedterm_emit_preexec "$1"
        __BEDTERM_RAN=1
    }

    add-zsh-hook precmd __bedterm_precmd
    add-zsh-hook preexec __bedterm_preexec

elif [ -n "${BASH_VERSION:-}" ]; then
    # bash has no native preexec — fake it via DEBUG trap, guarded so the
    # trap only fires once per command (DEBUG runs before *every* simple
    # command, including ones inside scripts and functions).
    __BEDTERM_IN_CMD=0

    __bedterm_preexec() {
        [ "$__BEDTERM_IN_CMD" -eq 1 ] && return
        case "$BASH_COMMAND" in
            __bedterm_*|__BEDTERM_*) return ;;
        esac
        __bedterm_emit_preexec "$BASH_COMMAND"
        __BEDTERM_RAN=1
        __BEDTERM_IN_CMD=1
    }

    __bedterm_precmd() {
        local exit_code=$?
        if [ -n "${__BEDTERM_RAN:-}" ]; then
            __bedterm_emit_finished "$exit_code"
            unset __BEDTERM_RAN
        fi
        __bedterm_emit_precmd "$PWD" "$(__bedterm_git_branch)"
        __BEDTERM_IN_CMD=0
    }

    trap '__bedterm_preexec' DEBUG
    PROMPT_COMMAND='__bedterm_precmd'"${PROMPT_COMMAND:+;$PROMPT_COMMAND}"

else
    # Unknown shell — quietly skip. The terminal still works; just no blocks.
    :
fi

__BEDTERM_INTEGRATION_INSTALLED=1
fi  # end "if [ -z $__BEDTERM_INTEGRATION_INSTALLED ]"
