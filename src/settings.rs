//! Remembered form values. Passwords are never stored.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub backend: String,
    pub config_path: String,
    pub sync_output: String,
    pub sync_previous: String,
    pub rustup_snapshot: String,
    pub rustup_version: String,
    pub rustup_output: String,
    pub rustup_promote: String,
    pub publish_source: String,
    pub publish_url: String,
    pub publish_ca: String,
    pub publish_user: String,
}

/// `<config dir>/rust-mirror-gui/settings.json`, or `None` if no home is known.
pub fn path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }?;
    Some(base.join("rust-mirror-gui").join("settings.json"))
}

pub fn load() -> Settings {
    let mut s: Settings = path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if s.backend.is_empty() {
        s.backend = "rust-mirror".into();
    }
    if s.rustup_version.is_empty() {
        s.rustup_version = "1.28.2".into();
    }
    s
}

/// Best effort; a failure only means values are not remembered.
pub fn save(settings: &Settings) -> Result<(), String> {
    let path = path().ok_or("no configuration directory")?;
    let dir = path.parent().ok_or("bad settings path")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}
