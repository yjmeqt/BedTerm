#!/usr/bin/env bash
# Real-world style mock: imitates `ls -la --color`, `git status`, `grep`,
# `man` headings. Useful for spotting regressions in actual CLI tools'
# typical SGR usage rather than synthetic test rows.

esc=$'\033'

# --- ls --color: bold blue dirs, bold green executables, regular files ---
printf '\n# ls -la --color\n'
printf 'drwxr-xr-x  3 user  staff   96 May 23 13:00 %sbin%s\n' "${esc}[01;34m" "${esc}[0m"
printf 'drwxr-xr-x  4 user  staff  128 May 23 13:00 %sproject%s\n' "${esc}[01;34m" "${esc}[0m"
printf -- '-rwxr-xr-x  1 user  staff  512 May 23 13:00 %sbuild.sh%s\n' "${esc}[01;32m" "${esc}[0m"
printf -- '-rw-r--r--  1 user  staff  340 May 23 13:00 README.md\n'
printf -- '-rw-r--r--  1 user  staff  120 May 23 13:00 .gitignore\n'

# --- git status: bold red unstaged, bold green staged, branch label bold ---
printf '\n# git status\n'
printf 'On branch %sfeature/sgr-rendering%s\n' "${esc}[1m" "${esc}[0m"
printf 'Changes not staged for commit:\n'
printf '  %smodified:   src/renderer/atlas.rs%s\n' "${esc}[31m" "${esc}[0m"
printf '  %smodified:   src/renderer/mod.rs%s\n' "${esc}[31m" "${esc}[0m"
printf 'Changes to be committed:\n'
printf '  %snew file:   prd/bedterm/terminal-view.xml%s\n' "${esc}[32m" "${esc}[0m"

# --- grep --color: bold red matches, line numbers in green ---
printf '\n# grep -n --color "rasterize" *.rs\n'
printf '%sglyph_raster.rs%s%s:%s%s75%s:%spub fn %srasterize%s(ch: char, ...)\n' \
    "${esc}[35m" "${esc}[0m" \
    "${esc}[36m" "${esc}[0m" \
    "${esc}[32m" "${esc}[0m" \
    "${esc}[36m" "${esc}[1;31m" "${esc}[0m"

# --- man page: bold section heads, underlined option args, italic notes ---
printf '\n# man-style\n'
printf '%sSYNOPSIS%s\n' "${esc}[1m" "${esc}[0m"
printf '       command [%s-v%s] [%s-h%s] [%sfile%s ...]\n' \
    "${esc}[4m" "${esc}[0m" \
    "${esc}[4m" "${esc}[0m" \
    "${esc}[4m" "${esc}[0m"
printf '\n%sDESCRIPTION%s\n' "${esc}[1m" "${esc}[0m"
printf '       Reads the named %sfile%s and emits %sstuff%s.  %sSee ALSO below.%s\n' \
    "${esc}[4m" "${esc}[0m" \
    "${esc}[1m" "${esc}[0m" \
    "${esc}[3m" "${esc}[0m"

# --- diff -u: bold add/remove headers + colour ---
printf '\n# diff -u\n'
printf '%sdiff --git a/foo.rs b/foo.rs%s\n' "${esc}[1m" "${esc}[0m"
printf '%sindex 1234abc..5678def 100644%s\n' "${esc}[1m" "${esc}[0m"
printf '%s--- a/foo.rs%s\n' "${esc}[1m" "${esc}[0m"
printf '%s+++ b/foo.rs%s\n' "${esc}[1m" "${esc}[0m"
printf '%s@@ -10,3 +10,4 @@%s\n' "${esc}[36m" "${esc}[0m"
printf ' regular context\n'
printf '%s-removed line%s\n' "${esc}[31m" "${esc}[0m"
printf '%s+added line%s\n' "${esc}[32m" "${esc}[0m"
printf '%s+another added line, %sbold inside%s%s\n' "${esc}[32m" "${esc}[1m" "${esc}[0m" "${esc}[0m"
