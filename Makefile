# Developer convenience targets. See packaging/dev-install.sh and packaging/RELEASE.md.
.PHONY: dev check

# Inner loop: build the workspace, copy the dev binaries over /usr/bin (sudo),
# and restart the daemon. The packaged unit runs /usr/bin/ghostty-voiced, so the
# restart picks up the dev build — no symlink, no systemd override.
dev:
	bash packaging/dev-install.sh

# The full gate (matches CI-equivalent).
check:
	cargo test
	cargo clippy --all-targets
	cargo fmt --check
