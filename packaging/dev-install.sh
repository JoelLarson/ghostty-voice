#!/usr/bin/env bash
# dev-install.sh — a lightweight reinstall of the parts that change (the Rust
# binaries), driven by the packaged systemd unit.
#
# Builds the workspace, copies the four freshly-built binaries over the
# pacman-owned ones in /usr/bin (sudo, like `makepkg -si`), then restarts the
# daemon. The packaged unit's ExecStart is /usr/bin/ghostty-voiced, so
# `systemctl --user restart ghostty-voiced` runs the dev build — no ~/.local/bin
# symlink and no systemd ExecStart override. Idempotent: re-running with no
# source change rebuilds (a cargo no-op) and reinstalls identical bytes.
#
#     make dev    # = this tool
#
# Requires a packaged install present first (`makepkg -si`) so the systemd unit
# exists to restart.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bins=(ghostty-voice ghostty-voiced ghostty-voice-ctl talk-to)
src="$repo/target/release"

# Migrate a machine set up the old way: the prior dev tooling symlinked
# ~/.local/bin/* to target/ and wrote a systemd ExecStart override pinning the
# daemon there. That override would shadow the /usr/bin copy we install below, so
# remove the override.conf this repo wrote (and the now-empty drop-in dir) and
# reload. The ~/.local/bin symlinks are harmless leftovers on PATH; we leave them.
migrate_off_override() {
  local override_dir override
  override_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/ghostty-voiced.service.d"
  override="$override_dir/override.conf"
  if [ -f "$override" ]; then
    echo "migrating:  removing old dev override $override"
    rm -f "$override"
    rmdir "$override_dir" 2>/dev/null || true
    if command -v systemctl >/dev/null 2>&1; then
      systemctl --user daemon-reload || true
    fi
  fi
}

build() {
  echo "building:   cargo build --release"
  cargo build --release --manifest-path "$repo/Cargo.toml"
}

install_binaries() {
  echo "installing: ${bins[*]} -> /usr/bin (sudo)"
  for b in "${bins[@]}"; do
    sudo install -Dm755 "$src/$b" "/usr/bin/$b"
  done
}

restart_daemon() {
  echo "restarting: systemctl --user restart ghostty-voiced"
  systemctl --user restart ghostty-voiced
}

main() {
  # Build first: a failed build aborts here (set -e) before any system mutation,
  # so the install is all-or-nothing.
  build
  migrate_off_override
  install_binaries
  restart_daemon
  echo "done:       dev build installed to /usr/bin and daemon restarted"
}

main "$@"
