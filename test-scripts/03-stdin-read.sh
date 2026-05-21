#!/usr/bin/env bash
# Three sequential reads. Composer should preserve "03-stdin-read.sh" in
# its locked text while you type each answer into the running block.
set -eu
echo "Will ask you three questions — answers go via composer passthrough."
read -p "1) Your name: " name
read -p "2) Favourite editor: " editor
read -sp "3) Secret (silent input): " secret; echo
echo "Got name='$name' editor='$editor' secret-len=${#secret}"
