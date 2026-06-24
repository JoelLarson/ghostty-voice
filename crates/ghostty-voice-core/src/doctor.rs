//! Environment diagnostics.
//!
//! Now that `talk-to` is the sole interface there is no input device or
//! desktop-typing capability to check — the one thing a user can get wrong is not
//! having the daemon running. `doctor` probes whether the daemon's control socket
//! is reachable. The probing is the boundary; turning probe results into named
//! checks is pure and tested here.

/// The outcome of one check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Problem(String),
}

/// A named diagnostic result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    pub status: CheckStatus,
}

/// Boolean probe results gathered at the IO boundary.
#[derive(Debug, Clone, Copy)]
pub struct Probes {
    /// The daemon's control socket could be connected to — without it no command
    /// or `talk-to` registration can reach `ghostty-voiced`.
    pub daemon_reachable: bool,
}

/// Turn probe results into named checks with actionable problem messages.
pub fn evaluate(probes: &Probes) -> Vec<Check> {
    fn check(name: &'static str, ok: bool, problem: &str) -> Check {
        Check {
            name,
            status: if ok {
                CheckStatus::Ok
            } else {
                CheckStatus::Problem(problem.to_owned())
            },
        }
    }

    vec![check(
        "daemon",
        probes.daemon_reachable,
        "ghostty-voiced is not reachable on its control socket — start it \
         (e.g. `systemctl --user start ghostty-voiced`)",
    )]
}

/// True when every check passed.
pub fn all_ok(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.status == CheckStatus::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reachable_daemon_has_no_problems() {
        let checks = evaluate(&Probes {
            daemon_reachable: true,
        });
        assert!(all_ok(&checks));
        assert_eq!(checks.len(), 1);
    }

    #[test]
    fn an_unreachable_daemon_is_flagged() {
        let checks = evaluate(&Probes {
            daemon_reachable: false,
        });
        assert!(!all_ok(&checks));
        let daemon = checks.iter().find(|c| c.name == "daemon").unwrap();
        assert!(matches!(daemon.status, CheckStatus::Problem(_)));
    }
}
