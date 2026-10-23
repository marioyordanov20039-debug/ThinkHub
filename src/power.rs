//! Unprivileged power-profiles-daemon adapter. No direct sysfs writes or root helper.
use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
pub struct Endpoint {
    pub name: &'static str,
    pub path: &'static str,
}
const ENDPOINTS: [Endpoint; 2] = [
    Endpoint {
        name: "org.freedesktop.UPower.PowerProfiles",
        path: "/org/freedesktop/UPower/PowerProfiles",
    },
    Endpoint {
        name: "net.hadess.PowerProfiles",
        path: "/net/hadess/PowerProfiles",
    },
];
pub struct Call<'a> {
    pub destination: &'a str,
    pub path: &'a str,
    pub interface: &'a str,
    pub method: &'a str,
    pub signature: &'a str,
    pub arguments: Vec<&'a str>,
    pub interactive: bool,
}
pub trait Transport {
    fn call(&mut self, call: Call<'_>) -> Result<Value>;
}
pub struct Busctl;
impl Transport for Busctl {
    fn call(&mut self, call: Call<'_>) -> Result<Value> {
        let program = ["/usr/bin/busctl", "/bin/busctl"]
            .into_iter()
            .find(|p| Path::new(p).is_file())
            .context("Optional busctl transport is not installed; hardware scanning still works")?;
        let timeout = if call.interactive { 45 } else { 3 };
        let mut child = Command::new(program)
            .args(["--system", "--auto-start=no", "--json=short"])
            .arg(format!("--timeout={timeout}"))
            .arg(format!(
                "--allow-interactive-authorization={}",
                if call.interactive { "yes" } else { "no" }
            ))
            .args([
                "call",
                call.destination,
                call.path,
                call.interface,
                call.method,
                call.signature,
            ])
            .args(&call.arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Cannot start D-Bus transport")?;
        // Drain both pipes concurrently; cap captured responses and bound the process lifetime.
        let stdout = child.stdout.take().context("Missing transport stdout")?;
        let stderr = child.stderr.take().context("Missing transport stderr")?;
        let capture = |pipe: Box<dyn Read + Send>| {
            thread::spawn(move || {
                let mut bytes = vec![];
                let result = pipe.take(65537).read_to_end(&mut bytes);
                (result, bytes)
            })
        };
        let out = capture(Box::new(stdout));
        let err = capture(Box::new(stderr));
        let deadline = Instant::now() + Duration::from_secs(timeout + 1);
        let status = loop {
            match child.try_wait()? {
                Some(status) => break Some(status),
                None if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                None => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let (out_result, output) = out
            .join()
            .map_err(|_| anyhow::anyhow!("D-Bus output reader failed"))?;
        let (err_result, error) = err
            .join()
            .map_err(|_| anyhow::anyhow!("D-Bus error reader failed"))?;
        out_result?;
        err_result?;
        let status = status.context("D-Bus operation timed out; a write may have completed, so read the current profile before retrying")?;
        ensure!(
            output.len() <= 65536 && error.len() <= 65536,
            "D-Bus response exceeded size limit"
        );
        ensure!(
            status.success(),
            "Power service rejected the operation: {}",
            String::from_utf8_lossy(&error).trim()
        );
        if output.is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_slice(&output).context("Malformed busctl response")
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub id: String,
    pub cpu_driver: Option<String>,
    pub platform_driver: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Hold {
    pub profile: String,
    pub application: Option<String>,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct State {
    pub api: String,
    pub service_owner: String,
    pub active: String,
    pub profiles: Vec<Profile>,
    pub holds: Option<Vec<Hold>>,
    pub degraded: Option<String>,
    pub inhibited: Option<String>,
    pub version: Option<String>,
}

fn variant<'a>(object: &'a Value, key: &str, signature: &str) -> Option<&'a Value> {
    let value = object.get(key)?;
    (value.get("type")?.as_str()? == signature)
        .then(|| value.get("data"))
        .flatten()
}
fn string(object: &Value, key: &str) -> Option<String> {
    variant(object, key, "s")?.as_str().map(str::to_owned)
}

pub fn parse_state(response: Value, api: &str, owner: &str) -> Result<State> {
    ensure!(
        response["type"] == "a{sv}",
        "Unexpected power service property signature"
    );
    let object = response["data"]
        .as_array()
        .filter(|v| v.len() == 1)
        .and_then(|v| v.first())
        .filter(|v| v.is_object())
        .context("Missing power service property dictionary")?;
    let active =
        string(object, "ActiveProfile").context("Active profile is missing or malformed")?;
    let entries = variant(object, "Profiles", "aa{sv}")
        .and_then(Value::as_array)
        .context("Available profiles are missing or malformed")?;
    let mut profiles = vec![];
    for entry in entries {
        let id = string(entry, "Profile").context("Profile identifier is missing")?;
        ensure!(
            !id.is_empty()
                && id.len() <= 128
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "Invalid advertised profile identifier"
        );
        ensure!(
            !profiles.iter().any(|p: &Profile| p.id == id),
            "Duplicate advertised profile"
        );
        profiles.push(Profile {
            id,
            cpu_driver: string(entry, "CpuDriver").or_else(|| string(entry, "Driver")),
            platform_driver: string(entry, "PlatformDriver"),
        });
    }
    ensure!(
        !profiles.is_empty() && profiles.iter().any(|p| p.id == active),
        "Active profile is not in the advertised list"
    );
    let holds = match variant(object, "ActiveProfileHolds", "aa{sv}") {
        Some(Value::Array(entries)) => Some(
            entries
                .iter()
                .map(|entry| {
                    Ok(Hold {
                        profile: string(entry, "Profile")
                            .context("Malformed active profile hold")?,
                        application: string(entry, "ApplicationId"),
                        reason: string(entry, "Reason"),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        None => None,
        _ => bail!("Malformed active profile holds"),
    };
    Ok(State {
        api: api.into(),
        service_owner: owner.into(),
        active,
        profiles,
        holds,
        degraded: string(object, "PerformanceDegraded"),
        inhibited: string(object, "PerformanceInhibited"),
        version: string(object, "Version"),
    })
}

pub struct Service<T: Transport> {
    transport: T,
    endpoint: Endpoint,
    owner: String,
}
impl<T: Transport> Service<T> {
    pub fn connect(mut transport: T) -> Result<Self> {
        let mut errors = vec![];
        for endpoint in ENDPOINTS {
            let lookup = transport.call(Call {
                destination: "org.freedesktop.DBus",
                path: "/org/freedesktop/DBus",
                interface: "org.freedesktop.DBus",
                method: "GetNameOwner",
                signature: "s",
                arguments: vec![endpoint.name],
                interactive: false,
            });
            match lookup {
                Ok(response) => {
                    ensure!(response["type"] == "s", "Unexpected D-Bus owner response");
                    let owner = response["data"]
                        .as_array()
                        .filter(|v| v.len() == 1)
                        .and_then(|v| v[0].as_str())
                        .filter(|v| v.starts_with(':'))
                        .context("Power service owner is malformed")?
                        .to_owned();
                    return Ok(Self {
                        transport,
                        endpoint,
                        owner,
                    });
                }
                Err(error) => errors.push(format!("{}: {error}", endpoint.name)),
            }
        }
        bail!(
            "No running compatible power service. Discovery does not start one. {}",
            errors.join("; ")
        )
    }
    pub fn state(&mut self) -> Result<State> {
        let response = self.transport.call(Call {
            destination: &self.owner,
            path: self.endpoint.path,
            interface: "org.freedesktop.DBus.Properties",
            method: "GetAll",
            signature: "s",
            arguments: vec![self.endpoint.name],
            interactive: false,
        })?;
        parse_state(response, self.endpoint.name, &self.owner)
    }
    pub fn set(&mut self, profile: &str) -> Result<State> {
        let before = self.state()?;
        ensure!(
            before.profiles.iter().any(|p| p.id == profile),
            "Requested power profile is not advertised by the running service"
        );
        if before.active == profile {
            return Ok(before);
        }
        ensure!(before.holds.as_ref().is_some_and(Vec::is_empty), "Another application holds a profile, or hold information is unavailable; refusing to override it");
        ensure!(
            profile != "performance" || before.inhibited.as_deref().unwrap_or_default().is_empty(),
            "Performance profile is inhibited by the service"
        );
        self.transport.call(Call {
            destination: &self.owner,
            path: self.endpoint.path,
            interface: "org.freedesktop.DBus.Properties",
            method: "Set",
            signature: "ssv",
            arguments: vec![self.endpoint.name, "ActiveProfile", "s", profile],
            interactive: true,
        })?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let state = self.state()?;
            if state.active == profile {
                return Ok(state);
            }
            ensure!(Instant::now() < deadline, "Service did not confirm requested profile; another client may have changed it. Read current state before retrying.");
            thread::sleep(Duration::from_millis(50));
        }
    }
    pub fn reversible_test(&mut self) -> Result<Value> {
        let before = self.state()?;
        ensure!(
            before.holds.as_ref().is_some_and(Vec::is_empty),
            "Active profile holds prevent a reversible test"
        );
        let alternate = ["balanced", "power-saver", "performance"]
            .into_iter()
            .find(|p| *p != before.active && before.profiles.iter().any(|v| v.id == *p))
            .context("A second profile is required for testing")?
            .to_string();
        let tested = self.set(&alternate);
        // Attempt restoration even after a failed write/readback. New holds may prevent it.
        let restored = self
            .set(&before.active)
            .context("Power test could not restore the original profile; inspect power show")?;
        tested.context("Power test failed; original profile was restored")?;
        Ok(
            serde_json::json!({"before":before.active,"tested":alternate,"after":restored.active,"restored":true,"api":restored.api}),
        )
    }
}

pub fn status() -> Value {
    match Service::connect(Busctl).and_then(|mut service| service.state()) {
        Ok(state) => serde_json::json!({"available":true,"state":state}),
        Err(error) => serde_json::json!({"available":false,"reason":format!("{error:#}")}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    struct Mock {
        replies: VecDeque<Result<Value>>,
        sets: Vec<String>,
    }
    impl Transport for Mock {
        fn call(&mut self, call: Call<'_>) -> Result<Value> {
            if call.method == "Set" {
                self.sets.push(call.arguments.last().unwrap().to_string());
            }
            self.replies.pop_front().expect("Unexpected bus call")
        }
    }
    fn owner() -> Result<Value> {
        Ok(serde_json::json!({"type":"s","data":[":1.42"]}))
    }
    fn state(active: &str, held: bool) -> Value {
        serde_json::json!({"type":"a{sv}","data":[{"ActiveProfile":{"type":"s","data":active},"Profiles":{"type":"aa{sv}","data":[{"Profile":{"type":"s","data":"balanced"}},{"Profile":{"type":"s","data":"performance"}}]},"ActiveProfileHolds":{"type":"aa{sv}","data":if held { vec![serde_json::json!({"Profile":{"type":"s","data":"performance"},"ApplicationId":{"type":"s","data":"test"}})] } else {vec![]}}}]})
    }
    #[test]
    fn supports_legacy_service_when_current_name_is_missing() {
        let mock = Mock {
            replies: VecDeque::from([
                Err(anyhow::anyhow!("not owned")),
                owner(),
                Ok(state("balanced", false)),
            ]),
            sets: vec![],
        };
        let mut service = Service::connect(mock).unwrap();
        assert_eq!(service.state().unwrap().api, "net.hadess.PowerProfiles");
    }
    #[test]
    fn active_holds_prevent_a_write() {
        let mock = Mock {
            replies: VecDeque::from([owner(), Ok(state("balanced", true))]),
            sets: vec![],
        };
        let mut service = Service::connect(mock).unwrap();
        assert!(service
            .set("performance")
            .unwrap_err()
            .to_string()
            .contains("holds"));
        assert!(service.transport.sets.is_empty());
    }
    #[test]
    fn invalid_profile_never_reaches_dbus_set() {
        let mock = Mock {
            replies: VecDeque::from([owner(), Ok(state("balanced", false))]),
            sets: vec![],
        };
        let mut service = Service::connect(mock).unwrap();
        assert!(service.set("invalid").is_err());
        assert!(service.transport.sets.is_empty());
    }
    #[test]
    fn already_active_profile_is_a_noop() {
        let mock = Mock {
            replies: VecDeque::from([owner(), Ok(state("balanced", true))]),
            sets: vec![],
        };
        let mut service = Service::connect(mock).unwrap();
        service.set("balanced").unwrap();
        assert!(service.transport.sets.is_empty());
    }
    #[test]
    fn reversible_test_restores_even_when_first_set_fails() {
        let mock = Mock {
            replies: VecDeque::from([
                owner(),
                Ok(state("performance", false)),
                Ok(state("performance", false)),
                Err(anyhow::anyhow!("denied")),
                Ok(state("balanced", false)),
                Ok(Value::Null),
                Ok(state("performance", false)),
            ]),
            sets: vec![],
        };
        let mut service = Service::connect(mock).unwrap();
        assert!(service
            .reversible_test()
            .unwrap_err()
            .to_string()
            .contains("original profile was restored"));
        assert_eq!(service.transport.sets, ["balanced", "performance"]);
    }
    #[test]
    fn malformed_mandatory_data_is_not_silently_accepted() {
        assert!(parse_state(serde_json::json!({"type":"s","data":[]}), "api", ":1.1").is_err());
        let mut response = state("balanced", false);
        response["data"][0]["Profiles"]["type"] = Value::String("s".into());
        assert!(parse_state(response, "api", ":1.1").is_err());
    }
}
