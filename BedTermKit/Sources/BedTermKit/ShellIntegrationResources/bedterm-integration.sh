# BedTerm shell integration — OSC 133 (FinalTerm) prompt + command markers,
# plus extension attrs (`cmd`, `cwd`, `dur`) so Block-style views know what
# ran. Designed to coexist with iTerm2 / kitty / VSCode shell integrations:
# our OSC sequences use the same 133 namespace, so other terminals ignore
# attrs they don't recognise.
#
# Sourced by BedTerm immediately after SSH session start. Idempotent — safe
# to source twice. Works in bash 4+ and zsh 5+. Other shells: silent no-op.
#
# This file is the literal payload pushed to the remote shell, so keep it
# small and side-effect-free. Anything that fails should fail silently — a
# broken integration must not break the user's session.

[ -n "${__BEDTERM_INTEGRATION_INSTALLED:-}" ] && return 2>/dev/null

__bedterm_b64() {
    # Base64-encode stdin → stdout, stripping wrapping newlines so the result
    # fits inside an OSC payload. macOS and Linux base64 differ on wrap flags;
    # tr handles both.
    base64 2>/dev/null | tr -d '\n'
}

__bedterm_emit() {
    # OSC 133 ; <kind> [; <attr>]* BEL
    # $1 = kind (A/B/C/D); $2..$n = key=value strings.
    local kind=$1
    shift
    if [ $# -gt 0 ]; then
        local IFS=';'
        printf '\033]133;%s;%s\007' "$kind" "$*"
    else
        printf '\033]133;%s\007' "$kind"
    fi
}

if [ -n "${ZSH_VERSION:-}" ]; then
    # EPOCHREALTIME is a sub-second float; ships with the zsh/datetime module
    # which is loaded by default in 5.0+, but be defensive on minimal builds.
    zmodload -F zsh/datetime +b:EPOCHREALTIME 2>/dev/null
    autoload -Uz add-zsh-hook

    __bedterm_precmd() {
        local exit_code=$?
        if [ -n "${__BEDTERM_T0:-}" ]; then
            local dur_ms=$(( (EPOCHREALTIME - __BEDTERM_T0) * 1000 ))
            __bedterm_emit D "$exit_code" "dur=${dur_ms%.*}" \
                "cwd=$(printf '%s' "$PWD" | __bedterm_b64)"
            unset __BEDTERM_T0
        fi
        __bedterm_emit A "cwd=$(printf '%s' "$PWD" | __bedterm_b64)"
    }

    __bedterm_preexec() {
        # $1 is the expanded command line.
        __bedterm_emit C "cmd=$(printf '%s' "$1" | __bedterm_b64)"
        __BEDTERM_T0=$EPOCHREALTIME
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
        __bedterm_emit C "cmd=$(printf '%s' "$BASH_COMMAND" | __bedterm_b64)"
        # EPOCHSECONDS is bash 5+; fall back to date for older bash (macOS
        # ships 3.2 — most users on Linux will have 4+).
        __BEDTERM_T0=${EPOCHSECONDS:-$(date +%s)}
        __BEDTERM_IN_CMD=1
    }

    __bedterm_precmd() {
        local exit_code=$?
        if [ -n "${__BEDTERM_T0:-}" ]; then
            local now=${EPOCHSECONDS:-$(date +%s)}
            local dur_ms=$(( (now - __BEDTERM_T0) * 1000 ))
            __bedterm_emit D "$exit_code" "dur=$dur_ms" \
                "cwd=$(printf '%s' "$PWD" | __bedterm_b64)"
            unset __BEDTERM_T0
        fi
        __bedterm_emit A "cwd=$(printf '%s' "$PWD" | __bedterm_b64)"
        __BEDTERM_IN_CMD=0
    }

    trap '__bedterm_preexec' DEBUG
    PROMPT_COMMAND='__bedterm_precmd'"${PROMPT_COMMAND:+;$PROMPT_COMMAND}"

else
    # Unknown shell — quietly skip. The terminal still works; just no blocks.
    return 2>/dev/null
fi

__BEDTERM_INTEGRATION_INSTALLED=1
