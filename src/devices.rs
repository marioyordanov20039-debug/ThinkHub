//! Read-only discovery without EDID or input-event access. Unique ID fields are not returned.
use crate::probe::{directories, id, number, read};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Devices {
    pub displays: Vec<Display>,
    pub inputs: Vec<Input>,
    pub trackpoints: Vec<TrackPoint>,
}
#[derive(Debug, Serialize)]
pub struct Display {
    pub connector: String,
    pub connected: Option<bool>,
    pub enabled: Option<bool>,
    pub supported_resolutions: Option<Vec<String>>,
}
#[derive(Debug, Serialize)]
pub struct Input {
    pub event: String,
    pub name: Option<String>,
    pub kind: String,
    pub classification_source: String,
}
#[derive(Debug, Serialize)]
pub struct TrackPoint {
    pub controller: String,
    pub name: String,
    pub sensitivity: Option<u64>,
    pub speed: Option<u64>,
    pub control_status: &'static str,
}

fn bit(value: &str, index: u32) -> bool {
    // The properties/EV flags used here fit in the lowest 32 bits on both kernel word sizes.
    value
        .split_whitespace()
        .last()
        .and_then(|v| u64::from_str_radix(v, 16).ok())
        .is_some_and(|v| v & (1 << index) != 0)
}
fn flag(data: &str, name: &str) -> bool {
    data.lines().any(|line| line == format!("E:{name}=1"))
}
fn event_id(name: &str) -> bool {
    name.strip_prefix("event")
        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|v| v.is_ascii_digit()))
}

pub fn scan(sys: &Path, udev: &Path) -> Devices {
    let mut displays = vec![];
    for p in directories(sys.join("class/drm")) {
        if !p.join("status").is_file() {
            continue;
        }
        let resolutions = std::fs::read_to_string(p.join("modes")).ok().map(|value| {
            let mut values = vec![];
            for mode in value.lines() {
                let mode = mode.trim();
                if let Some((width, height)) = mode.split_once('x') {
                    if width
                        .parse::<u32>()
                        .ok()
                        .is_some_and(|v| v > 0 && v <= 65536)
                        && height
                            .parse::<u32>()
                            .ok()
                            .is_some_and(|v| v > 0 && v <= 65536)
                        && !values.iter().any(|v| v == mode)
                    {
                        values.push(mode.to_string());
                    }
                }
            }
            values
        });
        displays.push(Display {
            connector: id(&p),
            connected: match read(p.join("status")).as_deref() {
                Some("connected") => Some(true),
                Some("disconnected") => Some(false),
                _ => None,
            },
            enabled: match read(p.join("enabled")).as_deref() {
                Some("enabled") => Some(true),
                Some("disabled") => Some(false),
                _ => None,
            },
            supported_resolutions: resolutions,
        });
    }
    let mut inputs = vec![];
    for p in directories(sys.join("class/input")) {
        let event = id(&p);
        if !event_id(&event) {
            continue;
        }
        let dev = p.join("device");
        let name = read(dev.join("name"));
        let properties = read(dev.join("properties")).unwrap_or_default();
        let udev_data = read(p.join("dev"))
            .filter(|v| {
                v.split(':').count() == 2 && v.bytes().all(|b| b.is_ascii_digit() || b == b':')
            })
            .and_then(|v| read(udev.join(format!("c{v}"))))
            .unwrap_or_default();
        let from_udev = [
            ("ID_INPUT_POINTINGSTICK", "trackpoint"),
            ("ID_INPUT_TOUCHPAD", "touchpad"),
            ("ID_INPUT_TOUCHSCREEN", "touchscreen"),
            ("ID_INPUT_TABLET", "tablet"),
            ("ID_INPUT_KEYBOARD", "keyboard"),
            ("ID_INPUT_MOUSE", "mouse"),
        ]
        .into_iter()
        .find(|(key, _)| flag(&udev_data, key))
        .map(|(_, kind)| kind);
        let (kind, source) = if let Some(kind) = from_udev {
            (kind, "udev")
        } else if bit(&properties, 5) {
            ("trackpoint", "kernel-properties")
        } else if bit(&properties, 2) {
            ("touchpad", "kernel-properties")
        } else if name
            .as_ref()
            .is_some_and(|v| v.to_ascii_lowercase().contains("trackpoint"))
        {
            ("trackpoint", "name-hint")
        } else if read(dev.join("capabilities/key")).is_some_and(|v| bit(&v, 30)) {
            ("keyboard", "kernel-capabilities")
        } else {
            ("other", "unknown")
        };
        inputs.push(Input {
            event,
            name,
            kind: kind.into(),
            classification_source: source.into(),
        });
    }
    let mut trackpoints = vec![];
    for p in directories(sys.join("bus/serio/devices")) {
        if let Some(name) = verified_ps2_trackpoint(&p) {
            trackpoints.push(TrackPoint {
                controller: id(&p),
                name,
                sensitivity: number(p.join("sensitivity")).filter(|v| *v <= 255),
                speed: number(p.join("speed")).filter(|v| *v <= 255),
                control_status: "detected; kernel write acceptance needs verification",
            });
        }
    }
    Devices {
        displays,
        inputs,
        trackpoints,
    }
}

