# Maintainer: Joel Larson
pkgname=ghostty-voice
pkgver=0.1.0
pkgrel=1
pkgdesc="Voice dictation into the focused Ghostty terminal (GNOME/Wayland), local Whisper over Vulkan"
arch=('x86_64')
url="https://github.com/JoelLarson/ghostty-voice"
license=('custom:unlicensed')
# Runtime deps: ydotool(->ydotoold) injection, pipewire(->pw-record) + sox capture,
# wl-clipboard, libnotify(->notify-send), Vulkan ICD loader (GPU offload),
# libcanberra(->canberra-gtk-play) for the default theme-event cues.
depends=('ydotool' 'pipewire' 'sox' 'wl-clipboard' 'libnotify' 'vulkan-icd-loader' 'libcanberra')
makedepends=('cargo' 'cmake' 'git' 'vulkan-headers')
optdepends=('vulkan-tools: vulkaninfo, to confirm RADV sees your GPU'
            'sound-theme-freedesktop: theme sounds for the default audio cues')
install="${pkgname}.install"
# Release source: the tagged tree on GitHub. The vendored whisper.cpp build is
# fetched at build time (its own large LFS-free source tree), pinned by _whisper_tag
# for reproducibility. 'SKIP' until the tag's tarball hash is recorded for a release.
source=("${pkgname}-${pkgver}.tar.gz::${url}/archive/refs/tags/v${pkgver}.tar.gz")
sha256sums=('SKIP')

# The vendored whisper.cpp Vulkan build (large; pinned tag for reproducibility).
_whisper_url="https://github.com/ggerganov/whisper.cpp"
_whisper_tag="v1.7.4"

build() {
  cd "$srcdir/${pkgname}-${pkgver}"

  # 1. Rust workspace (the three binaries + the S1 skeleton).
  cargo build --release --locked

  # 2. Vendored whisper.cpp built with Vulkan (-DGGML_VULKAN=1).
  if [ ! -d whisper.cpp ]; then
    git clone --depth=1 --branch "$_whisper_tag" "$_whisper_url" whisper.cpp
  fi
  cmake -S whisper.cpp -B whisper.cpp/build \
    -DGGML_VULKAN=1 -DCMAKE_BUILD_TYPE=Release -DWHISPER_BUILD_TESTS=OFF
  cmake --build whisper.cpp/build -j --target whisper-server
}

package() {
  cd "$srcdir/${pkgname}-${pkgver}"

  install -Dm755 target/release/ghostty-voice      "$pkgdir/usr/bin/ghostty-voice"
  install -Dm755 target/release/ghostty-voiced     "$pkgdir/usr/bin/ghostty-voiced"
  install -Dm755 target/release/ghostty-voice-ctl  "$pkgdir/usr/bin/ghostty-voice-ctl"

  # whisper-server lives on PATH at the config default ([whisper].binary = "whisper-server").
  install -Dm755 whisper.cpp/build/bin/whisper-server "$pkgdir/usr/bin/whisper-server"

  # systemd USER unit (the daemon owns whisper-server's lifecycle, not systemd).
  install -Dm644 dist/ghostty-voiced.service \
    "$pkgdir/usr/lib/systemd/user/ghostty-voiced.service"
  install -Dm644 config.toml.example \
    "$pkgdir/usr/share/ghostty-voice/config.toml.example"
  install -Dm644 README.md "$pkgdir/usr/share/doc/ghostty-voice/README.md"
}

# The ~3 GB model is NOT packaged. It is fetched on first run by the daemon
# (downloading state + notify): ggml-large-v3.bin -> ~/.local/share/ghostty-voice/models/.
# Per-user setup (hotkeys, doctor, enable) is printed by the .install post-install hook.
