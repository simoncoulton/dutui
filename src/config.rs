use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The name of a theme in `~/.config/dutui/themes/` (a file name without `.yml`).
    pub theme: String,
    /// Use the terminal's own background instead of the theme background.
    pub transparent: bool,
    /// Descend into other mounted filesystems while scanning.
    pub cross_filesystems: bool,
    /// `si` (kB, MB, GB; 1000-based, matches Finder and dua) or `iec` (KiB, MiB, GiB; 1024-based).
    pub units: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "tokyo-night".into(),
            transparent: false,
            cross_filesystems: false,
            units: "si".into(),
        }
    }
}

/// Written on first run. Edits go through [`Config::save`], which keeps these comments.
pub const DEFAULT_CONFIG: &str = "\
# dutui configuration. Press E inside dutui to edit this file.

# The name of a file in ~/.config/dutui/themes/ (without .yml): tokyo-night,
# catppuccin-mocha, dracula, nord, ... Drop in your own .yml file to add a theme.
theme: tokyo-night

# true: use the terminal's own background instead of the theme's.
transparent: false

# true: descend into other mounted drives and volumes while scanning.
cross_filesystems: false

# si: kB / MB / GB (1000-based, like Finder and dua). iec: KiB / MiB / GiB (1024-based).
units: si
";

pub fn config_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/dutui"))
}

pub fn config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("dutui.yml"))
}

pub fn themes_dir() -> Option<PathBuf> {
    Some(config_dir()?.join("themes"))
}

impl Config {
    /// Parse and validate config text. Errors carry the line and column serde reports.
    /// The theme name is checked against the loaded themes, not here.
    pub fn parse(text: &str) -> Result<Self, String> {
        let cfg: Self = serde_yaml_ng::from_str(text).map_err(|e| e.to_string())?;
        if !matches!(cfg.units.to_ascii_lowercase().as_str(), "si" | "iec") {
            return Err(format!("units must be 'si' or 'iec', not '{}'", cfg.units));
        }
        Ok(cfg)
    }

    /// Load the config, writing the commented default if none exists.
    /// A broken file is an error naming the file; it is never replaced by defaults.
    pub fn load() -> Result<Self, String> {
        let Some(path) = config_path() else {
            return Ok(Self::default());
        };
        match fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(dir) = path.parent() {
                    let _ = fs::create_dir_all(dir);
                }
                let _ = fs::write(&path, DEFAULT_CONFIG);
                Ok(Self::default())
            }
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Write our values into the file, replacing only the `key: value` lines so the
    /// user's comments and ordering survive. Refuses to touch a file that is already
    /// broken, and never writes something that would not parse back identically.
    pub fn save(&self) -> Result<(), String> {
        let Some(path) = config_path() else {
            return Ok(());
        };
        let old = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => DEFAULT_CONFIG.to_string(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        Self::parse(&old).map_err(|e| format!("{} is invalid, not saving: {e}", path.display()))?;
        let new = self.apply_to(&old);
        if Self::parse(&new).as_ref() != Ok(self) {
            return Err("refusing to write a config that does not round-trip".into());
        }
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if new != old || !path.exists() {
            fs::write(&path, new).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(())
    }

    fn apply_to(&self, text: &str) -> String {
        let values = [
            ("theme", self.theme.clone()),
            ("transparent", self.transparent.to_string()),
            ("cross_filesystems", self.cross_filesystems.to_string()),
            ("units", self.units.clone()),
        ];
        let mut out = text.to_string();
        for (key, value) in values {
            out = set_value(&out, key, &value);
        }
        out
    }
}

/// Replace the top-level `key:` line (dropping any trailing comment on it), or
/// append one if the key is absent.
fn set_value(text: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key}:");
    let mut found = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| {
            if !found && l.starts_with(&prefix) {
                found = true;
                format!("{key}: {value}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(format!("{key}: {value}"));
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_template_equals_defaults() {
        assert_eq!(Config::parse(DEFAULT_CONFIG).unwrap(), Config::default());
    }

    #[test]
    fn unknown_keys_and_bad_values_are_errors() {
        assert!(Config::parse("colour: red\n").is_err());
        assert!(Config::parse("units: furlongs\n").is_err());
        assert!(Config::parse("theme: [\n").is_err());
    }

    #[test]
    fn edits_keep_comments_and_other_lines() {
        let cfg = Config {
            theme: "dracula".into(),
            units: "iec".into(),
            ..Config::default()
        };
        let out = cfg.apply_to(DEFAULT_CONFIG);
        assert!(out.contains("theme: dracula\n"));
        assert!(out.contains("units: iec\n"));
        assert!(out.contains("# true: descend into other mounted drives"));
        assert_eq!(Config::parse(&out).unwrap(), cfg);
    }

    #[test]
    fn missing_keys_are_appended() {
        let cfg = Config {
            transparent: true,
            ..Config::default()
        };
        let out = cfg.apply_to("theme: tokyo-night\n");
        assert_eq!(Config::parse(&out).unwrap(), cfg);
    }
}
