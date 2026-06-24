#!/usr/bin/env bash
# dev-setup.sh — one-time setup for the fast local iteration loop.
#
# Points your shell and the systemd user service at dev builds in this repo's
# target/ dir, WITHOUT touching pacman-owned /usr/bin, so the inner loop is just:
#
#     cargo build --release && systemctl --user restart ghostty-voiced
#
# (or `make dev`). Idempotent — safe to re-run. To undo, see the notes printed at
# the end.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bindir="$HOME/.local/bin"
target="${1:-release}" # pass "debug" to symlink the faster-to-compile debug build
src="$repo/target/$target"
bins=(ghostty-voice ghostty-voiced ghostty-voice-ctl talk-to)

echo "repo:        $repo"
echo "symlinking:  $bindir/<bin> -> $src/<bin>"

mkdir -p "$bindir"
for b in "${bins[@]}"; do
  ln -sfn "$src/$b" "$bindir/$b"
done

# systemd user override: run the symlinked dev daemon instead of /usr/bin.
override_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/ghostty-voiced.service.d"
override="$override_dir/override.conf"
mkdir -p "$override_dir"
cat >"$override" <<EOF
# Written by packaging/dev-setup.sh — run the dev build from ~/.local/bin.
# Delete this file (and run 'systemctl --user daemon-reload') to return to the
# packaged /usr/bin/ghostty-voiced.
[Service]
ExecStart=
ExecStart=%h/.local/bin/ghostty-voiced
EOF
echo "override:    $override"

if command -v systemctl >/dev/null 2>&1; then
  systemctl --user daemon-reload || true
fi

echo
echo "Done. Build, then (re)start the daemon:"
echo "    cargo build --$target && systemctl --user restart ghostty-voiced"
echo "    # or: make dev"
echo
case ":$PATH:" in
  *":$bindir:"*) ;;
  *) echo "NOTE: $bindir is not on your PATH — add it so talk-to/ghostty-voice-ctl resolve to the dev build." ;;
esac
echo "To undo: rm ${bins[*]/#/$bindir/} \"$override\" && systemctl --user daemon-reload"
