use std::{fs, os::unix::fs::symlink, path::Path, process::Command};
use tempfile::TempDir;
use thinkpad_dashboard::{execute, plan, scan, ControlRequest};

fn put(root: &Path, path: &str, value: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, value).unwrap();
}
fn battery(root: &Path, name: &str, start: &str, end: &str) {
    for (attribute, value) in [
        ("type", "Battery"),
        ("present", "1"),
        ("charge_control_start_threshold", start),
        ("charge_control_end_threshold", end),
    ] {
        put(
            root,
            &format!("class/power_supply/{name}/{attribute}"),
            value,
        );
    }
}
fn thresholds(start: u8, end: u8) -> ControlRequest {
    ControlRequest::BatteryThresholds {
        battery: "BAT0".into(),
        start,
        end,
    }
}

#[test]
fn dual_batteries_keep_individual_energy_health() {
    let root = TempDir::new().unwrap();
    for (id, full, design) in [
        ("BAT0", "11360000", "23940000"),
        ("BAT1", "19440000", "24050000"),
    ] {
        battery(root.path(), id, "0", "100");
        put(
            root.path(),
            &format!("class/power_supply/{id}/energy_full"),
            full,
        );
        put(
            root.path(),
            &format!("class/power_supply/{id}/energy_full_design"),
            design,
        );
    }
    let s = scan(root.path());
    assert_eq!(s.batteries.len(), 2);
    assert!((s.batteries[0].health_percent.unwrap() - 47.4519).abs() < 0.01);
    assert!((s.batteries[1].health_percent.unwrap() - 80.8316).abs() < 0.01);
    assert_eq!(s.batteries[0].energy_full_wh, Some(11.36));
}

#[test]
fn older_charge_based_battery_uses_matching_units() {
    let root = TempDir::new().unwrap();
    battery(root.path(), "BAT0", "0", "100");
    for (key, value) in [
        ("charge_full", "3000000"),
        ("charge_full_design", "4000000"),
        ("charge_now", "1500000"),
        ("voltage_now", "12000000"),
        ("current_now", "-1000000"),
    ] {
        put(
            root.path(),
            &format!("class/power_supply/BAT0/{key}"),
            value,
        );
    }
    let s = scan(root.path());
    assert_eq!(s.batteries[0].health_percent, Some(75.0));
    assert_eq!(s.batteries[0].percent, Some(50.0));
    assert_eq!(s.batteries[0].power_w, Some(12.0));
    assert_eq!(s.batteries[0].energy_full_wh, None);
    assert_eq!(s.batteries[0].health_basis.as_deref(), Some("charge"));
}

#[test]
fn missing_malformed_and_zero_design_are_unknown_not_zero_health() {
    let root = TempDir::new().unwrap();
    battery(root.path(), "BAT0", "0", "100");
    put(
        root.path(),
        "class/power_supply/BAT0/energy_full",
        "garbled",
    );
    put(
        root.path(),
        "class/power_supply/BAT0/energy_full_design",
        "0",
    );
    let s = scan(root.path());
    assert_eq!(s.batteries[0].health_percent, None);
    assert_eq!(s.batteries[0].percent, None);
    assert!(!serde_json::to_string(&s).unwrap().contains("NaN"));
}

#[test]
fn removed_battery_has_no_stale_metrics_or_controls() {
    let root = TempDir::new().unwrap();
    battery(root.path(), "BAT0", "75", "80");
    put(root.path(), "class/power_supply/BAT0/present", "0");
    put(root.path(), "class/power_supply/BAT0/capacity", "100");
    let s = scan(root.path());
    assert_eq!(s.batteries[0].percent, None);
    assert_eq!(s.batteries[0].charge_start_percent, None);
    assert_eq!(s.capabilities[0].status, "unavailable");
    assert!(plan(root.path(), thresholds(75, 80)).is_err());
}

