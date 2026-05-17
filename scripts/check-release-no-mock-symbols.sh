#!/usr/bin/env bash
# Asserts that a Release-flavoured BedTermCore.xcframework contains no
# bt_mock_tty_* exported symbols. Run from CI on every PR.
#
# Usage: ./scripts/check-release-no-mock-symbols.sh
#   Run *after* a Release build (./scripts/build-rust-xcframework.sh Release).
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FW="$REPO_ROOT/BedTermKit/BinaryFrameworks/BedTermCore.xcframework"
fail=0
for slice in ios-arm64 ios-arm64-simulator macos-arm64; do
  lib="$FW/$slice/libbedterm_core.a"
  if [ ! -f "$lib" ]; then
    echo "missing $lib"
    exit 1
  fi
  if nm -gj "$lib" 2>/dev/null | grep -q '_bt_mock_tty_'; then
    echo "FAIL: $slice exports bt_mock_tty_* symbols"
    nm -gj "$lib" | grep '_bt_mock_tty_' || true
    fail=1
  else
    echo "OK: $slice clean"
  fi
done
exit $fail
