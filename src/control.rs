use crate::probe::{directories, id, number, read, words};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

/// Named operations only. Neither the CLI nor the privileged helper accepts a file path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlRequest {
    TrackPoint {
        controller: String,
        setting: crate::devices::TrackPointSetting,
        value: u8,
    },
    BatteryThresholds {
        battery: String,
        start: u8,
        end: u8,
    },
    BatteryChargeMode {
        battery: String,
        mode: crate::battery::ChargeMode,
    },
    Brightness {
        device: String,
        value: u64,
    },
    PlatformProfile {
        profile: String,
    },
    FanAuto {},
}

#[derive(Debug, Serialize)]
pub struct Change {
    pub attribute: String,
    pub before: String,
    pub after: String,
}
#[derive(Debug, Serialize)]
pub struct Plan {
    pub request: ControlRequest,
    pub changes: Vec<Change>,
    pub notes: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct ApplyReport {
    pub applied: Vec<Change>,
    pub verified: bool,
}

fn component(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty()
            && s.len() <= 128
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-:.".contains(&b))
            && s != "."
            && s != "..",
        "Invalid device identifier"
    );
    Ok(())
}
fn control_value(path: &Path) -> Option<String> {
    let raw = read(path)?;
    if id(path) == "charge_behaviour" {
        crate::battery::parse_choices(&raw).ok().map(|v| v.active)
    } else {
        Some(raw)
    }
}
fn change(sys: &Path, path: PathBuf, after: String) -> Result<Option<Change>> {
    // /sys/class entries are symlinks into /sys/devices; reject escapes from the trusted root.
    let actual = path
        .canonicalize()
        .context("Control attribute disappeared or is unavailable")?;
    ensure!(
        actual.starts_with(sys.canonicalize()?),
        "Control attribute escapes hardware filesystem"
    );
    let before = control_value(&path).context("Cannot read control attribute")?;
    if before == after {
        return Ok(None);
    }
    Ok(Some(Change {
        attribute: path.strip_prefix(sys)?.to_string_lossy().into_owned(),
        before,
        after,
    }))
}

/// Read-only validation. Interface presence is not a guarantee of firmware acceptance.
pub fn plan(sys: &Path, request: ControlRequest) -> Result<Plan> {
    let mut paths: Vec<(PathBuf, String)> = vec![];
    let mut notes = vec![
        "Preview only. No settings changed.".into(),
        "Other power-management software may override these settings.".into(),
    ];
    match &request {
        ControlRequest::TrackPoint {
            controller,
            setting,
            value,
        } => {
            ensure!(
                *value > 0,
                "TrackPoint controls require 1..255; zero is not exposed"
            );
            let attribute = crate::devices::trackpoint_attribute(sys, controller, *setting)?;
            paths.push((attribute, value.to_string()));
            notes.push("This changes a PS/2 hardware parameter, not desktop pointer acceleration. Driver readback does not measure physical pointer response. Hotplug or other software can reset it.".into());
        }
        ControlRequest::BatteryChargeMode { battery, mode } => {
            component(battery)?;
            let p = sys.join("class/power_supply").join(battery);
            ensure!(
                read(p.join("type")).as_deref() == Some("Battery")
                    && number(p.join("present")) != Some(0),
                "Selected battery is unavailable"
            );
            let attribute = p.join("charge_behaviour");
            let modes = crate::battery::parse_choices(
                &read(&attribute).context("Charging mode interface unavailable")?,
            )?;
            ensure!(
                modes.available.iter().any(|v| v == mode.as_str()),
                "Requested charging mode is not advertised by this battery"
            );
            paths.push((attribute, mode.as_str().into()));
            notes.push("Automatic charging respects charge thresholds; inhibit-charge pauses charging while AC is attached.".into());
        }
        ControlRequest::BatteryThresholds {
            battery,
            start,
            end,
        } => {
            component(battery)?;
            ensure!(
                *start <= 99 && (1..=100).contains(end) && start < end,
                "Charge thresholds require 0 <= start < end <= 100"
            );
            let p = sys.join("class/power_supply").join(battery);
            ensure!(
                read(p.join("type")).as_deref() == Some("Battery"),
                "Selected device is not a battery"
            );
            ensure!(
                number(p.join("present")) != Some(0),
                "Battery is not present"
            );
            let start_path = p.join("charge_control_start_threshold");
            let end_path = p.join("charge_control_end_threshold");
            let old_start = number(&start_path).context("Start threshold unavailable")?;
            let old_end = number(&end_path).context("End threshold unavailable")?;
            ensure!(
                old_start <= 99 && (1..=100).contains(&old_end) && old_start < old_end,
                "Current thresholds are invalid; refusing to guess firmware semantics"
            );
            // Raising the start above the old end requires raising the end first.
            // Otherwise lower the start first so every intermediate pair stays valid.
            if u64::from(*start) >= old_end {
                paths.push((end_path, end.to_string()));
                paths.push((start_path, start.to_string()));
            } else {
                paths.push((start_path, start.to_string()));
                paths.push((end_path, end.to_string()));
            }
            notes.push("Thresholds can persist in firmware. Model-specific limits may reject a valid percentage pair.".into());
            notes.push("A two-attribute update is not atomic; failures report completed writes and require a fresh scan.".into());
        }
        ControlRequest::Brightness { device, value } => {
            component(device)?;
            let display = sys.join("class/backlight").join(device);
            let p = if display.is_dir() {
                display
            } else {
                ensure!(
                    device.ends_with("::kbd_backlight") || device == "tpacpi::thinklight",
                    "Only display and keyboard lighting are supported"
                );
                sys.join("class/leds").join(device)
            };
            let max = number(p.join("max_brightness")).context("Brightness device unavailable")?;
            ensure!(
                max > 0 && *value <= max,
                "Brightness must be between 0 and {max}"
            );
            paths.push((p.join("brightness"), value.to_string()));
        }
        ControlRequest::PlatformProfile { profile } => {
            ensure!(
                words(sys.join("firmware/acpi/platform_profile_choices")).contains(profile),
                "Profile is not advertised by this kernel"
            );
            paths.push((sys.join("firmware/acpi/platform_profile"), profile.clone()));
        }
        ControlRequest::FanAuto {} => {
            let candidates: Vec<_> = directories(sys.join("class/hwmon"))
                .into_iter()
                .filter(|p| {
                    read(p.join("name")).as_deref() == Some("thinkpad")
                        && p.join("pwm1_enable").exists()
                })
                .collect();
            ensure!(
                candidates.len() == 1,
                "Expected exactly one ThinkPad fan controller; found {}",
                candidates.len()
            );
            paths.push((candidates[0].join("pwm1_enable"), "2".into()));
            notes.push("Firmware automatic mode can be rejected on some ThinkPads; no manual fallback is attempted.".into());
        }
    }
    let mut changes = vec![];
    for (p, after) in paths {
        if let Some(c) = change(sys, p, after)? {
            changes.push(c);
        }
    }
    Ok(Plan {
        request,
        changes,
        notes,
    })
}

