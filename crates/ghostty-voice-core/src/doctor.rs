//! Environment diagnostics (S7).
//!
//! The `doctor` command probes the environment (ydotoold socket, `input` group,
//! `/dev/uinput`) and reports actionable problems. The probing is the boundary;
//! turning probe results into named checks is pure and tested here.

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
    pub ydotool_socket_exists: bool,
    pub in_input_group: bool,
    pub uinput_present: bool,
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

    vec![
        check(
            "ydotoold socket",
            probes.ydotool_socket_exists,
            "ydotoold socket not found — start ydotoold and/or set YDOTOOL_SOCKET",
        ),
        check(
            "input group",
            probes.in_input_group,
            "user is not in the 'input' group — add it for /dev/uinput access, then re-login",
        ),
        check(
            "uinput device",
            probes.uinput_present,
            "/dev/uinput is missing — load the uinput kernel module",
        ),
    ]
}

/// True when every check passed.
pub fn all_ok(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.status == CheckStatus::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEALTHY: Probes = Probes {
        ydotool_socket_exists: true,
        in_input_group: true,
        uinput_present: true,
    };

    #[test]
    fn healthy_environment_has_no_problems() {
        let checks = evaluate(&HEALTHY);
        assert!(all_ok(&checks));
        assert_eq!(checks.len(), 3);
    }

    #[test]
    fn missing_ydotool_socket_is_flagged() {
        let probes = Probes {
            ydotool_socket_exists: false,
            ..HEALTHY
        };
        let checks = evaluate(&probes);
        assert!(!all_ok(&checks));
        let socket = checks.iter().find(|c| c.name == "ydotoold socket").unwrap();
        assert!(matches!(socket.status, CheckStatus::Problem(_)));
    }

    #[test]
    fn missing_input_group_is_flagged() {
        let probes = Probes {
            in_input_group: false,
            ..HEALTHY
        };
        let checks = evaluate(&probes);
        assert!(
            checks
                .iter()
                .any(|c| c.name == "input group" && c.status != CheckStatus::Ok)
        );
    }
}