#[test]
fn hwmon_numbering_is_discovered_and_invalid_temperatures_ignored() {
    let root = TempDir::new().unwrap();
    for (key, value) in [
        ("name", "thinkpad"),
        ("temp1_input", "48000"),
        ("temp1_label", "CPU"),
        ("temp2_input", "-128000"),
        ("fan1_input", "2700"),
        ("pwm1_enable", "1"),
    ] {
        put(root.path(), &format!("class/hwmon/hwmon37/{key}"), value);
    }
    let s = scan(root.path());
    assert_eq!(s.sensors.len(), 2);
    assert!(s
        .sensors
        .iter()
        .any(|s| s.label == "CPU" && s.value == 48.0));
    assert_eq!(
        s.diagnostics
            .iter()
            .filter(|s| s.contains("Invalid sensor"))
            .count(),
        1
    );
    let p = plan(root.path(), ControlRequest::FanAuto {}).unwrap();
    assert_eq!(p.changes[0].attribute, "class/hwmon/hwmon37/pwm1_enable");
    assert_eq!(p.changes[0].after, "2");
}

#[test]
fn modern_platform_profiles_validate_kernel_advertised_values() {
    let root = TempDir::new().unwrap();
    put(root.path(), "firmware/acpi/platform_profile", "balanced");
    put(
        root.path(),
        "firmware/acpi/platform_profile_choices",
        "low-power balanced performance",
    );
    assert!(scan(root.path()).platform_profile.is_some());
    assert!(plan(
        root.path(),
        ControlRequest::PlatformProfile {
            profile: "performance".into()
        }
    )
    .is_ok());
    assert!(plan(
        root.path(),
        ControlRequest::PlatformProfile {
            profile: "turbo-999".into()
        }
    )
    .is_err());
}

#[test]
fn missing_firmware_profile_does_not_hide_cpu_information() {
    let root = TempDir::new().unwrap();
    put(
        root.path(),
        "devices/system/cpu/cpufreq/policy0/scaling_driver",
        "amd-pstate-epp",
    );
    put(
        root.path(),
        "devices/system/cpu/cpufreq/policy0/scaling_governor",
        "powersave",
    );
    let s = scan(root.path());
    assert!(s.platform_profile.is_none());
    assert_eq!(s.cpu_policies[0].driver.as_deref(), Some("amd-pstate-epp"));
    assert!(plan(
        root.path(),
        ControlRequest::PlatformProfile {
            profile: "performance".into()
        }
    )
    .is_err());
}

#[test]
fn thresholds_preserve_valid_intermediate_pairs_in_both_directions() {
    let root = TempDir::new().unwrap();
    battery(root.path(), "BAT0", "70", "80");
    let p = plan(root.path(), thresholds(85, 95)).unwrap();
    assert!(p.changes[0].attribute.ends_with("end_threshold"));
    battery(root.path(), "BAT0", "85", "95");
    let p = plan(root.path(), thresholds(40, 60)).unwrap();
    assert!(p.changes[0].attribute.ends_with("start_threshold"));
}

#[test]
fn invalid_thresholds_are_rejected_before_any_write() {
    let root = TempDir::new().unwrap();
    battery(root.path(), "BAT0", "0", "100");
    for (start, end) in [(90, 80), (80, 80), (101, 100), (0, 0)] {
        assert!(plan(root.path(), thresholds(start, end)).is_err());
    }
    assert_eq!(
        fs::read_to_string(
            root.path()
                .join("class/power_supply/BAT0/charge_control_end_threshold")
        )
        .unwrap(),
        "100"
    );
}

#[test]
fn preview_and_already_applied_requests_do_not_write() {
    let root = TempDir::new().unwrap();
    battery(root.path(), "BAT0", "75", "80");
    assert!(plan(root.path(), thresholds(75, 80))
        .unwrap()
        .changes
        .is_empty());
    plan(root.path(), thresholds(40, 60)).unwrap();
    assert_eq!(
        fs::read_to_string(
            root.path()
                .join("class/power_supply/BAT0/charge_control_start_threshold")
        )
        .unwrap(),
        "75"
    );
}

