//! App-only appearance, contrast-aware accents and atomic user storage.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
pub const PRESETS: [(&str, &str); 8] = [
    ("ThinkPad red", "#d9233b"),
    ("Blue", "#2875d9"),
    ("Teal", "#008577"),
    ("Green", "#388447"),
    ("Purple", "#8359ca"),
    ("Pink", "#ce3788"),
    ("Orange", "#d36a16"),
    ("Gold", "#e5b52a"),
];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Appearance {
    pub schema_version: u32,
    pub theme: Theme,
    pub accent: String,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            schema_version: 1,
            theme: Theme::Dark,
            accent: PRESETS[0].1.into(),
        }
    }
}
impl Appearance {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("Unsupported appearance settings version".into());
        }
        rgb(&self.accent).map(|_| ())
    }
    pub fn css(&self) -> String {
        let palette = match self.theme {
            Theme::Dark => [
                "#111315", "#ededee", "#191b1e", "#353940", "#aeb3bc", "#292c31", "#2c2e32",
                "#1e2125", "#30343a", "#41464e", "#14171a", "#35383d", "#e4c083",
            ],
            Theme::Light => [
                "#f3f4f6", "#20242b", "#e9ecf0", "#c7cdd5", "#555e6b", "#dce1e8", "#dce1e8",
                "#ffffff", "#e5e9ef", "#d5dce5", "#ffffff", "#d1d7e0", "#805612",
            ],
        };
        let names = [
            "bg",
            "fg",
            "sidebar",
            "border",
            "muted",
            "hover",
            "selected",
            "card",
            "button",
            "button_hover",
            "input",
            "trough",
            "notice",
        ];
        let mut css = String::new();
        for (n, v) in names.into_iter().zip(palette) {
            css.push_str(&format!("@define-color {n} {v};\n"));
        }
        let base = rgb(&self.accent).unwrap_or([217, 35, 59]);
        let ink = if contrast(base, [255; 3]) >= contrast(base, [0; 3]) {
            "#ffffff"
        } else {
            "#000000"
        };
        let surface = rgb(palette[if self.theme == Theme::Dark { 8 } else { 6 }]).unwrap();
        let toward = if self.theme == Theme::Dark {
            [255; 3]
        } else {
            [0; 3]
        };
        let mut text = base;
        for _ in 0..256 {
            if contrast(text, surface) >= 4.5 {
                break;
            }
            text = mix(text, toward, 0.12);
        }
        for (n, v) in [
            ("accent", hex(base)),
            ("accent_text", hex(text)),
            ("accent_ink", ink.into()),
            (
                "accent_hover",
                hex(mix(
                    base,
                    if ink == "#ffffff" { [0; 3] } else { [255; 3] },
                    0.08,
                )),
            ),
        ] {
            css.push_str(&format!("@define-color {n} {v};\n"));
        }
        css + include_str!("../assets/style.css")
    }
}
fn rgb(s: &str) -> Result<[u8; 3], String> {
    if s.len() != 7 || !s.starts_with('#') || !s.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit) {
        return Err("Accent must be a six-digit hex color".into());
    }
    Ok([
        u8::from_str_radix(&s[1..3], 16).unwrap(),
        u8::from_str_radix(&s[3..5], 16).unwrap(),
        u8::from_str_radix(&s[5..7], 16).unwrap(),
    ])
}
fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}
fn mix(a: [u8; 3], b: [u8; 3], f: f64) -> [u8; 3] {
    std::array::from_fn(|i| (a[i] as f64 * (1. - f) + b[i] as f64 * f).round() as u8)
}
fn luminance(c: [u8; 3]) -> f64 {
    c.iter()
        .zip([0.2126, 0.7152, 0.0722])
        .map(|(c, w)| {
            let c = *c as f64 / 255.;
            w * if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
        .sum()
}
fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
pub fn path() -> Result<PathBuf, String> {
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(".config"))
        })
        .ok_or("No absolute user configuration directory")?;
    Ok(root.join("thinkhub/appearance.json"))
}
pub fn load(path: &Path) -> Result<Appearance, String> {
    if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink() || !m.is_file())
    {
        return Err("Appearance settings must be a regular file".into());
    }
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Appearance::default()),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = vec![];
    file.take(16_385)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16_384 {
        return Err("Appearance file is too large".into());
    }
    let value: Appearance = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Cannot read appearance settings: {e}"))?;
    value.validate()?;
    Ok(value)
}
pub fn save(path: &Path, value: &Appearance) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt;
    value.validate()?;
    let parent = path.parent().ok_or("Missing settings directory")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|e| e.to_string())?;
    if parent
        .symlink_metadata()
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Appearance directory must not be a symlink".into());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary
        .write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
        .and_then(|_| temporary.write_all(b"\n"))
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistence_validates_before_replacing_saved_choices() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("thinkhub/appearance.json");
        assert_eq!(load(&path).unwrap(), Appearance::default());
        assert!(!path.exists());
        let value = Appearance {
            theme: Theme::Light,
            accent: "#12abef".into(),
            ..Appearance::default()
        };
        save(&path, &value).unwrap();
        assert_eq!(load(&path).unwrap(), value);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(save(
            &path,
            &Appearance {
                accent: "red; background: url(x)".into(),
                ..value.clone()
            }
        )
        .is_err());
        assert_eq!(load(&path).unwrap(), value);
        fs::write(
            &path,
            r##"{"schema_version":2,"theme":"light","accent":"#ff0000"}"##,
        )
        .unwrap();
        assert!(load(&path).is_err());
        fs::write(&path, "x".repeat(20_000)).unwrap();
        assert!(load(&path).is_err());
    }
    #[test]
    fn readable_accents_in_both_themes() {
        for theme in [Theme::Dark, Theme::Light] {
            for color in PRESETS
                .iter()
                .map(|(_, c)| *c)
                .chain(["#ffffff", "#000000", "#ffff00", "#808080"])
            {
                let css = Appearance {
                    theme,
                    accent: color.into(),
                    ..Appearance::default()
                }
                .css();
                let get = |n: &str| {
                    rgb(css
                        .lines()
                        .find_map(|l| l.strip_prefix(&format!("@define-color {n} ")))
                        .unwrap()
                        .trim_end_matches(';'))
                    .unwrap()
                };
                assert!(
                    contrast(
                        get("accent_text"),
                        get(if theme == Theme::Dark {
                            "button"
                        } else {
                            "selected"
                        })
                    ) >= 4.5
                );
                assert!(contrast(get("accent"), get("accent_ink")) >= 4.5);
                assert!(contrast(get("accent_hover"), get("accent_ink")) >= 4.5);
            }
        }
    }
}
