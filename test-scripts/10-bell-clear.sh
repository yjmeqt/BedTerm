#!/usr/bin/env bash
# BEL + clear. `\a` must not crash the parser, and `clear` should only
# affect the running block (not blow away previous block bodies).
set -eu
echo "About to ring the bell..."
printf '\a'
sleep 0.3
echo "Now sending clear — only this block's body should go blank."
sleep 0.3
clear
echo "After clear: this is the only line that should remain."