#[test]
fn brightness_is_bounded_and_success_is_read_back() {
    let root = TempDir::new().unwrap();
    put(
        root.path(),
        "class/leds/tpacpi::kbd_backlight/max_brightness",
        "2",
    );
    put(
        root.path(),
        "class/leds/tpacpi::kbd_backlight/brightness",
        "1\n",
    );
    let req = |value| ControlRequest::Brightness {
        device: "tpacpi::kbd_backlight".into(),
        value,
    };
    assert!(plan(root.path(), req(3)).is_err());
    let report = execute(root.path(), req(2)).unwrap();
    assert!(report.verified);
    assert_eq!(report.applied.len(), 1);
    assert_eq!(
        fs::read_to_string(
            root.path()
                .join("class/leds/tpacpi::kbd_backlight/brightness")
        )
        .unwrap(),
        "2\n"
    );
}

#[test]
fn arbitrary_paths_unknown_fields_and_fan_off_are_not_accepted() {
    let root = TempDir::new().unwrap();
    assert!(plan(
        root.path(),
        ControlRequest::Brightness {
            device: "../../etc/passwd".into(),
            value: 0
        }
    )
    .is_err());
    for json in [
        r#"{"kind":"fan_off"}"#,
        r#"{"kind":"fan_auto","path":"/etc/passwd"}"#,
        r#"{"kind":"battery_thresholds","battery":"BAT0","start":-1,"end":80}"#,
    ] {
        assert!(serde_json::from_str::<ControlRequest>(json).is_err());
    }
}

#[test]
fn control_symlink_cannot_escape_the_hardware_root() {
    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    put(outside.path(), "brightness", "1");
    put(root.path(), "class/backlight/panel/max_brightness", "2");
    symlink(
        outside.path().join("brightness"),
        root.path().join("class/backlight/panel/brightness"),
    )
    .unwrap();
    assert!(plan(
        root.path(),
        ControlRequest::Brightness {
            device: "panel".into(),
            value: 2
        }
    )
    .unwrap_err()
    .to_string()
    .contains("escapes"));
}

#[test]
fn snapshots_do_not_export_serial_numbers_or_uuids() {
    let root = TempDir::new().unwrap();
    put(root.path(), "class/dmi/id/product_serial", "PRIVATE-SERIAL");
    put(root.path(), "class/dmi/id/product_uuid", "PRIVATE-UUID");
    put(root.path(), "class/power_supply/BAT0/type", "Battery");
    put(
        root.path(),
        "class/power_supply/BAT0/serial_number",
        "PRIVATE-BATTERY",
    );
    let json = serde_json::to_string(&scan(root.path())).unwrap();
    assert!(!json.contains("PRIVATE"));
}

#[test]
fn unavailable_hardware_root_does_not_crash() {
    let root = TempDir::new().unwrap();
    let s = scan(&root.path().join("does-not-exist"));
    assert!(s.batteries.is_empty());
    assert!(!s.diagnostics.is_empty());
}

#[test]
fn cli_help_and_errors_are_usable() {
    let binary = env!("CARGO_BIN_EXE_thinkpad-dashboard");
    assert!(Command::new(binary)
        .arg("--help")
        .output()
        .unwrap()
        .status
        .success());
    assert!(!Command::new(binary)
        .arg("typo")
        .output()
        .unwrap()
        .status
        .success());
    assert!(!Command::new(binary)
        .args(["plan", "{bad json}"])
        .output()
        .unwrap()
        .status
        .success());
}

