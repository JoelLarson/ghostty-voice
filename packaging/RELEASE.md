# Releasing ghostty-voice to the AUR

A repeatable procedure so a release can't ship broken or unverified. It exists
because the 0.1.8 release hit two avoidable problems: the new `talk-to` binary was
nearly omitted from `package()`, and `sha256sums` sat at `SKIP` (an unverified
download). Both are caught by following the steps below — the binary-completeness
guard fails the build if any binary is missing, and `updpkgsums` replaces `SKIP`
with a real hash.

All commands run from `packaging/` unless noted.

## Local development (not a release)

Three layers, matched to how often the thing they rebuild actually changes — so
you never run the full release pipeline just to test a code change:

1. **Inner loop (Rust changes — the common case).** `make dev` (wraps
   `packaging/dev-install.sh`): it `cargo build --release`s the workspace, copies
   the four freshly-built binaries over the pacman-owned ones in `/usr/bin`
   (`sudo install -Dm755`, the same paths `makepkg -si` writes), then
   `systemctl --user restart ghostty-voiced`:
   ```sh
   make dev
   ```
   The packaged unit's `ExecStart` is `/usr/bin/ghostty-voiced`, so the restart
   runs your dev build — no `~/.local/bin` symlink and no systemd override. A
   failed build installs nothing (all-or-nothing). This is a lightweight reinstall
   of just the binaries, so a packaged install must already be present (layer
   2/3) for the systemd unit to exist; re-running with no source change reinstalls
   identical bytes (idempotent).

   **Config drift-guard.** Before copying any binary, the tool compares this
   repo's static package files — `config.toml.example` and
   `dist/ghostty-voiced.service` — against their installed counterparts
   (`/usr/share/ghostty-voice/config.toml.example`,
   `/usr/lib/systemd/user/ghostty-voiced.service`). If a file differs it shows the
   diff and asks `overwrite installed configs? [y/N]`; **declining (or no TTY)
   aborts before any binary is copied or the daemon restarted** — you get a
   consistent version or nothing. Accepting `sudo`-overwrites the installed
   file(s), reloads the user manager, and proceeds. An absent installed
   counterpart is just installed. Your *personal*
   `~/.config/ghostty-voice/config.toml` is never read or touched here — a stale
   one fails loudly at the restart (strict config), which is the point.

2. **Packaged integration test (you changed the unit/install hook/deps, or want a
   pacman-tracked install).** `packaging/ghostty-voice-git/` is a VCS PKGBUILD
   that builds the latest *committed* HEAD of the local repo — `pkgver()` from
   `git describe`, so **no version bump, tag, or checksums**:
   ```sh
   makepkg -fi    # from packaging/ghostty-voice-git/   (or: paru -Bi packaging/ghostty-voice-git)
   ```
   This is also the future AUR `-git` companion (swap its `source` to the GitHub
   remote to publish).

3. **Release → AUR (rare).** The steps below, using the pinned-tarball PKGBUILD.

**Config is strict.** A `~/.config/ghostty-voice/config.toml` that exists but is
invalid — broken TOML *or* an unknown key (a typo, or a section left over from a
previous version) — makes the daemon **refuse to start** (and `reload` reject the
change, keeping the running config), rather than silently falling back to
defaults. So when a release removes a config field, that is a deliberate, loud
fix-the-config event for users — note it in the changelog.

## One-time setup

- Clone the AUR package repo somewhere alongside this one:
  ```sh
  git clone ssh://aur@aur.archlinux.org/ghostty-voice.git ../aur-ghostty-voice
  ```

## Per release

1. **Bump the version.** In `packaging/PKGBUILD`, set `pkgver=X.Y.Z` and reset
   `pkgrel=1` (bump only `pkgrel` for a packaging-only rebuild of the same
   upstream version).

2. **Commit, tag, and push the source repo.** The PKGBUILD `source=()` points at
   the GitHub tag tarball (`.../refs/tags/v${pkgver}.tar.gz`), so the tag must
   exist *before* checksums are computed:
   ```sh
   git commit -am "release vX.Y.Z"
   git tag vX.Y.Z
   git push origin main --tags
   ```

3. **Real checksums (never `SKIP`).** Download the tag tarball and write its real
   sha256 into `sha256sums=()`:
   ```sh
   updpkgsums
   ```

4. **Build and run the binary-completeness guard.** `package()` verifies all four
   binaries — `ghostty-voice`, `ghostty-voiced`, `ghostty-voice-ctl`, `talk-to` —
   are installed and **fails the build** if any is missing:
   ```sh
   makepkg -f
   ```
   To check a built `pkgdir` (or any install root) directly with the same logic:
   ```sh
   bash check-package-binaries.sh "$(pwd)/pkg/ghostty-voice"
   # or against the live system after install:
   bash check-package-binaries.sh /
   ```

5. **Regenerate `.SRCINFO`** (the AUR metadata; it must match the PKGBUILD):
   ```sh
   makepkg --printsrcinfo > .SRCINFO
   ```

6. **Push to the AUR.** Copy the release files into the AUR clone and push:
   ```sh
   cp PKGBUILD .SRCINFO ghostty-voice.install ../aur-ghostty-voice/
   git -C ../aur-ghostty-voice commit -am "vX.Y.Z"
   git -C ../aur-ghostty-voice push
   ```
   (The completeness guard is inline in `PKGBUILD`'s `package()`, so the AUR repo
   needs only these files. `check-package-binaries.sh` lives in this source repo
   for dry-run / post-install verification.)

## Dry run (no publish)

Run steps 1 and 3–5 **without** step 2's `push` and without step 6 to validate the
build, the real checksums, the regenerated `.SRCINFO`, and the completeness guard
entirely locally. (`updpkgsums` in step 3 needs the tag tarball to exist; for a
pure local dry-run, build from a local source instead.)
