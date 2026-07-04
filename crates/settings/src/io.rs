use std::fs;
use std::path::{Path, PathBuf};

use crate::migrate;
use crate::schema::{Config, CONFIG_VERSION};

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("config io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config (de)serialization failed: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("config migration failed: {0}")]
    Migration(String),
}

/// `%APPDATA%/openflow` (Windows), `~/Library/Application Support/openflow`
/// (macOS), `~/.config/openflow` (Linux).
pub fn default_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openflow")
}

pub fn config_path() -> PathBuf {
    default_config_dir().join("config.json")
}

/// Load the config, creating it with defaults if missing.
///
/// A corrupt or unmigratable file is never silently destroyed: it is copied to
/// `config.json.bak` beside the original before defaults are written.
/// A successfully migrated config is persisted immediately in its new shape.
pub fn load_or_init(path: &Path) -> Result<Config, SettingsError> {
    match fs::read_to_string(path) {
        Ok(raw) => match parse_and_migrate(&raw) {
            Ok((config, was_migrated)) => {
                if was_migrated {
                    save_atomic(path, &config)?;
                }
                Ok(config)
            }
            Err(err) => {
                tracing::warn!(%err, ?path, "config unreadable; backing up and using defaults");
                let _ = fs::copy(path, path.with_extension("json.bak"));
                let config = Config::default();
                save_atomic(path, &config)?;
                Ok(config)
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let config = Config::default();
            save_atomic(path, &config)?;
            Ok(config)
        }
        Err(e) => Err(e.into()),
    }
}

/// Read-only load: parses and migrates in memory but never writes, backs up,
/// or "repairs" the file. The hot-reload watcher uses this because it can
/// observe partially-written files mid-edit; destroying the user's edit by
/// "fixing" a transient parse failure is the one unforgivable outcome.
pub fn try_load(path: &Path) -> Result<Config, SettingsError> {
    let raw = fs::read_to_string(path)?;
    parse_and_migrate(&raw).map(|(config, _)| config)
}

fn parse_and_migrate(raw: &str) -> Result<(Config, bool), SettingsError> {
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if version > CONFIG_VERSION {
        // Written by a newer build. Treat as unreadable (caller backs it up)
        // rather than lossily reinterpreting it.
        return Err(SettingsError::Migration(format!(
            "config version {version} is newer than supported {CONFIG_VERSION}"
        )));
    }
    let was_migrated = version < CONFIG_VERSION;
    let value = migrate::migrate(version, value).map_err(SettingsError::Migration)?;
    Ok((serde_json::from_value(value)?, was_migrated))
}

/// Write via a temp file in the same directory + rename, so a crash mid-write
/// never leaves a truncated config. `fs::rename` replaces the destination
/// atomically on both POSIX and Windows (MOVEFILE_REPLACE_EXISTING).
pub fn save_atomic(path: &Path, config: &Config) -> Result<(), SettingsError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(config)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