#[test]
fn cli_settings_save_does_not_apply_hardware_or_install_automation() {
    let root = TempDir::new().unwrap();
    let binary = env!("CARGO_BIN_EXE_thinkpad-dashboard");
    let desired = r#"{"schema_version":1,"power_profile":"balanced","battery_thresholds":{"BAT0":{"start":75,"end":80}}}"#;
    let run = |args: &[&str]| {
        Command::new(binary)
            .env("XDG_CONFIG_HOME", root.path())
            .args(args)
            .output()
            .unwrap()
    };
    let saved = run(&["settings", "save", desired]);
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&saved.stdout).unwrap();
    assert_eq!(receipt["hardware_changed"], false);
    assert_eq!(receipt["auto_apply"], false);
    let loaded = run(&["settings", "show"]);
    let actual: serde_json::Value = serde_json::from_slice(&loaded.stdout).unwrap();
    assert_eq!(actual["preferences"]["power_profile"], "balanced");
    assert!(!root.path().join("systemd").exists());
    assert!(!root.path().join("autostart").exists());
    let before = fs::read(root.path().join("thinkpad-dashboard/preferences.json")).unwrap();
    let invalid = run(&["settings", "save", "{bad json}"]);
    assert!(!invalid.status.success());
    assert_eq!(
        fs::read(root.path().join("thinkpad-dashboard/preferences.json")).unwrap(),
        before
    );
}

#[test]
fn invalid_monitor_count_is_rejected_without_starting_a_loop() {
    let binary = env!("CARGO_BIN_EXE_thinkpad-dashboard");
    for count in ["0", "61", "abc"] {
        assert!(!Command::new(binary)
            .args(["monitor", count])
            .output()
            .unwrap()
            .status
            .success());
    }
}

#[test]
fn newer_battery_health_is_kept_separate_and_zero_cycles_are_unknown() {
    let root = TempDir::new().unwrap();
    battery(root.path(), "BAT0", "0", "100");
    for (name, value) in [
        ("cycle_count", "0"),
        ("state_of_health", "92"),
        ("health", "Good"),
        ("technology", "Li-ion"),
        ("charge_behaviour", "[auto] inhibit-charge force-discharge"),
    ] {
        put(
            root.path(),
            &format!("class/power_supply/BAT0/{name}"),
            value,
        );
    }
    let snapshot = scan(root.path());
    let battery = &snapshot.batteries[0];
    assert_eq!(battery.cycles, None);
    assert_eq!(battery.reported_health_percent, Some(92));
    assert_eq!(battery.health_percent, None);
    assert_eq!(battery.health_status.as_deref(), Some("Good"));
    assert_eq!(battery.charge_modes.as_ref().unwrap().active, "auto");
}

#[test]
fn programmable_power_supply_is_online_and_nan_power_is_unknown() {
    let root = TempDir::new().unwrap();
    put(root.path(), "class/power_supply/USB/type", "USB");
    put(root.path(), "class/power_supply/USB/online", "2");
    battery(root.path(), "BAT0", "0", "100");
    put(root.path(), "class/power_supply/BAT0/power_now", "NaN");
    let snapshot = scan(root.path());
    assert_eq!(snapshot.power_sources[0].online, Some(true));
    assert!(snapshot.batteries[0].power_w.is_none());
}

#[test]
fn trackpoint_requests_cannot_select_arbitrary_attributes_or_out_of_range_values() {
    for data in [
        r#"{"kind":"track_point","controller":"serio2","setting":"drift_time","value":1}"#,
        r#"{"kind":"track_point","controller":"serio2","setting":"speed","value":256}"#,
        r#"{"kind":"track_point","controller":"serio2","setting":"speed","value":10,"path":"/etc/passwd"}"#,
    ] {
        assert!(serde_json::from_str::<ControlRequest>(data).is_err());
    }
    let root = TempDir::new().unwrap();
    let request: ControlRequest = serde_json::from_str(
        r#"{"kind":"track_point","controller":"serio2","setting":"sensitivity","value":0}"#,
    )
    .unwrap();
    assert!(plan(root.path(), request)
        .unwrap_err()
        .to_string()
        .contains("1..255"));
}

#[test]
fn helper_version_can_be_read_without_administrator_authorization() {
    let output = Command::new(env!("CARGO_BIN_EXE_thinkpad-dashboard-helper"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        concat!("thinkpad-dashboard-helper ", env!("CARGO_PKG_VERSION"))
    );
}
