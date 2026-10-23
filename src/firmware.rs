//! Read-only firmware information. No refresh, download, update or BIOS write operations.
use crate::probe::read;
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::{path::Path, time::Duration};

const BIOS_FIELDS: &[&str] = &[
    "SecureBoot",
    "SecurityChip",
    "VirtualizationTechnology",
    "VTdFeature",
    "HyperThreadingTechnology",
    "FnCtrlKeySwap",
    "FnKeyAsPrimary",
    "AlwaysOnUSB",
    "WakeOnLAN",
    "WakeOnLANDock",
    "TouchPad",
    "TrackPoint",
    "IntegratedCameraAccess",
    "MicrophoneAccess",
    "ThunderboltSecurityLevel",
    "BootMode",
    "WindowsUEFIFirmwareUpdate",
];
#[derive(Debug, Serialize)]
pub struct BiosSetting {
    pub name: String,
    pub value: Option<String>,
    pub possible_values: Option<String>,
    pub readable: bool,
}
#[derive(Debug, Serialize)]
pub struct BiosInfo {
    pub interface_present: bool,
    pub settings: Vec<BiosSetting>,
    pub pending_reboot: Option<bool>,
    pub editing_supported: bool,
}
pub fn bios_info(sys: &Path) -> BiosInfo {
    let base = sys.join("class/firmware-attributes/thinklmi/attributes");
    let settings = BIOS_FIELDS
        .iter()
        .filter_map(|name| {
            let p = base.join(name);
            if !p.is_dir() {
                return None;
            }
            let value = read(p.join("current_value"));
            Some(BiosSetting {
                name: (*name).into(),
                readable: value.is_some(),
                value,
                possible_values: read(p.join("possible_values")),
            })
        })
        .collect();
    BiosInfo {
        interface_present: base.is_dir(),
        settings,
        pending_reboot: read(base.join("pending_reboot")).and_then(|v| match v.as_str() {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        }),
        editing_supported: false,
    }
}