/// Execute a freshly validated request and read back each value. Used by the helper only.
/// Does not elevate, install services, or persist settings to disk.
pub fn execute(sys: &Path, request: ControlRequest) -> Result<ApplyReport> {
    execute_with(sys, request, |p, value| {
        let mut file = fs::OpenOptions::new().write(true).open(p)?;
        file.write_all(format!("{value}\n").as_bytes())?;
        Ok(())
    })
}

fn execute_with(
    sys: &Path,
    request: ControlRequest,
    mut write: impl FnMut(&Path, &str) -> Result<()>,
) -> Result<ApplyReport> {
    let validated = plan(sys, request)?;
    let mut applied = vec![];
    for c in validated.changes {
        let p = sys.join(&c.attribute);
        let operation = (|| -> Result<()> {
            ensure!(
                control_value(&p).as_deref() == Some(&c.before),
                "Setting changed since validation; scan again"
            );
            if let ControlRequest::BatteryThresholds { battery, .. } = &validated.request {
                let base = sys.join("class/power_supply").join(battery);
                ensure!(
                    number(base.join("present")) != Some(0),
                    "Battery was removed"
                );
                let start = if id(&p) == "charge_control_start_threshold" {
                    c.after.parse()?
                } else {
                    number(base.join("charge_control_start_threshold"))
                        .context("Start threshold disappeared")?
                };
                let end = if id(&p) == "charge_control_end_threshold" {
                    c.after.parse()?
                } else {
                    number(base.join("charge_control_end_threshold"))
                        .context("End threshold disappeared")?
                };
                ensure!(
                    start < end && end <= 100,
                    "Concurrent threshold change would produce an invalid pair"
                );
            }
            if let ControlRequest::BatteryChargeMode { battery, .. } = &validated.request {
                ensure!(
                    number(sys.join("class/power_supply").join(battery).join("present")) != Some(0),
                    "Battery was removed"
                );
            }
            if let ControlRequest::TrackPoint {
                controller,
                setting,
                ..
            } = &validated.request
            {
                crate::devices::trackpoint_attribute(sys, controller, *setting)?;
            }
            write(&p, &c.after)?;
            wait_for_readback(&p, &c.after)?;
            Ok(())
        })();
        if let Err(error) = operation {
            let done: Vec<_> = applied
                .iter()
                .map(|c: &Change| c.attribute.as_str())
                .collect();
            bail!("Failed at {}: {error:#}. Verified earlier writes: {:?}. The failing write may have taken effect; rescan before retrying. No automatic rollback attempted.", c.attribute, done);
        }
        applied.push(c);
    }
    for c in &applied {
        ensure!(control_value(&sys.join(&c.attribute)).as_deref() == Some(&c.after),
            "{} changed again after applying; another controller may be active. Rescan before retrying.", c.attribute);
    }
    Ok(ApplyReport {
        applied,
        verified: true,
    })
}

