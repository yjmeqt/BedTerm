#!/usr/bin/env bash
# Copies BedTerm/Resources/DebugFixtures/*.cast into the built app bundle for
# Debug configurations only. Release builds skip the copy entirely.
set -euo pipefail
if [ "${CONFIGURATION:-}" = "Release" ]; then
  echo "Release: skipping DebugFixtures"
  exit 0
fi
SRC="$SRCROOT/BedTerm/Resources/DebugFixtures"
DST="$BUILT_PRODUCTS_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/DebugFixtures"
mkdir -p "$DST"
cp -R "$SRC/." "$DST/"
echo "Copied DebugFixtures to $DST"