#[derive(Debug, Serialize)]
pub struct Device {
    pub name: String,
    pub vendor: Option<String>,
    pub version: Option<String>,
    pub updatable: Option<bool>,
}
#[derive(Debug, Serialize)]
pub struct Update {
    pub component: String,
    pub installed_version: Option<String>,
    pub available_versions: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Scan {
    pub devices: Option<Vec<Device>>,
    pub cached_updates: Option<Vec<Update>>,
    pub errors: Vec<String>,
    pub metadata_freshness: &'static str,
    pub metadata_refreshed: bool,
    pub firmware_changed: bool,
}
fn devices(json: &Value) -> Result<&Vec<Value>> {
    json.get("Devices")
        .and_then(Value::as_array)
        .context("Firmware response has no Devices array")
}
fn text(json: &Value, key: &str) -> Option<String> {
    json.get(key).and_then(Value::as_str).map(str::to_string)
}
pub fn parse_devices(json: &Value) -> Result<Vec<Device>> {
    devices(json)?
        .iter()
        .map(|d| {
            Ok(Device {
                name: text(d, "Name").context("Firmware component name missing")?,
                vendor: text(d, "Vendor"),
                version: text(d, "Version"),
                updatable: d
                    .get("Flags")
                    .and_then(Value::as_array)
                    .map(|flags| flags.iter().any(|flag| flag.as_str() == Some("updatable"))),
            })
        })
        .collect()
}
pub fn parse_updates(json: &Value) -> Result<Vec<Update>> {
    devices(json)?
        .iter()
        .map(|d| {
            let releases = d
                .get("Releases")
                .and_then(Value::as_array)
                .context("Firmware release list missing")?;
            let available_versions = releases
                .iter()
                .map(|r| text(r, "Version").context("Firmware release version missing"))
                .collect::<Result<Vec<_>>>()?;
            Ok(Update {
                component: text(d, "Name").context("Firmware component name missing")?,
                installed_version: text(d, "Version"),
                available_versions,
            })
        })
        .collect()
}
pub fn client_available() -> bool {
    ["/usr/bin/fwupdmgr", "/bin/fwupdmgr"]
        .iter()
        .any(|p| Path::new(p).is_file())
}
fn query(operation: &str) -> Result<Value> {
    ensure!(
        ["get-devices", "get-updates"].contains(&operation),
        "Firmware operation is not read-only"
    );
    let program = ["/usr/bin/fwupdmgr", "/bin/fwupdmgr"]
        .into_iter()
        .find(|p| Path::new(p).is_file())
        .context("Optional fwupdmgr client is not installed")?;
    let output = crate::process::run(
        Path::new(program),
        &[
            operation,
            "--json",
            "--no-authenticate",
            "--no-metadata-check",
            "--no-remote-check",
            "--no-unreported-check",
        ],
        Duration::from_secs(30),
        4 * 1024 * 1024,
    )?;
    // fwupdmgr uses exit 2 for nothing-to-do. Only accept it with valid structured data.
    ensure!(
        output.status.success() || output.status.code() == Some(2),
        "Firmware information query failed (exit {:?}); no firmware changes were requested",
        output.status.code()
    );
    serde_json::from_slice(&output.stdout).context("Firmware client did not return valid JSON")
}
pub fn scan() -> Scan {
    let mut errors = vec![];
    let devices = match query("get-devices").and_then(|data| parse_devices(&data)) {
        Ok(devices) => Some(devices),
        Err(error) => {
            errors.push(format!("Device inventory: {error:#}"));
            None
        }
    };
    let updates = match query("get-updates").and_then(|data| parse_updates(&data)) {
        Ok(updates) => Some(updates),
        Err(error) => {
            errors.push(format!("Cached update information: {error:#}"));
            None
        }
    };
    Scan {
        devices,
        cached_updates: updates,
        errors,
        metadata_freshness: "unknown; local metadata only",
        metadata_refreshed: false,
        firmware_changed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firmware_output_is_allowlisted_and_unique_identifiers_are_dropped() {
        let data = serde_json::json!({"Devices":[{"Name":"System Firmware","Version":"1.0","Serial":"SECRET","DeviceId":"PRIVATE","Guid":["UUID"],"InstanceIds":["SENSITIVE"],"Flags":["updatable"]}]});
        let parsed = parse_devices(&data).unwrap();
        assert_eq!(parsed[0].updatable, Some(true));
        let output = serde_json::to_string(&parsed).unwrap();
        for private in ["SECRET", "PRIVATE", "UUID", "SENSITIVE"] {
            assert!(!output.contains(private));
        }
    }
    #[test]
    fn missing_update_data_is_not_reported_as_no_updates() {
        assert!(parse_updates(&serde_json::json!({})).is_err());
        assert!(parse_updates(&serde_json::json!({"Devices":[{"Name":"BIOS"}]})).is_err());
        assert!(parse_updates(&serde_json::json!({"Devices":[]}))
            .unwrap()
            .is_empty());
    }
    #[test]
    fn release_versions_are_preserved_without_guessing_order() {
        let data = serde_json::json!({"Devices":[{"Name":"BIOS","Version":"1.0","Releases":[{"Version":"1.10","Description":"IGNORED"},{"Version":"1.9"}]}]});
        assert_eq!(
            parse_updates(&data).unwrap()[0].available_versions,
            ["1.10", "1.9"]
        );
    }
    #[test]
    fn bios_information_does_not_enumerate_password_settings() {
        let root = tempfile::tempdir().unwrap();
        let base = root
            .path()
            .join("class/firmware-attributes/thinklmi/attributes");
        for name in ["SecureBoot", "SupervisorPassword"] {
            std::fs::create_dir_all(base.join(name)).unwrap();
            std::fs::write(
                base.join(name).join("current_value"),
                if name == "SecureBoot" {
                    "Enable"
                } else {
                    "SECRET"
                },
            )
            .unwrap();
        }
        let data = bios_info(root.path());
        assert_eq!(data.settings.len(), 1);
        assert!(!data.editing_supported);
        assert!(!serde_json::to_string(&data).unwrap().contains("SECRET"));
    }
}
