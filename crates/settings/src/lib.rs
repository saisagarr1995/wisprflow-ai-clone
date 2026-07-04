//! Typed configuration: schema, defaults, atomic persistence, versioned
//! migrations, and hot reload.
//!
//! The rest of the app never touches the config file. `SettingsHandle::init`
//! loads (or creates) it and returns a `tokio::sync::watch::Receiver<Config>`;
//! consumers read the latest snapshot from that channel and react to changes.
//! Writes go through `SettingsHandle::update`; external edits to the file are
//! picked up by a filesystem watcher and broadcast on the same channel.

mod io;
mod migrate;
mod schema;
mod watch;

pub use io::{config_path, default_config_dir, load_or_init, save_atomic, try_load, SettingsError};
pub use schema::*;
pub use watch::SettingsHandle;
