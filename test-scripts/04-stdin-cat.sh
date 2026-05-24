#!/usr/bin/env bash
# Plain `cat` echo loop. Type lines via passthrough — each Enter sends a
# line back; Ctrl-D ends the block. Validates that the inline status row
# stays during a long stdin-reading command.
set -eu
echo "Type lines; each Enter echoes back. Ctrl-D (or close) to end."
cat
