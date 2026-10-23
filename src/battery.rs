use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Choices {
    pub active: String,
    pub available: Vec<String>,
}
pub fn parse_choices(raw: &str) -> Result<Choices> {
    let mut active = None;
    let mut available = vec![];
    for token in raw.split_whitespace() {
        let selected = token.starts_with('[') && token.ends_with(']');
        let value = if selected {
            &token[1..token.len() - 1]
        } else {
            token
        };
        ensure!(
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "Malformed charging mode list"
        );
        ensure!(
            !available.iter().any(|v| v == value),
            "Duplicate charging mode"
        );
        if selected {
            ensure!(active.is_none(), "Multiple active charging modes");
            active = Some(value.to_string());
        }
        available.push(value.to_string());
    }
    Ok(Choices {
        active: active.context("Charging mode list has no selected value")?,
        available,
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ChargeMode {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "inhibit-charge")]
    InhibitCharge,
}
impl ChargeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::InhibitCharge => "inhibit-charge",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub title: &'static str,
    pub start: u8,
    pub end: u8,
    pub description: &'static str,
}
pub fn presets() -> Vec<Preset> {
    vec![
        Preset {
            id: "desk",
            title: "Mostly plugged in",
            start: 50,
            end: 60,
            description: "Example lower-charge preset for desk use; reduces unplugged runtime",
        },
        Preset {
            id: "everyday",
            title: "Everyday",
            start: 75,
            end: 80,
            description: "Example compromise between charge limit and unplugged runtime",
        },
        Preset {
            id: "full",
            title: "Full charge",
            start: 0,
            end: 100,
            description: "Allow charging to full using this driver's threshold semantics",
        },
    ]
}
pub fn preset_request(id: &str, battery: &str) -> Result<crate::ControlRequest> {
    let selected = presets()
        .into_iter()
        .find(|p| p.id == id)
        .context("Unknown charging preset")?;
    Ok(crate::ControlRequest::BatteryThresholds {
        battery: battery.into(),
        start: selected.start,
        end: selected.end,
    })
}

#[derive(Debug, Default, Serialize)]
pub struct Overview {
    pub battery_count: usize,
    pub energy_now_wh: Option<f64>,
    pub energy_full_wh: Option<f64>,
    pub percent: Option<f64>,
    pub estimated_discharge_hours: Option<f64>,
}
pub fn overview(batteries: &[crate::probe::Battery]) -> Overview {
    let present: Vec<_> = batteries
        .iter()
        .filter(|b| b.present != Some(false) && b.scope.as_deref() != Some("Device"))
        .collect();
    if present.is_empty() {
        return Overview::default();
    }
    let total = |values: Vec<Option<f64>>| {
        values
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .map(|v| v.iter().sum::<f64>())
    };
    let energy = total(present.iter().map(|b| b.energy_now_wh).collect());
    let full = total(present.iter().map(|b| b.energy_full_wh).collect()).filter(|v| *v > 0.0);
    let power = total(present.iter().map(|b| b.power_w).collect()).filter(|v| *v > 0.0);
    let discharge_only = present
        .iter()
        .any(|b| b.status.as_deref() == Some("Discharging"))
        && present.iter().all(|b| {
            matches!(
                b.status.as_deref(),
                Some("Discharging" | "Not charging" | "Full")
            )
        });
    Overview {
        battery_count: present.len(),
        energy_now_wh: energy,
        energy_full_wh: full,
        percent: energy.zip(full).map(|(now, full)| now / full * 100.0),
        estimated_discharge_hours: if discharge_only {
            energy.zip(power).map(|(energy, power)| energy / power)
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_is_normalized_and_ambiguous_lists_rejected() {
        let parsed = parse_choices("auto [inhibit-charge] force-discharge").unwrap();
        assert_eq!(parsed.active, "inhibit-charge");
        assert_eq!(parsed.available.len(), 3);
        for value in [
            "auto inhibit-charge",
            "[auto] [inhibit-charge]",
            "[auto] auto",
            "[]",
            "[auto",
            "[auto] ../x",
        ] {
            assert!(parse_choices(value).is_err(), "{value}");
        }
    }
    #[test]
    fn forced_discharge_is_not_an_exposed_control() {
        assert!(serde_json::from_str::<ChargeMode>("\"force-discharge\"").is_err());
    }
    #[test]
    fn aggregate_charge_is_weighted_by_energy_not_battery_count() {
        let batteries = vec![
            crate::probe::Battery {
                energy_now_wh: Some(10.0),
                energy_full_wh: Some(10.0),
                ..Default::default()
            },
            crate::probe::Battery {
                energy_now_wh: Some(0.0),
                energy_full_wh: Some(30.0),
                ..Default::default()
            },
        ];
        assert_eq!(overview(&batteries).percent, Some(25.0));
    }
    #[test]
    fn missing_energy_or_charging_does_not_invent_a_runtime() {
        let mut battery = crate::probe::Battery {
            energy_now_wh: Some(20.0),
            energy_full_wh: Some(40.0),
            power_w: Some(10.0),
            status: Some("Discharging".into()),
            ..Default::default()
        };
        assert_eq!(
            overview(&[battery.clone()]).estimated_discharge_hours,
            Some(2.0)
        );
        battery.status = Some("Charging".into());
        assert!(overview(&[battery.clone()])
            .estimated_discharge_hours
            .is_none());
        battery.energy_now_wh = None;
        assert!(overview(&[battery]).percent.is_none());
    }

    #[test]
    fn peripheral_batteries_are_not_counted_as_laptop_batteries() {
        let battery = crate::probe::Battery {
            scope: Some("Device".into()),
            percent: Some(90.0),
            ..Default::default()
        };
        assert_eq!(overview(&[battery]).battery_count, 0);
    }
}
