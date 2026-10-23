//! Additional local hardware information without unique identifiers or subprocesses.
use crate::probe::{directories, id, number, read};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct Inventory {
    pub os_name: Option<String>,
    pub kernel: Option<String>,
    pub architecture: String,
    pub cpu_model: Option<String>,
    pub logical_cpus: usize,
    pub memory_total_bytes: Option<u64>,
    pub memory_available_bytes: Option<u64>,
    pub graphics: Vec<Graphics>,
    pub drives: Vec<Drive>,
}
#[derive(Debug, Serialize)]
pub struct Graphics {
    pub vendor: String,
    pub pci_device_id: Option<String>,
    pub driver: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Drive {
    pub id: String,
    pub model: Option<String>,
    pub size_bytes: Option<u64>,
    pub rotational: Option<bool>,
    pub removable: Option<bool>,
}

pub fn scan(sys: &Path, proc: &Path, os_release: &Path) -> Inventory {
    let cpuinfo = read(proc.join("cpuinfo")).unwrap_or_default();
    let mut cpu_model = None;
    let mut logical_cpus = 0;
    for line in cpuinfo.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "processor" => logical_cpus += 1,
            "model name" | "Hardware" | "Processor" if cpu_model.is_none() => {
                cpu_model = Some(value.trim().into())
            }
            _ => {}
        }
    }
    let memory = read(proc.join("meminfo")).unwrap_or_default();
    let mem = |key: &str| {
        memory.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name != key {
                return None;
            }
            let mut parts = value.split_whitespace();
            let amount: u64 = parts.next()?.parse().ok()?;
            (parts.next()? == "kB")
                .then(|| amount.checked_mul(1024))
                .flatten()
        })
    };
    let mut graphics = vec![];
    for p in directories(sys.join("bus/pci/devices")) {
        if !read(p.join("class"))
            .unwrap_or_default()
            .starts_with("0x03")
        {
            continue;
        }
        let vendor_id = read(p.join("vendor")).unwrap_or_default();
        let vendor = match vendor_id.as_str() {
            "0x8086" => "Intel",
            "0x1002" => "AMD",
            "0x10de" => "NVIDIA",
            _ => vendor_id.as_str(),
        };
        graphics.push(Graphics {
            vendor: vendor.into(),
            pci_device_id: read(p.join("device")),
            driver: std::fs::read_link(p.join("driver")).ok().map(|v| id(&v)),
        });
    }
    let mut drives = vec![];
    for p in directories(sys.join("class/block")) {
        if p.join("partition").exists() || !p.join("device").exists() {
            continue;
        }
        drives.push(Drive {
            id: id(&p),
            model: read(p.join("device/model")),
            size_bytes: number(p.join("size")).and_then(|v| v.checked_mul(512)),
            rotational: number(p.join("queue/rotational")).map(|v| v == 1),
            removable: number(p.join("removable")).map(|v| v == 1),
        });
    }
    Inventory {
        os_name: read(os_release).and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("PRETTY_NAME=")
                    .map(|v| v.trim_matches('"').to_string())
            })
        }),
        kernel: read(proc.join("sys/kernel/osrelease")),
        architecture: std::env::consts::ARCH.into(),
        cpu_model,
        logical_cpus,
        memory_total_bytes: mem("MemTotal"),
        memory_available_bytes: mem("MemAvailable"),
        graphics,
        drives,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn memory_is_converted_from_kibibytes_and_missing_values_stay_unknown() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("meminfo"),
            "MemTotal: 4096 kB\nMemAvailable: 0 kB",
        )
        .unwrap();
        fs::write(
            root.path().join("cpuinfo"),
            "processor : 0\nmodel name : Test CPU\nprocessor : 1",
        )
        .unwrap();
        let result = scan(root.path(), root.path(), &root.path().join("os-release"));
        assert_eq!(result.memory_total_bytes, Some(4194304));
        assert_eq!(result.memory_available_bytes, Some(0));
        assert_eq!(result.logical_cpus, 2);
        assert!(result.os_name.is_none());
    }
    #[test]
    fn disks_use_512_byte_sectors_regardless_of_logical_block_size() {
        let root = tempfile::tempdir().unwrap();
        let disk = root.path().join("class/block/nvme0n1");
        fs::create_dir_all(disk.join("device")).unwrap();
        fs::create_dir_all(disk.join("queue")).unwrap();
        fs::write(disk.join("size"), "100").unwrap();
        fs::write(disk.join("queue/logical_block_size"), "4096").unwrap();
        fs::write(disk.join("device/serial"), "SECRET").unwrap();
        let result = scan(root.path(), root.path(), &root.path().join("missing"));
        assert_eq!(result.drives[0].size_bytes, Some(51200));
        assert!(!serde_json::to_string(&result).unwrap().contains("SECRET"));
    }
}
