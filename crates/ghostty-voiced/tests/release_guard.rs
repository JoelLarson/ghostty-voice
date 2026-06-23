//! Test the binary-completeness guard script (`packaging/check-package-binaries.sh`,
//! TASK-12.2): it must pass when all four binaries are present in an install root
//! and fail — naming the missing one — when any is absent. Run from the test
//! suite so the release guard's logic stays green in CI-equivalent.
//!
//! This shells out to the real script (the script is the single source of the
//! check that `PKGBUILD`'s `package()` mirrors inline as the build-time gate).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

/// Absolute path to the guard script (relative to this crate's manifest).
fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging/check-package-binaries.sh")
        .canonicalize()
        .expect("guard script must exist at packaging/check-package-binaries.sh")
}

/// Build a throwaway install root containing exactly `bins` as executables under
/// `usr/bin`. Returned dir is removed on drop.
struct Root(PathBuf);
impl Root {
    fn with(bins: &[&str]) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gv-relguard-{}-{:?}",
            std::process::id(),
            thread::current().id(),
        ));
        let _ = fs::remove_dir_all(&dir);
        let bin_dir = dir.join("usr/bin");
        fs::create_dir_all(&bin_dir).unwrap();
        for bin in bins {
            let path = bin_dir.join(bin);
            fs::write(&path, b"#!/bin/sh\n").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self(dir)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ALL: [&str; 4] = [
    "ghostty-voice",
    "ghostty-voiced",
    "ghostty-voice-ctl",
    "talk-to",
];

fn run_guard(root: &Path) -> std::process::Output {
    Command::new("bash")
        .arg(script_path())
        .arg(root)
        .output()
        .expect("running the guard script")
}

#[test]
fn passes_when_all_four_binaries_are_present() {
    let root = Root::with(&ALL);
    let out = run_guard(&root.0);
    assert!(
        out.status.success(),
        "guard must pass with all binaries present; stderr: {}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("OK"));
}

#[test]
fn fails_and_names_the_missing_binary() {
    // talk-to omitted — exactly the 0.1.8 near-miss.
    let root = Root::with(&["ghostty-voice", "ghostty-voiced", "ghostty-voice-ctl"]);
    let out = run_guard(&root.0);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a missing binary must fail the guard with exit 1",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("talk-to"),
        "the guard must name the missing binary; stderr: {stderr}",
    );
}

#[test]
fn fails_when_every_binary_is_missing() {
    let root = Root::with(&[]);
    let out = run_guard(&root.0);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    for bin in ALL {
        assert!(stderr.contains(bin), "must list {bin} as missing: {stderr}");
    }
}

#[test]
fn a_non_executable_binary_does_not_count_as_present() {
    // A file that exists but isn't executable is an incomplete install.
    let root = Root::with(&["ghostty-voice", "ghostty-voiced", "ghostty-voice-ctl"]);
    let talk_to = root.0.join("usr/bin/talk-to");
    fs::write(&talk_to, b"not executable\n").unwrap();
    fs::set_permissions(&talk_to, fs::Permissions::from_mode(0o644)).unwrap();
    let out = run_guard(&root.0);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("talk-to"));
}

#[test]
fn usage_error_without_an_argument() {
    let out = Command::new("bash")
        .arg(script_path())
        .output()
        .expect("running the guard script");
    assert_eq!(
        out.status.code(),
        Some(2),
        "no argument must be a usage error (exit 2)",
    );
}
