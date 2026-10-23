//! One-shot, root-owned helper. Authorization is provided by pkexec/polkit.
use anyhow::{ensure, Context, Result};
use std::{env, fs, path::Path};
use thinkpad_dashboard::{
    authorization::OperationLock, controllers, devices, execute, firmware, probe, ControlRequest,
};

fn keyboard_test(sys: &Path) -> Result<serde_json::Value> {
    keyboard_test_with(sys, |sys, request| execute(sys, request).map(|_| ()))
}

fn keyboard_test_with(
    sys: &Path,
    mut apply: impl FnMut(&Path, ControlRequest) -> Result<()>,
) -> Result<serde_json::Value> {
    let base = sys.join("class/leds/tpacpi::kbd_backlight");
    let before: u64 = fs::read_to_string(base.join("brightness"))?
        .trim()
        .parse()?;
    let maximum: u64 = fs::read_to_string(base.join("max_brightness"))?
        .trim()
        .parse()?;
    ensure!(
        maximum > 0 && before <= maximum,
        "No usable ThinkPad keyboard backlight"
    );
    let during = if before > 0 { before - 1 } else { 1 };
    let request = |value| ControlRequest::Brightness {
        device: "tpacpi::kbd_backlight".into(),
        value,
    };
    let test = apply(sys, request(during));
    // Restore even if the test write or readback failed. Do not hide restoration errors.
    let restored = apply(sys, request(before));
    restored.context("Keyboard test could not restore the original brightness; inspect the keyboard lighting setting")?;
    test.context("Keyboard test failed; original brightness was restored")?;
    let after: u64 = fs::read_to_string(base.join("brightness"))?
        .trim()
        .parse()?;
    ensure!(
        after == before,
        "Keyboard brightness changed after restoration"
    );
    Ok(
        serde_json::json!({"operation":"keyboard_backlight_self_test", "before":before, "tested":during, "after":after, "restored":true}),
    )
}

fn run() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.as_slice() == ["--version"] {
        println!("thinkpad-dashboard-helper {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    ensure!(
        (args.len() == 2 && args[0] == "--apply")
            || (args.len() == 1
                && ["--check", "--test-keyboard", "--test-trackpoint", "--bios-info"].contains(&args[0].as_str())),
        "Usage: helper --apply '<request JSON>' | --check | --test-keyboard | --test-trackpoint | --bios-info"
    );
    ensure!(
        args.iter().map(String::len).sum::<usize>() <= 4096,
        "Request too large"
    );
    let request: Option<ControlRequest> = if args[0] == "--apply" {
        Some(serde_json::from_str(&args[1]).context("Invalid control request")?)
    } else {
        None
    };
    // SAFETY: geteuid takes no pointers and cannot invalidate Rust memory.
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "Administrator authorization required. This helper does not elevate itself."
    );
    let sys = Path::new("/sys");
    let model = ["product_version", "product_name"]
        .iter()
        .filter_map(|v| fs::read_to_string(sys.join("class/dmi/id").join(v)).ok())
        .collect::<Vec<_>>()
        .join(" ");
    ensure!(
        model.to_lowercase().contains("thinkpad"),
        "ThinkPad identity could not be verified; refusing hardware changes"
    );
    let _lock = OperationLock::acquire()?;
    match request {
        Some(request) => {
            let blockers = controllers::blockers(&request, &controllers::discover());
            ensure!(blockers.is_empty(), "{}", blockers.join("\n"));
            println!("{}", serde_json::to_string_pretty(&execute(sys, request)?)?);
        }
        None if args[0] == "--check" => println!(
            "{}",
            serde_json::json!({"ready":true,"model":probe::scan(sys).system.model,"version":env!("CARGO_PKG_VERSION")})
        ),
        None if args[0] == "--bios-info" => println!(
            "{}",
            serde_json::to_string_pretty(&firmware::bios_info(sys))?
        ),
        None if args[0] == "--test-trackpoint" => {
            println!("{}", serde_json::to_string_pretty(&trackpoint_test(sys)?)?)
        }
        None => println!("{}", serde_json::to_string_pretty(&keyboard_test(sys)?)?),
    }
    Ok(())
}

