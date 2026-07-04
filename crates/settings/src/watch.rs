use std::path::PathBuf;
use std::sync::Mutex;

use notify::{RecursiveMode, Watcher};
use tokio::sync::watch;

use crate::io::{load_or_init, save_atomic, SettingsError};
use crate::schema::Config;

/// Owns the config file. Consumers hold the `watch::Receiver<Config>` returned
/// by [`SettingsHandle::init`] and never read the file themselves.
pub struct SettingsHandle {
    path: PathBuf,
    tx: watch::Sender<Config>,
    /// Serializes `update` calls so read-modify-write cycles don't interleave.
    write_lock: Mutex<()>,
    /// Kept alive for the lifetime of the handle; dropping it stops watching.
    _watcher: Option<notify::RecommendedWatcher>,
}

impl SettingsHandle {
    /// Load (or create) the config at `path` and start watching it for
    /// external edits. Failure to start the watcher is downgraded to a log
    /// line — hot reload is a convenience, not a correctness requirement.
    pub fn init(path: PathBuf) -> Result<(Self, watch::Receiver<Config>), SettingsError> {
        let config = load_or_init(&path)?;
        let (tx, rx) = watch::channel(config);
        let watcher = Self::start_watcher(&path, tx.clone());
        Ok((
            Self {
                path,
                tx,
                write_lock: Mutex::new(()),
                _watcher: watcher,
            },
            rx,
        ))
    }

    /// Read-modify-write: applies `mutate` to the current config, persists
    /// atomically, and broadcasts if anything actually changed.
    pub fn update(&self, mutate: impl FnOnce(&mut Config)) -> Result<(), SettingsError> {
        let _guard = self
            .write_lock
            .lock()
            .expect("settings write lock poisoned");
        let mut config = self.tx.borrow().clone();
        mutate(&mut config);
        save_atomic(&self.path, &config)?;
        broadcast_if_changed(&self.tx, config);
        Ok(())
    }

    /// Current snapshot, for callers outside the watch-channel flow.
    pub fn current(&self) -> Config {
        self.tx.borrow().clone()
    }

    fn start_watcher(
        path: &std::path::Path,
        tx: watch::Sender<Config>,
    ) -> Option<notify::RecommendedWatcher> {
        // Watch the parent directory, not the file: atomic saves replace the
        // file by rename, which breaks direct file watches on some platforms.
        let dir = path.parent()?.to_path_buf();
        let file_name = path.file_name()?.to_os_string();
        let path = path.to_path_buf();

        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                let Ok(event) = res else { return };
                let touches_config = event
                    .paths
                    .iter()
                    .any(|p| p.file_name() == Some(file_name.as_os_str()));
                if !touches_config {
                    return;
                }
                // Reload and broadcast. Our own atomic saves also land here;
                // `broadcast_if_changed` makes that a no-op. Editors that emit
                // several events per save just cause a couple of cheap reloads.
                match load_or_init(&path) {
                    Ok(config) => broadcast_if_changed(&tx, config),
                    Err(err) => tracing::warn!(%err, "config hot-reload failed"),
                }
            })
            .map_err(|err| tracing::warn!(%err, "config watcher unavailable"))
            .ok()?;

        watcher
            .watch(&dir, RecursiveMode::NonRecursive)
            .map_err(|err| tracing::warn!(%err, "config watcher unavailable"))
            .ok()?;
        Some(watcher)
    }
}

fn broadcast_if_changed(tx: &watch::Sender<Config>, new: Config) {
    tx.send_if_modified(|current| {
        if *current == new {
            false
        } else {
            *current = new;
            true
        }
    });
}
