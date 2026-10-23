use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub timestamp_unix: u64,
    pub system: System,
    pub batteries: Vec<Battery>,
    pub battery_overview: crate::battery::Overview,
    pub power_sources: Vec<PowerSource>,
    pub sensors: Vec<Sensor>,
    pub brightness: Vec<Brightness>,
    pub radios: Vec<Radio>,
    pub cpu_policies: Vec<CpuPolicy>,
    pub platform_profile: Option<Profile>,
    pub capabilities: Vec<Capability>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct System {
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub machine_type: Option<String>,
    pub bios_version: Option<String>,
    pub bios_date: Option<String>,
    pub thinkpad_acpi_loaded: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Battery {
    pub id: String,
    pub scope: Option<String>,
    pub present: Option<bool>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub percent: Option<f64>,
    pub health_percent: Option<f64>,
    pub health_basis: Option<String>,
    pub cycles: Option<u64>,
    pub energy_now_wh: Option<f64>,
    pub energy_full_wh: Option<f64>,
    pub energy_design_wh: Option<f64>,
    pub charge_now_ah: Option<f64>,
    pub charge_full_ah: Option<f64>,
    pub charge_design_ah: Option<f64>,
    pub power_w: Option<f64>,
    pub voltage_v: Option<f64>,
    pub temperature_c: Option<f64>,
    pub charge_start_percent: Option<u64>,
    pub charge_end_percent: Option<u64>,
    pub charge_behaviour: Option<String>,
    pub charge_modes: Option<crate::battery::Choices>,
    pub health_status: Option<String>,
    pub technology: Option<String>,
    pub reported_health_percent: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct PowerSource {
    pub id: String,
    pub kind: String,
    pub online: Option<bool>,
}
#[derive(Debug, Serialize)]
pub struct Sensor {
    pub id: String,
    pub driver: String,
    pub label: String,
    pub unit: String,
    pub value: f64,
    pub critical: Option<f64>,
}
#[derive(Debug, Serialize)]
pub struct Brightness {
    pub id: String,
    pub kind: String,
    pub current: Option<u64>,
    pub maximum: u64,
}
#[derive(Debug, Serialize)]
pub struct Radio {
    pub id: String,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub software_blocked: Option<bool>,
    pub hardware_blocked: Option<bool>,
}
#[derive(Debug, Serialize)]
pub struct CpuPolicy {
    pub id: String,
    pub driver: Option<String>,
    pub governor: Option<String>,
    pub governors: Vec<String>,
    pub energy_preference: Option<String>,
    pub energy_preferences: Vec<String>,
    pub minimum_khz: Option<u64>,
    pub maximum_khz: Option<u64>,
}
#[derive(Debug, Serialize)]
pub struct Profile {
    pub current: String,
    pub choices: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Capability {
    pub id: String,
    /// "detected" means a kernel interface exists, not that firmware accepted a write.
    pub status: String,
    pub reason: String,
    pub choices: Vec<String>,
    pub authorization: String,
}

pub(crate) fn read(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}
pub(crate) fn number(path: impl AsRef<Path>) -> Option<u64> {
    read(path)?.parse().ok()
}
fn signed(path: impl AsRef<Path>) -> Option<f64> {
    read(path)?.parse::<f64>().ok().filter(|v| v.is_finite())
}
pub(crate) fn directories(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<_> = fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|v| v.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}
pub(crate) fn id(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
pub(crate) fn words(path: impl AsRef<Path>) -> Vec<String> {
    read(path)
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}
fn ratio(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) if b > 0.0 => Some(a / b * 100.0),
        _ => None,
    }
}
fn micro(path: impl AsRef<Path>) -> Option<f64> {
    number(path).map(|v| v as f64 / 1_000_000.0)
}
fn capability(id: String, detected: bool, choices: Vec<String>, reason: &str) -> Capability {
    Capability {
        id,
        status: if detected { "detected" } else { "unavailable" }.into(),
        reason: reason.into(),
        choices,
        authorization: if detected {
            "May require administrator authorization; kernel can reject unsupported operations"
        } else {
            "none"
        }
        .into(),
    }
}

/// `sys` is `/sys` in production. A supplied fixture root makes tests hardware independent.
/// This probe never writes, reads serial numbers/UUIDs, or runs external commands.
pub fn scan(sys: &Path) -> Snapshot {
    let dmi = sys.join("class/dmi/id");
    let mut out = Snapshot {
        schema_version: 1,
        timestamp_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        system: System {
            vendor: read(dmi.join("sys_vendor")),
            model: read(dmi.join("product_version")),
            machine_type: read(dmi.join("product_name")),
            bios_version: read(dmi.join("bios_version")),
            bios_date: read(dmi.join("bios_date")),
            thinkpad_acpi_loaded: sys.join("module/thinkpad_acpi").exists(),
        },
        batteries: vec![],
        battery_overview: crate::battery::Overview::default(),
        power_sources: vec![],
        sensors: vec![],
        brightness: vec![],
        radios: vec![],
        cpu_policies: vec![],
        platform_profile: None,
        capabilities: vec![],
        diagnostics: vec![],
    };
    if !sys.join("class").is_dir() {
        out.diagnostics
            .push("Hardware filesystem unavailable: no class directory".into());
    }
    if out.system.model.is_none() {
        out.diagnostics
            .push("Model identity unavailable; compatibility is unknown".into());
    }
    for p in directories(sys.join("class/power_supply")) {
        let Some(kind) = read(p.join("type")) else {
            continue;
        };
        if kind != "Battery" {
            out.power_sources.push(PowerSource {
                id: id(&p),
                kind,
                online: number(p.join("online")).map(|v| v > 0),
            });
            continue;
        }
        let present = number(p.join("present")).map(|v| v == 1);
        // Slots without a battery must not expose stale charge controls.
        let energy_full = micro(p.join("energy_full"));
        let energy_design = micro(p.join("energy_full_design"));
        let charge_full = micro(p.join("charge_full"));
        let charge_design = micro(p.join("charge_full_design"));
        let energy_health = ratio(energy_full, energy_design);
        let charge_health = ratio(charge_full, charge_design);
        let absent = present == Some(false);
        let thresholds = !absent
            && number(p.join("charge_control_start_threshold")).is_some()
            && number(p.join("charge_control_end_threshold")).is_some();
        out.capabilities.push(capability(format!("battery-thresholds:{}", id(&p)), thresholds, vec![],
            if thresholds { "Charge threshold attributes detected; firmware restrictions are checked when applying" }
            else { "Battery absent, unreadable, or kernel does not expose both threshold attributes" }));
        if absent {
            out.batteries.push(Battery {
                id: id(&p),
                present,
                ..Battery::default()
            });
            continue;
        }
        let voltage = micro(p.join("voltage_now"));
        let charge_modes = read(p.join("charge_behaviour"))
            .and_then(|raw| crate::battery::parse_choices(&raw).ok());
        let choices = charge_modes
            .as_ref()
            .map(|v| {
                v.available
                    .iter()
                    .filter(|v| ["auto", "inhibit-charge"].contains(&v.as_str()))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        out.capabilities.push(capability(
            format!("battery-charge-mode:{}", id(&p)),
            !choices.is_empty(),
            choices,
            "Charging modes require a valid kernel-advertised list; force-discharge is not exposed",
        ));
        out.batteries.push(Battery {
            id: id(&p),
            scope: read(p.join("scope")),
            present,
            manufacturer: read(p.join("manufacturer")),
            model: read(p.join("model_name")),
            status: if absent { None } else { read(p.join("status")) },
            percent: if absent {
                None
            } else {
                number(p.join("capacity"))
                    .filter(|v| *v <= 100)
                    .map(|v| v as f64)
                    .or_else(|| ratio(micro(p.join("energy_now")), energy_full))
                    .or_else(|| ratio(micro(p.join("charge_now")), charge_full))
            },
            health_percent: if absent {
                None
            } else {
                energy_health.or(charge_health)
            },
            health_basis: if absent {
                None
            } else if energy_health.is_some() {
                Some("energy".into())
            } else if charge_health.is_some() {
                Some("charge".into())
            } else {
                None
            },
            cycles: number(p.join("cycle_count")).filter(|v| *v > 0),
            energy_now_wh: micro(p.join("energy_now")),
            energy_full_wh: energy_full,
            energy_design_wh: energy_design,
            charge_now_ah: micro(p.join("charge_now")),
            charge_full_ah: charge_full,
            charge_design_ah: charge_design,
            power_w: signed(p.join("power_now"))
                .map(|v| v.abs() / 1_000_000.0)
                .or_else(|| Some(signed(p.join("current_now"))?.abs() / 1_000_000.0 * voltage?)),
            voltage_v: voltage,
            temperature_c: signed(p.join("temp")).map(|v| v / 10.0),
            charge_start_percent: number(p.join("charge_control_start_threshold")),
            charge_end_percent: number(p.join("charge_control_end_threshold")),
            charge_behaviour: read(p.join("charge_behaviour")),
            charge_modes,
            health_status: read(p.join("health")),
            technology: read(p.join("technology")),
            reported_health_percent: number(p.join("state_of_health")).filter(|v| *v <= 100),
        });
    }
    out.battery_overview = crate::battery::overview(&out.batteries);
    for p in directories(sys.join("class/hwmon")) {
        let driver = read(p.join("name")).unwrap_or_else(|| id(&p));
        let mut files: Vec<_> = fs::read_dir(&p)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|v| v.path())
            .collect();
        files.sort();
        for file in files {
            let name = id(&file);
            let Some(stem) = name.strip_suffix("_input") else {
                continue;
            };
            let (unit, divisor) = if stem.starts_with("temp") {
                ("°C", 1000.0)
            } else if stem.starts_with("fan") {
                ("RPM", 1.0)
            } else {
                continue;
            };
            let Some(raw) = signed(&file) else {
                out.diagnostics
                    .push(format!("Cannot read sensor {}/{}", id(&p), name));
                continue;
            };
            let value = raw / divisor;
            if !value.is_finite()
                || (unit == "°C" && !(-40.0..=150.0).contains(&value))
                || (unit == "RPM" && value < 0.0)
            {
                out.diagnostics.push(format!(
                    "Invalid sensor reading ignored: {}/{}",
                    id(&p),
                    name
                ));
                continue;
            }
            out.sensors.push(Sensor {
                id: format!("{}/{}", id(&p), stem),
                driver: driver.clone(),
                label: read(p.join(format!("{stem}_label"))).unwrap_or_else(|| stem.into()),
                unit: unit.into(),
                value,
                critical: signed(p.join(format!("{stem}_crit"))).map(|v| v / divisor),
            });
        }
    }
    for (class, kind) in [("backlight", "display"), ("leds", "keyboard")] {
        for p in directories(sys.join(format!("class/{class}"))) {
            let name = id(&p);
            if class == "leds" && !name.ends_with("::kbd_backlight") && name != "tpacpi::thinklight"
            {
                continue;
            }
            if let Some(maximum) = number(p.join("max_brightness")).filter(|v| *v > 0) {
                out.brightness.push(Brightness {
                    id: name.clone(),
                    kind: kind.into(),
                    current: number(p.join("brightness")),
                    maximum,
                });
                out.capabilities.push(capability(
                    format!("brightness:{name}"),
                    true,
                    vec![format!("0..{maximum}")],
                    "Kernel brightness interface detected",
                ));
            }
        }
    }
    let profile = sys.join("firmware/acpi/platform_profile");
    let choices = words(sys.join("firmware/acpi/platform_profile_choices"));
    if let Some(current) = read(&profile) {
        out.platform_profile = Some(Profile {
            current,
            choices: choices.clone(),
        });
    }
    out.capabilities.push(capability(
        "platform-profile".into(),
        out.platform_profile.is_some() && !choices.is_empty(),
        choices,
        if out.platform_profile.is_some() {
            "Firmware platform profile interface detected"
        } else {
            "Not exposed by this model, firmware, or kernel; CPU governors are a separate feature"
        },
    ));
    for p in directories(sys.join("devices/system/cpu/cpufreq")) {
        if !id(&p).starts_with("policy") {
            continue;
        }
        out.cpu_policies.push(CpuPolicy {
            id: id(&p),
            driver: read(p.join("scaling_driver")),
            governor: read(p.join("scaling_governor")),
            governors: words(p.join("scaling_available_governors")),
            energy_preference: read(p.join("energy_performance_preference")),
            energy_preferences: words(p.join("energy_performance_available_preferences")),
            minimum_khz: number(p.join("scaling_min_freq")),
            maximum_khz: number(p.join("scaling_max_freq")),
        });
    }
    for p in directories(sys.join("class/rfkill")) {
        out.radios.push(Radio {
            id: id(&p),
            name: read(p.join("name")),
            kind: read(p.join("type")),
            software_blocked: number(p.join("soft")).map(|v| v == 1),
            hardware_blocked: number(p.join("hard")).map(|v| v == 1),
        });
    }
    out.capabilities.push(capability("manual-fan-control".into(), false, vec![], "Deferred until watchdog, thermal failsafe, and model-specific behavior are implemented and tested; monitoring is available"));
    let fan_auto = directories(sys.join("class/hwmon")).iter().any(|p| {
        read(p.join("name")).as_deref() == Some("thinkpad")
            && number(p.join("pwm1_enable")).is_some()
    });
    out.capabilities.push(capability(
        "fan-auto".into(),
        fan_auto,
        vec!["firmware-auto".into()],
        "Restore firmware cooling control when accepted by the driver",
    ));
    out
}

impl Snapshot {
    pub fn summary(&self) -> String {
        let mut text = format!(
            "ThinkPad Dashboard — backend {}\n{} ({})\nBIOS: {}\n",
            env!("CARGO_PKG_VERSION"),
            self.system.model.as_deref().unwrap_or("Unknown model"),
            self.system
                .machine_type
                .as_deref()
                .unwrap_or("unknown type"),
            self.system.bios_version.as_deref().unwrap_or("unavailable")
        );
        for b in &self.batteries {
            let metric = |v: Option<f64>| {
                v.map(|v| format!("{v:.1}%"))
                    .unwrap_or_else(|| "unavailable".into())
            };
            text.push_str(&format!(
                "{}: {} · charge {} · health {} · thresholds {}–{}\n",
                b.id,
                b.status.as_deref().unwrap_or("unavailable"),
                metric(b.percent),
                metric(b.health_percent),
                b.charge_start_percent
                    .map(|v| format!("{v}%"))
                    .unwrap_or_else(|| "unavailable".into()),
                b.charge_end_percent
                    .map(|v| format!("{v}%"))
                    .unwrap_or_else(|| "unavailable".into())
            ));
        }
        let mut sensors: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for s in &self.sensors {
            sensors
                .entry(&s.driver)
                .or_default()
                .push(format!("{} {:.1} {}", s.label, s.value, s.unit));
        }
        for (driver, values) in sensors {
            text.push_str(&format!("{driver}: {}\n", values.join(", ")));
        }
        text.push_str("\nControls (detected interfaces still need firmware write verification):\n");
        for c in &self.capabilities {
            text.push_str(&format!("  {}: {} — {}\n", c.id, c.status, c.reason));
        }
        for d in &self.diagnostics {
            text.push_str(&format!("Note: {d}\n"));
        }
        text
    }
}
