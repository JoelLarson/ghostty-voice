# Developer convenience targets. See packaging/dev-setup.sh and packaging/RELEASE.md.
.PHONY: setup setup-debug dev dev-debug check

# One-time: symlink ~/.local/bin -> target/ and install the systemd user override.
setup:
	bash packaging/dev-setup.sh release

setup-debug:
	bash packaging/dev-setup.sh debug

# Inner loop: rebuild and restart the daemon. The symlinks make the swap a no-op.
dev:
	cargo build --release
	systemctl --user restart ghostty-voiced

# Faster compiles (the heavy compute is in whisper-server, a separate process).
# Use after `make setup-debug`.
dev-debug:
	cargo build
	systemctl --user restart ghostty-voiced

# The full gate (matches CI-equivalent).
check:
	cargo test
	cargo clippy --all-targets
	cargo fmt --check
