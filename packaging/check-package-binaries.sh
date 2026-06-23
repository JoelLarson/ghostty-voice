#!/usr/bin/env bash
# Binary-completeness guard (TASK-12.2).
#
# Verify a built package's `$pkgdir` (or any install root) contains ALL FOUR
# ghostty-voice binaries under usr/bin. Fails (exit 1), listing any that are
# missing, so a release can never silently ship without one — the 0.1.8 release
# nearly went out without `talk-to` (it was missing from PKGBUILD's package()).
#
# Usage:
#   check-package-binaries.sh <pkgdir-or-install-root>
# e.g. against a freshly built package:
#   bash check-package-binaries.sh "$(pwd)/pkg/ghostty-voice"
# or against the live system:
#   bash check-package-binaries.sh /
#
# Exit codes: 0 = all present, 1 = one or more missing, 2 = usage error.
#
# PKGBUILD's package() runs the same check inline as the authoritative build-time
# gate; keep the required-binary list here in sync with it.
set -euo pipefail

root="${1:-}"
if [ -z "$root" ]; then
  echo "usage: $0 <pkgdir-or-install-root>" >&2
  exit 2
fi

# The four binaries a complete ghostty-voice package must install.
required=(ghostty-voice ghostty-voiced ghostty-voice-ctl talk-to)

missing=()
for bin in "${required[@]}"; do
  if [ ! -x "$root/usr/bin/$bin" ]; then
    missing+=("$bin")
  fi
done

if [ "${#missing[@]}" -ne 0 ]; then
  echo "FAIL: package is missing required binaries: ${missing[*]}" >&2
  exit 1
fi

echo "OK: all ${#required[@]} binaries present (${required[*]})"
