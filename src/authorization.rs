use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::Path,
    process::Command,
};

pub const HELPER: &str = "/usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper";
pub const POLICY: &str = "/usr/share/polkit-1/actions/io.github.thinkpaddashboard.control.policy";

#[derive(Debug, Serialize)]
pub struct Installation {
    pub helper_installed: bool,
    pub trusted: bool,
    pub policy_installed: bool,
    pub detail: String,
    pub helper_version: Option<String>,
    pub matches_app_version: bool,
}

pub fn trusted_helper(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "Helper path must be absolute");
    for component in path.ancestors() {
        let meta = fs::symlink_metadata(component).context("Helper is not installed")?;
        ensure!(
            !meta.file_type().is_symlink(),
            "Helper path must not contain symlinks"
        );
        ensure!(meta.uid() == 0 && meta.mode() & 0o022 == 0, "Helper and its parent directories must be root-owned and not writable by group or others");
    }
    let meta = fs::metadata(path)?;
    ensure!(
        meta.is_file() && meta.mode() & 0o111 != 0 && meta.mode() & 0o6000 == 0,
        "Helper must be an executable regular file without setuid/setgid bits"
    );
    Ok(())
}
pub fn installation() -> Installation {
    let validation = trusted_helper(Path::new(HELPER));
    let helper_version = if validation.is_ok() {
        crate::process::run(
            Path::new(HELPER),
            &["--version"],
            std::time::Duration::from_secs(1),
            1024,
        )
        .ok()
        .filter(|v| v.status.success())
        .and_then(|v| String::from_utf8(v.stdout).ok())
        .and_then(|v| parse_version(&v))
    } else {
        None
    };
    let matches_app_version = helper_version.as_deref() == Some(env!("CARGO_PKG_VERSION"));
    Installation {
        helper_installed: Path::new(HELPER).exists(),
        trusted: validation.is_ok(),
        policy_installed: Path::new(POLICY).is_file(),
        detail: validation
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "Root-owned helper validated".into()),
        helper_version,
        matches_app_version,
    }
}
fn parse_version(value: &str) -> Option<String> {
    let version = value.trim().strip_prefix("thinkpad-dashboard-helper ")?;
    if version.split('.').count() != 3
        || version
            .split('.')
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some(version.to_string())
}

/// Only the fixed installed helper is eligible for elevation. PATH is never used.
pub fn invoke(operation: &str, payload: Option<&str>) -> Result<()> {
    ensure!(
        [
            "--apply",
            "--test-keyboard",
            "--test-trackpoint",
            "--check",
            "--bios-info"
        ]
        .contains(&operation),
        "Unknown helper operation"
    );
    trusted_helper(Path::new(HELPER))?;
    ensure!(installation().matches_app_version, "Installed helper does not match this app version; update the local installation before requesting hardware changes");
    let mut command = Command::new("/usr/bin/pkexec");
    command.args(["--disable-internal-agent", HELPER, operation]);
    if let Some(payload) = payload {
        command.arg(payload);
    }
    let status = command
        .status()
        .context("Could not start polkit; install pkexec and a desktop authentication agent")?;
    match status.code() {
        Some(0) => Ok(()),
        Some(126) => bail!("Administrator authorization was cancelled; no operation was started"),
        Some(127) => {
            bail!("Administrator authorization failed or no authentication agent was available")
        }
        _ => bail!("Hardware helper failed; review its error and scan again"),
    }
}

/// Advisory lock serializes our helpers; it cannot lock out unrelated hardware tools.
pub struct OperationLock {
    _file: File,
}
impl OperationLock {
    pub fn acquire() -> Result<Self> {
        Self::acquire_at(Path::new("/run/thinkpad-dashboard"), 0)
    }
    fn acquire_at(directory: &Path, expected_owner: u32) -> Result<Self> {
        match fs::DirBuilder::new().mode(0o700).create(directory) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let meta = fs::symlink_metadata(directory)?;
        ensure!(
            meta.is_dir() && meta.uid() == expected_owner && meta.mode() & 0o077 == 0,
            "Unsafe helper lock directory"
        );
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(directory.join("operation.lock"))?;
        let meta = file.metadata()?;
        ensure!(
            meta.is_file()
                && meta.uid() == expected_owner
                && meta.nlink() == 1
                && meta.mode() & 0o077 == 0,
            "Unsafe helper lock file"
        );
        // SAFETY: file owns a live descriptor; flock does not borrow a Rust pointer.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            bail!(
                "Another hardware operation is running or locking failed: {}",
                std::io::Error::last_os_error()
            );
        }
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn user_owned_helper_is_never_elevated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("helper");
        fs::write(&path, "not a helper").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(trusted_helper(&path).is_err());
    }

    #[test]
    fn operations_are_serialized_and_lock_releases_on_drop() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("locks");
        let owner = fs::metadata(root.path()).unwrap().uid();
        let first = OperationLock::acquire_at(&path, owner).unwrap();
        assert!(OperationLock::acquire_at(&path, owner).is_err());
        drop(first);
        assert!(OperationLock::acquire_at(&path, owner).is_ok());
    }

    #[test]
    fn lock_file_symlinks_and_insecure_directories_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("locks");
        let owner = fs::metadata(root.path()).unwrap().uid();
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(OperationLock::acquire_at(&path, owner).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let target = root.path().join("target");
        fs::write(&target, "leave untouched").unwrap();
        std::os::unix::fs::symlink(&target, path.join("operation.lock")).unwrap();
        assert!(OperationLock::acquire_at(&path, owner).is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "leave untouched");
    }

    #[test]
    fn helper_version_response_is_strict_and_unambiguous() {
        assert_eq!(
            parse_version("thinkpad-dashboard-helper 0.5.0\n"),
            Some("0.5.0".into())
        );
        for value in [
            "helper 0.5.0",
            "thinkpad-dashboard-helper 0.5",
            "thinkpad-dashboard-helper 0.x.0",
            "thinkpad-dashboard-helper 0.5.0\nextra",
        ] {
            assert!(parse_version(value).is_none());
        }
    }
}
