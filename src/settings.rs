//! User-owned preferences only. Saving never contacts hardware or installs startup automation.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_BYTES: u64 = 65536;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    pub start: u8,
    pub end: u8,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub schema_version: u32,
    pub power_profile: Option<String>,
    pub battery_thresholds: BTreeMap<String, Thresholds>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            power_profile: None,
            battery_thresholds: BTreeMap::new(),
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "Unsupported preferences version; preserve this file for a newer app"
        );
        if let Some(profile) = &self.power_profile {
            ensure!(
                ["power-saver", "balanced", "performance"].contains(&profile.as_str()),
                "Unknown power profile preference"
            );
        }
        ensure!(
            self.battery_thresholds.len() <= 16,
            "Too many battery preferences"
        );
        for (battery, values) in &self.battery_thresholds {
            ensure!(
                !battery.is_empty()
                    && battery.len() <= 128
                    && battery != "."
                    && battery != ".."
                    && battery
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-:.".contains(&b)),
                "Invalid battery identifier in preferences"
            );
            ensure!(
                values.start < values.end && values.end <= 100,
                "Battery {battery} requires 0 <= start < end <= 100"
            );
        }
        Ok(())
    }
}

/// Relative XDG paths are ignored as required by the XDG specification.
pub fn path_from(config: Option<&Path>, home: Option<&Path>) -> Result<PathBuf> {
    let base = config
        .filter(|v| v.is_absolute())
        .map(Path::to_path_buf)
        .or_else(|| home.filter(|v| v.is_absolute()).map(|v| v.join(".config")))
        .context("No absolute XDG_CONFIG_HOME or HOME is available")?;
    Ok(base.join("thinkpad-dashboard/preferences.json"))
}
pub fn path() -> Result<PathBuf> {
    path_from(
        std::env::var_os("XDG_CONFIG_HOME")
            .as_deref()
            .map(Path::new),
        std::env::var_os("HOME").as_deref().map(Path::new),
    )
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Cannot safely open preferences"),
    };
    let meta = file.metadata()?;
    // SAFETY: geteuid takes no pointers and only reads process credentials.
    ensure!(
        meta.is_file() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o022 == 0,
        "Preferences must be an owned regular file, not writable by other users"
    );
    let mut data = vec![];
    file.take(MAX_BYTES + 1).read_to_end(&mut data)?;
    ensure!(
        data.len() <= MAX_BYTES as usize,
        "Preferences exceed size limit"
    );
    Ok(Some(data))
}
fn decode(data: &[u8]) -> Result<Preferences> {
    let preferences: Preferences = serde_json::from_slice(data)
        .context("Preferences are malformed; existing data was preserved")?;
    preferences.validate()?;
    Ok(preferences)
}
pub fn load(path: &Path) -> Result<Preferences> {
    match read_optional(path)? {
        Some(data) => decode(&data),
        None => Ok(Preferences::default()),
    }
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().context("Preferences path has no parent")?;
    let temp = parent.join(format!(
        ".preferences-{}-{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut created = false;
    let operation = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp)?;
        created = true;
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        File::open(parent)?
            .sync_all()
            .context("Preferences renamed but directory sync failed; verify saved state")?;
        Ok(())
    })();
    if operation.is_err() && created {
        let _ = fs::remove_file(temp);
    }
    operation
}

pub fn save(path: &Path, preferences: &Preferences) -> Result<()> {
    preferences.validate()?;
    let mut data = serde_json::to_vec_pretty(preferences)?;
    data.push(b'\n');
    let parent = path.parent().context("Preferences path has no parent")?;
    ensure!(
        parent.is_absolute(),
        "Preferences directory must be absolute"
    );
    if let Some(base) = parent.parent() {
        fs::create_dir_all(base)?;
    }
    match fs::DirBuilder::new().mode(0o700).create(parent) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let meta = fs::symlink_metadata(parent)?;
    // SAFETY: geteuid has no pointer parameters.
    ensure!(
        meta.is_dir() && meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o077 == 0,
        "Preferences directory must be owned, private, and not a symlink"
    );
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(parent.join(".preferences.lock"))?;
    let meta = lock.metadata()?;
    // SAFETY: geteuid has no pointer parameters; lock owns a valid descriptor for flock.
    ensure!(
        meta.is_file()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.nlink() == 1
            && meta.mode() & 0o077 == 0,
        "Unsafe preferences lock"
    );
    // SAFETY: flock uses a live owned descriptor and does not borrow Rust pointers.
    ensure!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
        "Another preferences save is in progress"
    );
    if let Some(old) = read_optional(path)? {
        decode(&old)?; // Never overwrite malformed data or a newer schema.
        if old == data {
            return Ok(());
        }
        atomic_write(&parent.join("preferences.previous.json"), &old)?;
    }
    atomic_write(path, &data)
}

