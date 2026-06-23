# Releasing ghostty-voice to the AUR

A repeatable procedure so a release can't ship broken or unverified. It exists
because the 0.1.8 release hit two avoidable problems: the new `talk-to` binary was
nearly omitted from `package()`, and `sha256sums` sat at `SKIP` (an unverified
download). Both are caught by following the steps below — the binary-completeness
guard fails the build if any binary is missing, and `updpkgsums` replaces `SKIP`
with a real hash.

All commands run from `packaging/` unless noted.

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
