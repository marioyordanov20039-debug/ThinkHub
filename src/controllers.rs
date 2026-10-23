//! Advisory discovery and conservative ownership checks. Never stops another service.
use crate::ControlRequest;
use serde::Serialize;
use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const UNITS: &[&str] = &[
    "tlp.service",
    "power-profiles-daemon.service",
    "vantaged.service",
    "tuned.service",
    "auto-cpufreq.service",
    "thinkfan.service",
    "fancontrol.service",
    "nbfc_service.service",
];

#[derive(Debug, Clone, Serialize)]
pub struct Controller {
    pub unit: String,
    pub state: String,
}

pub fn parse_services(output: &str) -> Vec<Controller> {
    UNITS
        .iter()
        .map(|unit| {
            let block = output
                .split("\n\n")
                .find(|block| block.lines().any(|line| line == format!("Id={unit}")));
            let state = block
                .and_then(|b| b.lines().find_map(|l| l.strip_prefix("ActiveState=")))
                .unwrap_or("unknown");
            Controller {
                unit: (*unit).into(),
                state: state.into(),
            }
        })
        .collect()
}

pub fn discover() -> Vec<Controller> {
    let unknown = || parse_services("");
    let Some(program) = ["/usr/bin/systemctl", "/bin/systemctl"]
        .into_iter()
        .find(|p| Path::new(p).is_file())
    else {
        return unknown();
    };
    let Ok(mut child) = Command::new(program)
        .args(["show", "--no-pager", "--property=Id,LoadState,ActiveState"])
        .args(UNITS)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return unknown();
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return unknown();
            }
        }
    }
    // Output is bounded by the fixed eight-unit query. Missing units may yield nonzero status.
    child
        .wait_with_output()
        .ok()
        .map(|v| parse_services(&String::from_utf8_lossy(&v.stdout)))
        .unwrap_or_else(unknown)
}

pub fn blockers(request: &ControlRequest, controllers: &[Controller]) -> Vec<String> {
    let owners: &[&str] = match request {
        ControlRequest::BatteryThresholds { .. } | ControlRequest::BatteryChargeMode { .. } => &[
            "tlp.service",
            "tuned.service",
            "auto-cpufreq.service",
            "vantaged.service",
        ],
        ControlRequest::PlatformProfile { .. } => &[
            "power-profiles-daemon.service",
            "tlp.service",
            "tuned.service",
            "auto-cpufreq.service",
            "vantaged.service",
        ],
        ControlRequest::FanAuto {} => &[
            "thinkfan.service",
            "fancontrol.service",
            "nbfc_service.service",
            "vantaged.service",
        ],
        ControlRequest::Brightness { .. } | ControlRequest::TrackPoint { .. } => &[],
    };
    owners
        .iter()
        .filter_map(|unit| {
            let state = controllers
                .iter()
                .find(|c| &c.unit == unit)
                .map(|c| c.state.as_str())
                .unwrap_or("unknown");
            match state {
                "inactive" | "failed" => None,
                "unknown" => Some(format!(
                    "Cannot determine whether {unit} owns this setting; direct write blocked"
                )),
                _ => Some(format!(
                    "{unit} is {state} and may manage this setting; direct write blocked"
                )),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_system_bus_is_unknown_not_permission_to_write() {
        let state = parse_services("");
        assert!(!blockers(&ControlRequest::FanAuto {}, &state).is_empty());
        assert!(blockers(
            &ControlRequest::Brightness {
                device: "panel".into(),
                value: 1
            },
            &state
        )
        .is_empty());
    }
    #[test]
    fn power_daemon_blocks_profiles_but_not_keyboard_lighting() {
        let state = parse_services("Id=power-profiles-daemon.service\nLoadState=loaded\nActiveState=active\n\nId=tlp.service\nLoadState=not-found\nActiveState=inactive\n");
        assert!(blockers(
            &ControlRequest::PlatformProfile {
                profile: "balanced".into()
            },
            &state
        )
        .iter()
        .any(|s| s.contains("power-profiles-daemon.service is active")));
        assert!(blockers(
            &ControlRequest::Brightness {
                device: "tpacpi::kbd_backlight".into(),
                value: 1
            },
            &state
        )
        .is_empty());
        assert_eq!(
            state
                .iter()
                .find(|c| c.unit == "tlp.service")
                .unwrap()
                .state,
            "inactive"
        );
    }
}
