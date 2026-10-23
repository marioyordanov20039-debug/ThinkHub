//! BIOS updates delegated to fwupd; no raw firmware or arbitrary-file installation.
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const CLIENT: &str = "/usr/bin/fwupdmgr";
const PKEXEC: &str = "/usr/bin/pkexec";
const READ_FLAGS: [&str; 5] = [
    "--json",
    "--no-authenticate",
    "--no-metadata-check",
    "--no-remote-check",
    "--no-unreported-check",
];
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Release {
    pub version: String,
    pub checksum_sha256: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct BiosPlan {
    #[serde(skip_serializing)]
    device_id: String,
    pub component: String,
    pub installed_version: String,
    pub release: Option<Release>,
    pub blockers: Vec<String>,
    pub metadata_age_seconds: Option<u64>,
}
impl BiosPlan {
    pub fn ready(&self) -> bool {
        self.release.is_some() && self.blockers.is_empty()
    }
}
pub trait Backend {
    fn query(&mut self, operation: &str) -> Result<Value>;
    fn supports_version_install(&mut self) -> Result<bool>;
    fn power_blockers(&mut self) -> Vec<String>;
    fn install(&mut self, device: &str, version: &str) -> Result<String>;
}
pub struct System;
fn has(v: &Value, key: &str, word: &str) -> bool {
    v[key]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(word)))
}
fn string(v: &Value, key: &str) -> Result<String> {
    v[key]
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("fwupd omitted {key}"))
}
fn hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn valid_version(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}
pub fn plan(backend: &mut impl Backend, now: u64) -> Result<BiosPlan> {
    let data = backend.query("get-devices")?;
    let devices = data["Devices"]
        .as_array()
        .context("fwupd did not return a device list")?;
    // The signed release category distinguishes BIOS from EC, ME, dbx and peripherals.
    let candidates: Vec<_> = devices
        .iter()
        .filter(|d| {
            d["Plugin"] == "uefi_capsule"
                && has(d, "Flags", "internal")
                && has(d, "Protocols", "org.uefi.capsule")
                && d["Releases"]
                    .as_array()
                    .is_some_and(|r| r.iter().any(|r| has(r, "Categories", "X-System")))
        })
        .collect();
    ensure!(candidates.len()==1,"No unambiguous supported UEFI system BIOS was found in fwupd metadata. This model or firmware service may not offer BIOS updates.");
    let d = candidates[0];
    let device_id = string(d, "DeviceId")?;
    ensure!(hex(&device_id, 40), "Invalid fwupd BIOS device identifier");
    let mut blockers = backend.power_blockers();
    if !backend.supports_version_install()? {
        blockers.push("This fwupdmgr version does not support exact-version installation. A newer fwupd client is required for ThinkHub BIOS updates.".into());
    }
    if !has(d, "Flags", "updatable") {
        blockers.push("fwupd does not currently allow updating this BIOS.".into());
    }
    for problem in d["Problems"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        blockers.push(format!("fwupd: {problem}"));
    }
    if matches!(d["UpdateState"].as_u64(), Some(1 | 4)) {
        blockers.push("A firmware update is already pending. Complete the required restart before preparing another update.".into());
    }
    if matches!(d["UpdateState"].as_u64(), Some(3 | 5)) {
        blockers.push(format!("fwupd reports a previous BIOS update failure. Review and resolve it before retrying. {}",d["UpdateError"].as_str().unwrap_or("Check fwupd update results.")));
    }
    let release = d["Releases"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| {
            r["RemoteId"] == "lvfs"
                && r["Protocol"] == "org.uefi.capsule"
                && has(r, "Categories", "X-System")
                && has(r, "Flags", "trusted-metadata")
                && has(r, "Flags", "is-upgrade")
                && ![
                    "is-downgrade",
                    "blocked-version",
                    "blocked-approval",
                    "is-alternate-branch",
                    "is-community",
                ]
                .iter()
                .any(|f| has(r, "Flags", f))
        })
        .map(|r| -> Result<Release> {
            let version = string(r, "Version")?;
            ensure!(valid_version(&version), "Invalid BIOS release version");
            let checksum_sha256 = r["Checksum"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .find(|s| hex(s, 64))
                .context("BIOS release has no SHA-256 checksum")?
                .to_owned();
            Ok(Release {
                version,
                checksum_sha256,
            })
        })
        .transpose()?;
    if release.is_none() {
        blockers.push("No stable, trusted LVFS BIOS upgrade is listed in the local catalog. This does not prove that your BIOS is current.".into());
    }
    let remotes = backend.query("get-remotes")?;
    let remote = remotes["Remotes"]
        .as_array()
        .context("fwupd did not return remotes")?
        .iter()
        .find(|r| r["Id"] == "lvfs" && r["Enabled"] == true);
    let age = remote
        .and_then(|r| r["Mtime"].as_u64())
        .filter(|mtime| *mtime > 0)
        .and_then(|mtime| now.checked_sub(mtime));
    if age.is_none_or(|age| age > 86400) {
        blockers.push("Refresh the LVFS update catalog before preparing a BIOS update (catalog missing, future-dated, or older than 24 hours).".into());
    }
    Ok(BiosPlan {
        device_id,
        component: string(d, "Name")?,
        installed_version: string(d, "Version")?,
        release,
        blockers,
        metadata_age_seconds: age,
    })
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn inspect() -> Result<BiosPlan> {
    plan(&mut System, now())
}
pub fn install_reviewed(backend: &mut impl Backend, reviewed: &BiosPlan) -> Result<String> {
    ensure!(reviewed.ready(), "The reviewed BIOS update is blocked");
    let current = plan(backend, now())?;
    ensure!(
        current.ready(),
        "BIOS readiness changed: {}",
        current.blockers.join("\n")
    );
    ensure!(
        current.device_id == reviewed.device_id
            && current.installed_version == reviewed.installed_version
            && current.release == reviewed.release,
        "The BIOS device or release changed. Check and review it again."
    );
    backend.install(
        &current.device_id,
        &current.release.as_ref().unwrap().version,
    )
}
fn install_arguments(device: &str, version: &str) -> Result<Vec<String>> {
    ensure!(
        hex(device, 40) && valid_version(version),
        "Invalid BIOS installation target"
    );
    Ok([
        "install",
        device,
        version,
        "--json",
        "--no-reboot-check",
        "--no-unreported-check",
        "--no-remote-check",
        "--no-metadata-check",
        "--no-device-prompt",
        "--no-security-fix",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect())
}
fn authorized_arguments(device: &str, version: &str) -> Result<Vec<String>> {
    let mut args = vec!["--disable-internal-agent".to_owned(), CLIENT.to_owned()];
    args.extend(install_arguments(device, version)?);
    Ok(args)
}
pub fn refresh() -> Result<String> {
    let output = crate::process::run(
        Path::new(CLIENT),
        &[
            "refresh",
            "--json",
            "--no-reboot-check",
            "--no-remote-check",
            "--no-unreported-check",
        ],
        Duration::from_secs(180),
        2_000_000,
    )?;
    ensure!(
        output.status.success() || output.status.code() == Some(2),
        "Catalog refresh failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(
        "Update catalog refresh completed. Check BIOS updates again; no firmware was installed."
            .into(),
    )
}
fn power_readiness(s: &crate::Snapshot) -> Vec<String> {
    let mut errors = vec![];
    if !s.power_sources.iter().any(|p| p.online == Some(true)) {
        errors.push("Connect external AC power; external power has not been confirmed.".into());
    }
    let batteries: Vec<_> = s
        .batteries
        .iter()
        .filter(|b| b.present == Some(true) && b.scope.as_deref() != Some("Device"))
        .collect();
    if batteries.is_empty() {
        errors.push("A present laptop battery could not be confirmed.".into());
    }
    for b in batteries {
        if b.percent
            .is_none_or(|p| !p.is_finite() || !(30.0..=100.0).contains(&p))
        {
            errors.push(format!(
                "{} must have a known charge of at least 30%. fwupd may require a higher level.",
                b.id
            ));
        }
    }
    errors
}

impl Backend for System {
    fn query(&mut self, operation: &str) -> Result<Value> {
        ensure!(
            ["get-devices", "get-remotes"].contains(&operation),
            "Unsupported firmware query"
        );
        let mut args = vec![operation];
        args.extend(READ_FLAGS);
        let out =
            crate::process::run(Path::new(CLIENT), &args, Duration::from_secs(35), 8_000_000)?;
        ensure!(
            out.status.success(),
            "Firmware query failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).context("Invalid fwupd JSON")
    }
    fn supports_version_install(&mut self) -> Result<bool> {
        crate::authorization::trusted_helper(Path::new(CLIENT))
            .context("The system fwupd client is not trusted")?;
        ensure!(
            Path::new(PKEXEC).is_file(),
            "Install polkit and a desktop authentication agent to authorize BIOS updates"
        );
        let out = crate::process::run(
            Path::new(CLIENT),
            &["--help"],
            Duration::from_secs(5),
            100_000,
        )?;
        Ok(String::from_utf8_lossy(&out.stdout).lines().any(|l| {
            l.trim_start().starts_with("install ")
                && l.contains("[VERSION]")
                && l.contains("[DEVICE-ID|GUID]")
        }))
    }
    fn power_blockers(&mut self) -> Vec<String> {
        power_readiness(&crate::scan(Path::new("/sys")))
    }
    fn install(&mut self, device: &str, version: &str) -> Result<String> {
        ensure!(unsafe {libc::geteuid()}!=0,"Run ThinkHub as your normal user; BIOS preparation must request system administrator authorization");
        crate::authorization::trusted_helper(Path::new(CLIENT))?;
        let args = authorized_arguments(device, version)?;
        // Only the fixed, root-owned system client is elevated. Password entry belongs
        // to polkit; the GUI neither receives nor stores credentials. No kill timeout.
        let mut child = Command::new(PKEXEC)
            // fwupdmgr's install has a legacy file-path fallback. A root-owned cwd
            // prevents a user file named like the device ID from selecting that path.
            .current_dir("/")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start fwupd installation")?;
        let drain = |mut pipe: Box<dyn Read + Send>| {
            std::thread::spawn(move || {
                let mut output = vec![];
                let mut buffer = [0; 8192];
                loop {
                    match pipe.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            let keep = n.min(2_000_000usize.saturating_sub(output.len()));
                            output.extend_from_slice(&buffer[..keep]);
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok(output)
            })
        };
        let out = drain(Box::new(child.stdout.take().unwrap()));
        let err = drain(Box::new(child.stderr.take().unwrap()));
        let status = child.wait()?;
        let _out = out
            .join()
            .map_err(|_| anyhow::anyhow!("Firmware output reader failed"))??;
        let err = err
            .join()
            .map_err(|_| anyhow::anyhow!("Firmware error reader failed"))??;
        ensure!(
            status.code() != Some(126),
            "System authorization was cancelled. No BIOS installation was started."
        );
        ensure!(status.code()!=Some(127),"System authorization was denied or no authentication agent is available. No BIOS installation was started.");
        ensure!(status.success(),"fwupd did not complete successfully: {}. Check firmware status before retrying; staging may be partial.",String::from_utf8_lossy(&err));
        Ok("fwupd completed the BIOS preparation request. A restart is normally required to apply UEFI BIOS updates. Keep AC connected and follow the firmware updater screen without interrupting it. ThinkHub has not restarted the computer or verified the new BIOS after boot.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Fake {
        data: Value,
        remote: Value,
        power: Vec<String>,
        installed: bool,
    }
    impl Backend for Fake {
        fn query(&mut self, op: &str) -> Result<Value> {
            Ok(if op == "get-devices" {
                self.data.clone()
            } else {
                self.remote.clone()
            })
        }
        fn supports_version_install(&mut self) -> Result<bool> {
            Ok(true)
        }
        fn power_blockers(&mut self) -> Vec<String> {
            self.power.clone()
        }
        fn install(&mut self, _: &str, _: &str) -> Result<String> {
            self.installed = true;
            Ok("staged".into())
        }
    }
    fn fixture() -> Fake {
        Fake {
            data: json!({"Devices":[{"Name":"System Firmware","DeviceId":"a".repeat(40),"Plugin":"uefi_capsule","Protocols":["org.uefi.capsule"],"Flags":["internal","updatable"],"Version":"1.0","Releases":[{"Version":"1.1","Categories":["X-System"],"RemoteId":"lvfs","Protocol":"org.uefi.capsule","Flags":["trusted-metadata","is-upgrade"],"Checksum":["b".repeat(64)]}]}]}),
            remote: json!({"Remotes":[{"Id":"lvfs","Enabled":true,"Mtime":now()}]}),
            power: vec![],
            installed: false,
        }
    }
    #[test]
    fn review_is_read_only_and_install_is_exact_and_revalidated() {
        let mut f = fixture();
        let reviewed = plan(&mut f, now()).unwrap();
        assert!(!serde_json::to_string(&reviewed)
            .unwrap()
            .contains(&"a".repeat(40)));
        assert!(reviewed.ready());
        assert!(!f.installed);
        f.power.push("AC removed".into());
        assert!(install_reviewed(&mut f, &reviewed).is_err());
        assert!(!f.installed);
        f.power.clear();
        f.data["Devices"][0]["Releases"][0]["Version"] = json!("1.2");
        assert!(install_reviewed(&mut f, &reviewed).is_err());
        assert!(!f.installed);
        f.data["Devices"][0]["Releases"][0]["Version"] = json!("1.1");
        install_reviewed(&mut f, &reviewed).unwrap();
        assert!(f.installed);
    }
    #[test]
    fn non_bios_untrusted_and_stale_metadata_never_enable_install() {
        let mut f = fixture();
        f.data["Devices"][0]["Releases"][0]["Categories"] = json!(["X-EmbeddedController"]);
        assert!(plan(&mut f, now()).is_err());
        let mut f = fixture();
        f.data["Devices"][0]["Releases"][0]["Flags"] = json!(["is-upgrade"]);
        assert!(!plan(&mut f, now()).unwrap().ready());
        let mut f = fixture();
        f.remote["Remotes"][0]["Mtime"] = json!(1);
        assert!(!plan(&mut f, now()).unwrap().ready());
    }
    #[test]
    fn exact_arguments_reject_wildcards_and_disable_automatic_reboot() {
        let authorized = authorized_arguments(&"a".repeat(40), "1.2").unwrap();
        assert_eq!(&authorized[..2], &["--disable-internal-agent", CLIENT]);
        let args = install_arguments(&"a".repeat(40), "1.2").unwrap();
        assert_eq!(&args[..3], &["install", "a".repeat(40).as_str(), "1.2"]);
        assert!(args.iter().any(|a| a == "--no-reboot-check"));
        for flag in [
            "--force",
            "--allow-older",
            "--allow-reinstall",
            "--assume-yes",
            "--no-safety-check",
        ] {
            assert!(!args.iter().any(|a| a == flag));
        }
        assert!(install_arguments("*", "1.2").is_err());
        assert!(install_arguments(&"a".repeat(40), "--force").is_err());
    }
    #[test]
    fn absent_low_and_unknown_batteries_or_ac_are_blocked() {
        let root = tempfile::tempdir().unwrap();
        assert!(!power_readiness(&crate::scan(root.path())).is_empty());
        let ac = root.path().join("class/power_supply/AC");
        let bat = root.path().join("class/power_supply/BAT0");
        for p in [&ac, &bat] {
            std::fs::create_dir_all(p).unwrap();
        }
        for (p, value) in [
            (ac.join("type"), "Mains"),
            (ac.join("online"), "1"),
            (bat.join("type"), "Battery"),
            (bat.join("present"), "1"),
            (bat.join("capacity"), "50"),
        ] {
            std::fs::write(p, value).unwrap();
        }
        assert!(power_readiness(&crate::scan(root.path())).is_empty());
        std::fs::write(bat.join("capacity"), "29").unwrap();
        assert!(!power_readiness(&crate::scan(root.path())).is_empty());
        std::fs::write(bat.join("capacity"), "50").unwrap();
        std::fs::write(ac.join("online"), "0").unwrap();
        assert!(!power_readiness(&crate::scan(root.path())).is_empty());
    }
}
