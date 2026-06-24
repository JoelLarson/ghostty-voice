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
#     make dev          # = this tool (install)
#     make clean-xdg    # = this tool --clean (wipe XDG user state)
#
# Requires a packaged install present first (`makepkg -si`) so the systemd unit
# exists to restart.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bins=(ghostty-voice ghostty-voiced ghostty-voice-ctl talk-to)
src="$repo/target/release"
assume_yes=0 # set by -y/--force; skips the --clean confirmation

usage() {
  cat <<'EOF'
usage: dev-install.sh [--clean [-y|--force]]

  (no args)   build the workspace, drift-guard the installed package files, copy
              the four binaries over /usr/bin (sudo), and restart the daemon.
  --clean     remove the ghostty-voice XDG user state — config, data (incl. the
              ~3 GB model), cache, and state — behind one confirmation. Performs
              no build or install. -y / --force skips the confirmation.
EOF
}

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

# Config drift-guard. Before any binary is copied, compare this repo's static
# package files against what is installed, so a dev install never silently ships
# binaries built against a newer config schema than the unit/example on disk. The
# pairs are repo-source::installed-path:
#
#   config.toml.example       -> /usr/share/ghostty-voice/config.toml.example
#   dist/ghostty-voiced.service -> /usr/lib/systemd/user/ghostty-voiced.service
#
# The maintainer's PERSONAL ~/.config/ghostty-voice/config.toml is deliberately
# never read or written here: with strict config, a stale personal config fails
# loudly at the restart, which is the intended signal — not something this tool
# papers over.
#
# An absent installed counterpart is just installed (nothing to clobber). A
# difference prompts once to overwrite ALL differing files; declining — or no TTY
# to ask — aborts the whole run before any binary is copied and before the
# restart, so the machine is left as a consistent version (or untouched).
drift_guard() {
  local pairs=(
    "$repo/config.toml.example::/usr/share/ghostty-voice/config.toml.example"
    "$repo/dist/ghostty-voiced.service::/usr/lib/systemd/user/ghostty-voiced.service"
  )
  local differ=()
  local pair src dst

  for pair in "${pairs[@]}"; do
    src="${pair%%::*}"
    dst="${pair##*::}"
    if [ ! -e "$dst" ]; then
      echo "installing: missing package file $dst (sudo)"
      sudo install -Dm644 "$src" "$dst"
    elif ! cmp -s "$src" "$dst"; then
      differ+=("$pair")
    fi
  done

  [ "${#differ[@]}" -eq 0 ] && return 0

  echo "drift:      installed package file(s) differ from this repo:"
  for pair in "${differ[@]}"; do
    src="${pair%%::*}"
    dst="${pair##*::}"
    echo
    # diff installed(old) -> repo(new): '<' is installed, '>' is this repo.
    diff --label "$dst (installed)" --label "$src (repo)" "$dst" "$src" || true
  done
  echo

  if [ ! -t 0 ]; then
    echo "abort:      package files differ and no TTY to confirm overwrite; nothing installed or restarted" >&2
    exit 1
  fi

  local reply
  read -r -p "overwrite installed configs? [y/N] " reply
  case "$reply" in
    [yY] | [yY][eE][sS]) ;;
    *)
      echo "abort:      declined; nothing installed or restarted" >&2
      exit 1
      ;;
  esac

  for pair in "${differ[@]}"; do
    src="${pair%%::*}"
    dst="${pair##*::}"
    echo "overwriting: $dst (sudo)"
    sudo install -Dm644 "$src" "$dst"
  done
  # The unit file may have changed — re-read user units before the restart.
  if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload || true
  fi
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

# Standalone maintenance action: wipe the ghostty-voice XDG user state. Honors
# the XDG base-dir overrides, falling back to the ~/.config etc. defaults. This
# is the old manual "delete the xdg files" step made a first-class, opt-in
# action; it does NOT build or install anything.
clean_xdg() {
  local dirs=(
    "${XDG_CONFIG_HOME:-$HOME/.config}/ghostty-voice"
    "${XDG_DATA_HOME:-$HOME/.local/share}/ghostty-voice"
    "${XDG_CACHE_HOME:-$HOME/.cache}/ghostty-voice"
    "${XDG_STATE_HOME:-$HOME/.local/state}/ghostty-voice"
  )
  local present=() d
  for d in "${dirs[@]}"; do
    [ -e "$d" ] && present+=("$d")
  done

  if [ "${#present[@]}" -eq 0 ]; then
    echo "clean:      nothing to remove (no ghostty-voice XDG state found)"
    return 0
  fi

  echo "clean:      will remove the following ghostty-voice XDG state:"
  for d in "${present[@]}"; do
    printf '              %s\t%s\n' "$(du -sh "$d" 2>/dev/null | cut -f1)" "$d"
  done
  echo "            removing the data dir deletes the ~3 GB model; it re-downloads"
  echo "            on the next daemon start."
  echo

  if [ "$assume_yes" -ne 1 ]; then
    if [ ! -t 0 ]; then
      echo "abort:      no TTY to confirm; pass --force to remove without prompting" >&2
      exit 1
    fi
    local reply
    read -r -p "remove all of the above? [y/N] " reply
    case "$reply" in
      [yY] | [yY][eE][sS]) ;;
      *)
        echo "abort:      declined; nothing removed" >&2
        exit 1
        ;;
    esac
  fi

  for d in "${present[@]}"; do
    echo "removing:   $d"
    rm -rf "$d"
  done
  echo "done:       XDG state removed"
}

install_flow() {
  # Build first: a failed build aborts here (set -e) before any system mutation,
  # so the install is all-or-nothing.
  build
  # Then the drift-guard: a decline (or no TTY) aborts before any binary is
  # copied and before the restart.
  drift_guard
  migrate_off_override
  install_binaries
  restart_daemon
  echo "done:       dev build installed to /usr/bin and daemon restarted"
}

main() {
  local do_clean=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --clean) do_clean=1 ;;
      -y | --yes | --force) assume_yes=1 ;;
      -h | --help)
        usage
        exit 0
        ;;
      *)
        echo "unknown option: $1" >&2
        usage >&2
        exit 2
        ;;
    esac
    shift
  done

  if [ "$do_clean" -eq 1 ]; then
    clean_xdg
  else
    install_flow
  fi
}

main "$@"
