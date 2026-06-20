# Maintainer: Joel Larson
pkgname=ghostty-voice
pkgver=0.0.0
pkgrel=1
pkgdesc="Voice dictation into the focused Ghostty terminal (GNOME/Wayland), local Whisper over Vulkan"
arch=('x86_64')
url="https://github.com/JoelLarson/ghostty-voice"
license=('custom:unlicensed')
depends=('ydotool' 'pipewire' 'sox' 'wl-clipboard' 'libnotify' 'vulkan-icd-loader')
makedepends=('cargo' 'cmake' 'git' 'vulkan-headers')
# Build from the local checkout. For a release, switch to a git+https source on a tag.
source=()
sha256sums=()

# The vendored whisper.cpp Vulkan build (large; pinned for reproducibility).
_whisper_url="https://github.com/ggerganov/whisper.cpp"

build() {
  cd "$startdir"

  # 1. Rust workspace (three binaries).
  cargo build --release --locked

  # 2. Vendored whisper.cpp built with Vulkan (-DGGML_VULKAN=1).
  if [ ! -d whisper.cpp ]; then
    git clone --depth=1 "$_whisper_url" whisper.cpp
  fi
  cmake -S whisper.cpp -B whisper.cpp/build \
    -DGGML_VULKAN=1 -DCMAKE_BUILD_TYPE=Release -DWHISPER_BUILD_TESTS=OFF
  cmake --build whisper.cpp/build -j --target whisper-server
}

package() {
  cd "$startdir"

  install -Dm755 target/release/ghostty-voice      "$pkgdir/usr/bin/ghostty-voice"
  install -Dm755 target/release/ghostty-voiced     "$pkgdir/usr/bin/ghostty-voiced"
  install -Dm755 target/release/ghostty-voice-ctl  "$pkgdir/usr/bin/ghostty-voice-ctl"

  # whisper-server lives next to the binary path in config (default: whisper-server on PATH).
  install -Dm755 whisper.cpp/build/bin/whisper-server "$pkgdir/usr/bin/whisper-server"

  install -Dm644 dist/ghostty-voiced.service \
    "$pkgdir/usr/lib/systemd/user/ghostty-voiced.service"
  install -Dm644 config.toml.example \
    "$pkgdir/usr/share/ghostty-voice/config.toml.example"
  install -Dm644 README.md "$pkgdir/usr/share/doc/ghostty-voice/README.md"
}

# The ~3 GB model is NOT packaged. It is fetched on first run (see README):
#   ggml-large-v3.bin -> ~/.local/share/ghostty-voice/models/
# Post-install hint: run `ghostty-voice-ctl install-hotkeys` and `ghostty-voice-ctl doctor`.
