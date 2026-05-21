#!/usr/bin/env bash
# Mock Claude-style TUI — boxed banner, streaming faux-AI reply, prompt
# strip at the bottom. Does NOT enter alt-screen (matches real claude
# inline rendering), so this exercises the in-block long-running case
# with passthrough composer.
#
# What to verify in BedTerm while it runs:
#   1. Block stays in the list, body grows downward as the box re-renders.
#   2. Composer collapses to the slim "Forwarding keys" status row.
#   3. Each typed line streams chunk-by-chunk (validates frequent feeds).
#   4. `/quit` exits cleanly; block seals with exit 0.
set -eu

cyan="\033[36m"; mag="\033[35m"; dim="\033[2m"; bold="\033[1m"; reset="\033[0m"

banner() {
    printf '%b' "${mag}╭──────────────────────────────────────────╮${reset}\n"
    printf '%b' "${mag}│${reset}        ${bold}Mock Claude${reset} — pretend AI shell        ${mag}│${reset}\n"
    printf '%b' "${mag}│${reset}        Opus 4.7 (1M context)             ${mag}│${reset}\n"
    printf '%b' "${mag}╰──────────────────────────────────────────╯${reset}\n"
}

stream_reply() {
    local prompt="$1"
    # A fake "thinking" pause, then a paragraph streamed a few chars at a
    # time so you can watch BlockGrid append in near-real-time.
    printf '%b' "${dim}thinking…${reset}"
    sleep 0.4
    printf '\r%b' "${dim}                ${reset}\r"
    local reply="Here's a thought about \"$prompt\": the per-block VTE in BedTerm \
should now render this exact paragraph one syllable at a time, with no \
backslash leaking into column zero and no neighbour block stealing the header. \
If everything works, /quit returns you to a fresh prompt."
    local i=0 chunk
    while [ $i -lt ${#reply} ]; do
        chunk="${reply:i:6}"
        printf '%b' "${cyan}$chunk${reset}"
        i=$(( i + 6 ))
        sleep 0.03
    done
    printf '\n'
}

banner
printf '%b' "${dim}Type a message and press Enter — \"/quit\" to leave.${reset}\n\n"

while :; do
    printf '%b' "${bold}❯${reset} "
    if ! IFS= read -r line; then
        printf '\n'; break
    fi
    case "$line" in
        /quit|/exit) printf '%b' "${dim}bye${reset}\n"; break ;;
        '') continue ;;
        *) stream_reply "$line" ;;
    esac
done