fn verified_ps2_trackpoint(path: &Path) -> Option<String> {
    if std::fs::read_link(path.join("driver"))
        .ok()
        .as_deref()
        .map(id)
        .as_deref()
        != Some("psmouse")
    {
        return None;
    }
    directories(path.join("input"))
        .into_iter()
        .find_map(|input| {
            let props = read(input.join("properties")).unwrap_or_default();
            let name = read(input.join("name"))?;
            // Driver properties establish identity independently of vendor product-name spelling.
            bit(&props, 5).then_some(name)
        })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackPointSetting {
    Sensitivity,
    Speed,
}
impl TrackPointSetting {
    pub fn attribute(self) -> &'static str {
        match self {
            Self::Sensitivity => "sensitivity",
            Self::Speed => "speed",
        }
    }
}

pub fn trackpoint_attribute(
    sys: &Path,
    controller: &str,
    setting: TrackPointSetting,
) -> Result<PathBuf> {
    let suffix = controller
        .strip_prefix("serio")
        .context("Expected a detected PS/2 controller ID")?;
    ensure!(
        !suffix.is_empty() && suffix.bytes().all(|v| v.is_ascii_digit()) && controller.len() < 64,
        "Invalid PS/2 controller ID"
    );
    let path = sys.join("bus/serio/devices").join(controller);
    ensure!(
        verified_ps2_trackpoint(&path).is_some(),
        "Controller is not a verified PS/2 TrackPoint; USB/I2C devices need their own adapter"
    );
    let attribute = path.join(setting.attribute());
    ensure!(
        number(&attribute).is_some_and(|v| v <= 255),
        "TrackPoint setting is unavailable or invalid"
    );
    Ok(attribute)
}

