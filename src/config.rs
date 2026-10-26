use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    /// One of: tokyo-night, catppuccin, dracula, one-dark
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

pub fn config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/dutui/dutui.yml"))
}

impl Config {
    /// Load the config, writing a default file if none exists.
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(s) => serde_yaml_ng::from_str(&s).unwrap_or_default(),
            Err(_) => {
                let cfg = Self::default();
                cfg.save();
                cfg
            }
        }
    }

    pub fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_yaml_ng::to_string(self) {
            let _ = fs::write(path, s);
        }
    }
}
