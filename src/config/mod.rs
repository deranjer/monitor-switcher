pub mod schema;

pub use schema::{Config, HotkeyBinding, InputSourceValue, ModifierKey, MonitorKey, Profile};

use std::fs;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not determine config directory")]
    NoConfigDir,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] serde_json::Error),
}

fn config_dir() -> Result<PathBuf, ConfigError> {
    let dirs = directories::ProjectDirs::from("", "", "monitor-switcher")
        .ok_or(ConfigError::NoConfigDir)?;
    Ok(dirs.config_dir().to_path_buf())
}

/// Loads the config from disk, creating a default one if missing.
///
/// If the existing file fails to parse, it is renamed to a `.bak-<timestamp>` file
/// rather than overwritten or discarded, and a fresh default config is returned - a
/// schema hiccup should never silently destroy the user's saved profiles/hotkeys.
pub fn load() -> Result<Config, ConfigError> {
    let dir = config_dir()?;
    let path = dir.join("config.json");

    if !path.exists() {
        let cfg = Config::default();
        save(&cfg)?;
        return Ok(cfg);
    }

    let raw = fs::read_to_string(&path)?;
    match serde_json::from_str::<Config>(&raw) {
        Ok(cfg) => Ok(migrate(cfg)),
        Err(e) => {
            tracing::warn!("config.json failed to parse ({e}); backing up and resetting to default");
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup_path = dir.join(format!("config.json.bak-{timestamp}"));
            fs::rename(&path, &backup_path)?;
            let cfg = Config::default();
            save(&cfg)?;
            Ok(cfg)
        }
    }
}

fn migrate(cfg: Config) -> Config {
    match cfg.version {
        schema::CURRENT_CONFIG_VERSION => cfg,
        _ => cfg, // no prior versions to migrate from yet
    }
}

/// Atomically writes the config: write to a temp file, then rename over the real path
/// (rename is atomic on the same NTFS volume), so a crash mid-write never corrupts the
/// on-disk config.
pub fn save(cfg: &Config) -> Result<(), ConfigError> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.json");
    let tmp_path = dir.join("config.json.tmp");

    let json = serde_json::to_string_pretty(cfg)?;
    fs::write(&tmp_path, json)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}