pub fn brightness_request(sys: &Path, device: &str, percent: u8) -> Result<crate::ControlRequest> {
    ensure!(
        percent <= 100
            && !device.is_empty()
            && device
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || b"_-".contains(&v)),
        "Invalid display device or percentage"
    );
    let max = number(
        sys.join("class/backlight")
            .join(device)
            .join("max_brightness"),
    )
    .filter(|v| *v > 0)
    .context("Display backlight is unavailable")?;
    let value = ((u128::from(max) * u128::from(percent) + 50) / 100) as u64;
    Ok(crate::ControlRequest::Brightness {
        device: device.into(),
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::symlink};
    fn put(root: &Path, path: &str, value: &str) {
        let p = root.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, value).unwrap();
    }
    #[test]
    fn displays_deduplicate_resolutions_without_inventing_refresh_or_current_mode() {
        let root = tempfile::tempdir().unwrap();
        put(root.path(), "class/drm/card3-eDP-1/status", "connected");
        put(
            root.path(),
            "class/drm/card3-eDP-1/modes",
            "1920x1080\n1920x1080\ninvalid\n0x0\n1366x768\n",
        );
        let data = scan(root.path(), root.path());
        assert_eq!(
            data.displays[0].supported_resolutions.as_ref().unwrap(),
            &["1920x1080", "1366x768"]
        );
        assert_eq!(data.displays[0].connected, Some(true));
        assert_eq!(data.displays[0].enabled, None);
    }
    #[test]
    fn event_aliases_are_not_counted_twice_and_pointing_stick_wins_over_mouse() {
        let root = tempfile::tempdir().unwrap();
        for event in ["event4", "mouse0"] {
            put(
                root.path(),
                &format!("class/input/{event}/device/name"),
                "TrackPoint",
            );
        }
        put(root.path(), "class/input/event4/dev", "13:68");
        put(
            root.path(),
            "udev/c13:68",
            "E:ID_INPUT_MOUSE=1\nE:ID_INPUT_POINTINGSTICK=1\nE:ID_SERIAL=SECRET",
        );
        let data = scan(root.path(), &root.path().join("udev"));
        assert_eq!(data.inputs.len(), 1);
        assert_eq!(data.inputs[0].kind, "trackpoint");
        assert!(!serde_json::to_string(&data).unwrap().contains("SECRET"));
    }
    #[test]
    fn trackpoint_controls_require_both_driver_and_pointing_stick_identity() {
        let root = tempfile::tempdir().unwrap();
        put(root.path(), "bus/serio/devices/serio7/sensitivity", "128");
        put(
            root.path(),
            "bus/serio/devices/serio7/input/input9/name",
            "PS/2 pointing stick",
        );
        put(
            root.path(),
            "bus/serio/devices/serio7/input/input9/properties",
            "21",
        );
        assert!(
            trackpoint_attribute(root.path(), "serio7", TrackPointSetting::Sensitivity).is_err()
        );
        fs::create_dir_all(root.path().join("drivers/psmouse")).unwrap();
        symlink(
            root.path().join("drivers/psmouse"),
            root.path().join("bus/serio/devices/serio7/driver"),
        )
        .unwrap();
        assert!(
            trackpoint_attribute(root.path(), "serio7", TrackPointSetting::Sensitivity).is_ok()
        );
        assert!(
            trackpoint_attribute(root.path(), "../serio7", TrackPointSetting::Sensitivity).is_err()
        );
        put(
            root.path(),
            "bus/serio/devices/serio7/input/input9/properties",
            "0",
        );
        assert!(
            trackpoint_attribute(root.path(), "serio7", TrackPointSetting::Sensitivity).is_err()
        );
    }
    #[test]
    fn brightness_percentage_is_rounded_and_validated() {
        let root = tempfile::tempdir().unwrap();
        put(
            root.path(),
            "class/backlight/intel_backlight/max_brightness",
            "1515",
        );
        match brightness_request(root.path(), "intel_backlight", 50).unwrap() {
            crate::ControlRequest::Brightness { value, .. } => assert_eq!(value, 758),
            _ => panic!("Wrong request"),
        }
        assert!(brightness_request(root.path(), "../other", 50).is_err());
        assert!(brightness_request(root.path(), "intel_backlight", 101).is_err());
    }
    #[test]
    fn hot_unplug_is_reflected_on_next_scan() {
        let root = tempfile::tempdir().unwrap();
        put(root.path(), "class/drm/card0-HDMI-A-1/status", "connected");
        assert_eq!(scan(root.path(), root.path()).displays.len(), 1);
        fs::remove_dir_all(root.path().join("class/drm/card0-HDMI-A-1")).unwrap();
        assert!(scan(root.path(), root.path()).displays.is_empty());
    }
}