fn trackpoint_test(sys: &Path) -> Result<serde_json::Value> {
    trackpoint_test_with(sys, |sys, request| execute(sys, request).map(|_| ()))
}
fn trackpoint_test_with(
    sys: &Path,
    mut apply: impl FnMut(&Path, ControlRequest) -> Result<()>,
) -> Result<serde_json::Value> {
    let hardware = devices::scan(sys, Path::new("/run/udev/data"));
    ensure!(
        hardware.trackpoints.len() == 1,
        "Expected exactly one verified PS/2 TrackPoint for the reversible test"
    );
    let device = &hardware.trackpoints[0];
    let before = device
        .sensitivity
        .filter(|v| *v > 0 && *v <= 255)
        .context("TrackPoint sensitivity unavailable or zero")? as u8;
    let during = if before == 255 { 254 } else { before + 1 };
    let request = |value| ControlRequest::TrackPoint {
        controller: device.controller.clone(),
        setting: devices::TrackPointSetting::Sensitivity,
        value,
    };
    let tested = apply(sys, request(during));
    let restored = apply(sys, request(before));
    restored.context(
        "TrackPoint test could not restore original sensitivity; inspect the input setting",
    )?;
    tested.context("TrackPoint test failed; original driver setting was restored")?;
    // Verify only kernel state; never capture input events.
    let result = devices::scan(sys, Path::new("/run/udev/data"));
    let current = result
        .trackpoints
        .iter()
        .find(|v| v.controller == device.controller)
        .and_then(|v| v.sensitivity);
    ensure!(
        current == Some(u64::from(before)),
        "TrackPoint setting changed after restoration"
    );
    Ok(
        serde_json::json!({"controller":device.controller,"before":before,"tested":during,"after":current,"restored":true,"verification":"kernel parameter readback only; no input events captured"}),
    )
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("class/leds/tpacpi::kbd_backlight");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("brightness"), "2\n").unwrap();
        fs::write(path.join("max_brightness"), "2\n").unwrap();
        root
    }
    #[test]
    fn keyboard_test_changes_and_restores_value() {
        let root = fixture();
        let result = keyboard_test(root.path()).unwrap();
        assert_eq!(result["before"], 2);
        assert_eq!(result["tested"], 1);
        assert_eq!(result["after"], 2);
        assert_eq!(result["restored"], true);
    }
    #[test]
    fn keyboard_test_restores_after_a_failed_test_readback() {
        let root = fixture();
        let mut count = 0;
        let result = keyboard_test_with(root.path(), |sys, request| {
            count += 1;
            execute(sys, request)?;
            if count == 1 {
                anyhow::bail!("simulated readback failure");
            }
            Ok(())
        });
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("original brightness was restored"));
        assert_eq!(count, 2);
        assert_eq!(
            fs::read_to_string(
                root.path()
                    .join("class/leds/tpacpi::kbd_backlight/brightness")
            )
            .unwrap(),
            "2\n"
        );
    }

    fn trackpoint_fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("bus/serio/devices/serio2");
        fs::create_dir_all(base.join("input/input7")).unwrap();
        fs::create_dir_all(root.path().join("drivers/psmouse")).unwrap();
        std::os::unix::fs::symlink(root.path().join("drivers/psmouse"), base.join("driver"))
            .unwrap();
        fs::write(base.join("input/input7/name"), "IBM TrackPoint").unwrap();
        fs::write(base.join("input/input7/properties"), "21").unwrap();
        fs::write(base.join("sensitivity"), "200\n").unwrap();
        root
    }
    #[test]
    fn trackpoint_test_restores_the_original_kernel_parameter() {
        let root = trackpoint_fixture();
        let result = trackpoint_test(root.path()).unwrap();
        assert_eq!(result["before"], 200);
        assert_eq!(result["tested"], 201);
        assert_eq!(result["after"], 200);
    }
    #[test]
    fn trackpoint_test_restores_after_a_partial_test_failure() {
        let root = trackpoint_fixture();
        let mut calls = 0;
        let result = trackpoint_test_with(root.path(), |sys, request| {
            calls += 1;
            execute(sys, request)?;
            if calls == 1 {
                anyhow::bail!("simulated driver error after changing cached value");
            }
            Ok(())
        });
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("original driver setting was restored"));
        assert_eq!(calls, 2);
        assert_eq!(
            fs::read_to_string(root.path().join("bus/serio/devices/serio2/sensitivity"))
                .unwrap()
                .trim(),
            "200"
        );
    }
}
