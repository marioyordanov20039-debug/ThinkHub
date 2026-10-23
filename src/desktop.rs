//! Session information and read-only compositor queries. Never changes desktop settings.
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::{path::Path, time::Duration};

#[derive(Debug, Serialize)]
pub struct Output {
    pub name: String,
    pub width: u64,
    pub height: u64,
    pub refresh_hz: f64,
    pub scale: f64,
    pub transform: Option<u64>,
    pub enabled: Option<bool>,
}
#[derive(Debug, Serialize)]
pub struct Session {
    pub desktop: Option<String>,
    pub session_type: Option<String>,
    pub adapter: String,
    pub outputs: Option<Vec<Output>>,
    pub pointer_names: Option<Vec<String>>,
    pub diagnostics: Vec<String>,
    pub desktop_controls: &'static str,
}
pub fn parse_outputs(data: Value) -> Result<Vec<Output>> {
    data.as_array()
        .context("Compositor outputs are not an array")?
        .iter()
        .map(|value| {
            let width = value["width"].as_u64().context("Output width missing")?;
            let height = value["height"].as_u64().context("Output height missing")?;
            let refresh_hz = value["refreshRate"]
                .as_f64()
                .context("Output refresh rate missing")?;
            let scale = value["scale"].as_f64().context("Output scale missing")?;
            ensure!(
                width <= 65536
                    && height <= 65536
                    && refresh_hz.is_finite()
                    && refresh_hz >= 0.0
                    && scale.is_finite()
                    && scale > 0.0,
                "Invalid compositor output metrics"
            );
            Ok(Output {
                name: value["name"]
                    .as_str()
                    .context("Output name missing")?
                    .into(),
                width,
                height,
                refresh_hz,
                scale,
                transform: value["transform"].as_u64(),
                enabled: value["disabled"].as_bool().map(|v| !v),
            })
        })
        .collect()
}
fn query(arguments: &[&str]) -> Result<Value> {
    let program = ["/usr/bin/hyprctl", "/bin/hyprctl"]
        .into_iter()
        .find(|p| Path::new(p).is_file())
        .context("Optional Hyprland client is not installed")?;
    let output = crate::process::run(
        Path::new(program),
        arguments,
        Duration::from_secs(3),
        1024 * 1024,
    )?;
    ensure!(
        output.status.success(),
        "Compositor query failed; session data is unavailable"
    );
    serde_json::from_slice(&output.stdout).context("Compositor returned invalid JSON")
}
pub fn scan(desktop: Option<String>, session_type: Option<String>) -> Session {
    let is_hyprland = desktop
        .as_ref()
        .is_some_and(|v| v.split(':').any(|v| v.eq_ignore_ascii_case("hyprland")));
    let mut session = Session { desktop, session_type, adapter: if is_hyprland { "hyprland-read-only" } else { "generic-kernel-discovery" }.into(), outputs: None, pointer_names: None, diagnostics: vec![], desktop_controls: "Not implemented; kernel controls are distinct from compositor acceleration, tap and scrolling settings" };
    if is_hyprland {
        match query(&["-j", "monitors", "all"]).and_then(parse_outputs) {
            Ok(outputs) => session.outputs = Some(outputs),
            Err(error) => session.diagnostics.push(format!("{error:#}")),
        }
        match query(&["-j", "devices"]).and_then(|value| {
            value["mice"]
                .as_array()
                .context("Compositor pointer list unavailable")?
                .iter()
                .map(|value| {
                    value["name"]
                        .as_str()
                        .map(str::to_string)
                        .context("Compositor pointer name missing")
                })
                .collect::<Result<Vec<_>>>()
        }) {
            Ok(names) => session.pointer_names = Some(names),
            Err(error) => session.diagnostics.push(format!("{error:#}")),
        }
    }
    session
}
pub fn current() -> Session {
    scan(
        std::env::var("XDG_CURRENT_DESKTOP").ok(),
        std::env::var("XDG_SESSION_TYPE").ok(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_desktop_does_not_query_or_apply_another_desktops_settings() {
        let result = scan(Some("GNOME".into()), Some("wayland".into()));
        assert_eq!(result.adapter, "generic-kernel-discovery");
        assert!(result.outputs.is_none());
    }
    #[test]
    fn output_metrics_are_allowlisted_and_invalid_scale_rejected() {
        let mut data = serde_json::json!([{"name":"eDP-1","width":1920,"height":1080,"refreshRate":60.031,"scale":1.05,"serial":"SECRET"}]);
        let outputs = parse_outputs(data.clone()).unwrap();
        assert_eq!(outputs[0].scale, 1.05);
        assert!(!serde_json::to_string(&outputs).unwrap().contains("SECRET"));
        data[0]["scale"] = Value::from(0);
        assert!(parse_outputs(data).is_err());
    }
}