/// Hardware-independent saved requests. Application is an explicit, separate user action.
pub fn battery_requests(preferences: &Preferences) -> Vec<crate::ControlRequest> {
    preferences
        .battery_thresholds
        .iter()
        .map(
            |(battery, value)| crate::ControlRequest::BatteryThresholds {
                battery: battery.clone(),
                start: value.start,
                end: value.end,
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn desired() -> Preferences {
        Preferences {
            schema_version: 1,
            power_profile: Some("balanced".into()),
            battery_thresholds: BTreeMap::from([(
                "BAT0".into(),
                Thresholds { start: 75, end: 80 },
            )]),
        }
    }
    #[test]
    fn relative_xdg_is_ignored_and_missing_home_is_explained() {
        assert_eq!(
            path_from(Some(Path::new("relative")), Some(Path::new("/home/test"))).unwrap(),
            Path::new("/home/test/.config/thinkpad-dashboard/preferences.json")
        );
        assert!(path_from(None, None).is_err());
        assert_eq!(
            path_from(Some(Path::new("/custom")), None).unwrap(),
            Path::new("/custom/thinkpad-dashboard/preferences.json")
        );
    }
    #[test]
    fn missing_preferences_are_empty_without_creating_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("missing/preferences.json");
        assert_eq!(load(&path).unwrap(), Preferences::default());
        assert!(!path.parent().unwrap().exists());
    }
    #[test]
    fn saving_round_trips_with_private_files_and_previous_backup() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/preferences.json");
        save(&path, &desired()).unwrap();
        let original = fs::read(&path).unwrap();
        assert_eq!(load(&path).unwrap(), desired());
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        let next = Preferences::default();
        save(&path, &next).unwrap();
        assert_eq!(load(&path).unwrap(), next);
        assert_eq!(
            fs::read(path.with_file_name("preferences.previous.json")).unwrap(),
            original
        );
    }
    #[test]
    fn invalid_preferences_never_replace_valid_data() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/preferences.json");
        save(&path, &desired()).unwrap();
        let before = fs::read(&path).unwrap();
        let mut invalid = desired();
        invalid.battery_thresholds.get_mut("BAT0").unwrap().end = 50;
        assert!(save(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    #[test]
    fn malformed_or_future_settings_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/preferences.json");
        save(&path, &desired()).unwrap();
        for data in [
            b"broken json".as_slice(),
            br#"{"schema_version":2,"power_profile":null,"battery_thresholds":{}}"#,
        ] {
            fs::write(&path, data).unwrap();
            assert!(load(&path).is_err());
            assert!(save(&path, &desired()).is_err());
            assert_eq!(fs::read(&path).unwrap(), data);
        }
    }
    #[test]
    fn symlink_preferences_cannot_redirect_a_save() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/preferences.json");
        save(&path, &desired()).unwrap();
        fs::remove_file(&path).unwrap();
        let target = root.path().join("target");
        fs::write(&target, "untouched").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(save(&path, &desired()).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "untouched");
    }
    #[test]
    fn unknown_fields_and_unsafe_names_are_rejected() {
        assert!(serde_json::from_str::<Preferences>(
            r#"{"schema_version":1,"power_profile":null,"battery_thresholds":{},"auto_apply":true}"#
        )
        .is_err());
        let mut p = desired();
        p.battery_thresholds
            .insert("../../sys".into(), Thresholds { start: 75, end: 80 });
        assert!(p.validate().is_err());
    }
    #[test]
    fn directory_symlinks_and_shared_directories_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/preferences.json");
        fs::create_dir(path.parent().unwrap()).unwrap();
        fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(save(&path, &desired()).is_err());
        fs::remove_dir(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(root.path(), path.parent().unwrap()).unwrap();
        assert!(save(&path, &desired()).is_err());
    }
}