/// LED class writes may be queued by the kernel. Observe the result; never repeat the write.
fn wait_for_readback(path: &Path, expected: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut matches = 0;
    loop {
        let actual = control_value(path)
            .context("Control attribute disappeared or became invalid during verification")?;
        if actual == expected {
            matches += 1;
            if matches >= 2 {
                return Ok(());
            }
        } else {
            matches = 0;
        }
        ensure!(
            Instant::now() < deadline,
            "Write readback differs from requested value after waiting for hardware"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charging_mode_write_verifies_selected_value_in_kernel_style_list() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("class/power_supply/BAT0");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("type"), "Battery").unwrap();
        fs::write(base.join("present"), "1").unwrap();
        fs::write(
            base.join("charge_behaviour"),
            "[auto] inhibit-charge force-discharge",
        )
        .unwrap();
        let request = ControlRequest::BatteryChargeMode {
            battery: "BAT0".into(),
            mode: crate::battery::ChargeMode::InhibitCharge,
        };
        let preview = plan(root.path(), request.clone()).unwrap();
        assert_eq!(preview.changes[0].before, "auto");
        assert_eq!(preview.changes[0].after, "inhibit-charge");
        let mut writes = 0;
        let applied = execute_with(root.path(), request, |path, value| {
            writes += 1;
            assert_eq!(value, "inhibit-charge");
            fs::write(path, "auto [inhibit-charge] force-discharge")?;
            Ok(())
        })
        .unwrap();
        assert!(applied.verified);
        assert_eq!(writes, 1);
    }

    #[test]
    fn unsupported_charging_mode_is_rejected_before_a_write() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("class/power_supply/BAT0");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("type"), "Battery").unwrap();
        fs::write(base.join("charge_behaviour"), "[auto]").unwrap();
        assert!(plan(
            root.path(),
            ControlRequest::BatteryChargeMode {
                battery: "BAT0".into(),
                mode: crate::battery::ChargeMode::InhibitCharge
            }
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(base.join("charge_behaviour")).unwrap(),
            "[auto]"
        );
    }

    #[test]
    fn delayed_hardware_readback_is_observed_without_repeating_the_write() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("brightness");
        fs::write(&path, "1").unwrap();
        let delayed = path.clone();
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            fs::write(delayed, "2").unwrap();
        });
        wait_for_readback(&path, "2").unwrap();
        worker.join().unwrap();
    }
    #[test]
    fn reports_partial_failure_instead_of_claiming_success() {
        let root = tempfile::tempdir().unwrap();
        let battery = root.path().join("class/power_supply/BAT0");
        fs::create_dir_all(&battery).unwrap();
        for (name, value) in [
            ("type", "Battery"),
            ("present", "1"),
            ("charge_control_start_threshold", "70"),
            ("charge_control_end_threshold", "80"),
        ] {
            fs::write(battery.join(name), value).unwrap();
        }
        let mut count = 0;
        let result = execute_with(
            root.path(),
            ControlRequest::BatteryThresholds {
                battery: "BAT0".into(),
                start: 85,
                end: 95,
            },
            |p, value| {
                count += 1;
                if count == 2 {
                    bail!("simulated firmware rejection");
                }
                fs::write(p, value)?;
                Ok(())
            },
        );
        let error = result.unwrap_err().to_string();
        assert!(error.contains("simulated firmware rejection"));
        assert!(error.contains("Verified earlier writes"));
        assert_eq!(
            number(battery.join("charge_control_end_threshold")),
            Some(95)
        );
        assert_eq!(
            number(battery.join("charge_control_start_threshold")),
            Some(70)
        );
    }

    #[test]
    fn rejected_readback_is_an_error() {
        let root = tempfile::tempdir().unwrap();
        let p = root.path().join("class/backlight/panel");
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("brightness"), "50").unwrap();
        fs::write(p.join("max_brightness"), "100").unwrap();
        let result = execute_with(
            root.path(),
            ControlRequest::Brightness {
                device: "panel".into(),
                value: 60,
            },
            |_, _| Ok(()),
        );
        assert!(result.unwrap_err().to_string().contains("readback differs"));
    }

    #[test]
    fn concurrent_threshold_change_stops_the_second_write() {
        let root = tempfile::tempdir().unwrap();
        let battery = root.path().join("class/power_supply/BAT0");
        fs::create_dir_all(&battery).unwrap();
        for (name, value) in [
            ("type", "Battery"),
            ("present", "1"),
            ("charge_control_start_threshold", "70"),
            ("charge_control_end_threshold", "80"),
        ] {
            fs::write(battery.join(name), value).unwrap();
        }
        let mut count = 0;
        let result = execute_with(
            root.path(),
            ControlRequest::BatteryThresholds {
                battery: "BAT0".into(),
                start: 85,
                end: 95,
            },
            |p, value| {
                count += 1;
                fs::write(p, value)?;
                fs::write(battery.join("charge_control_start_threshold"), "60")?;
                Ok(())
            },
        );
        assert_eq!(count, 1);
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("changed since validation"));
    }
}
