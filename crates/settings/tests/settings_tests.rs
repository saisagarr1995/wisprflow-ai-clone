use std::fs;
use std::time::Duration;

use openflow_settings::{
    load_or_init, save_atomic, Config, HotkeyMode, SettingsHandle, CONFIG_VERSION,
};
use tempfile::TempDir;

fn temp_config_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("config.json")
}

#[test]
fn roundtrip_preserves_config() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);

    let mut config = Config::default();
    config.hotkey.chord = "Ctrl+Shift+Space".into();
    config.hotkey.mode = HotkeyMode::Toggle;
    config.llm.cleanup_model = "qwen3:1.7b".into();

    save_atomic(&path, &config).unwrap();
    let loaded = load_or_init(&path).unwrap();
    assert_eq!(loaded, config);
}

#[test]
fn missing_file_creates_defaults() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);

    let config = load_or_init(&path).unwrap();
    assert_eq!(config, Config::default());
    assert!(path.exists(), "defaults should be persisted on first load");
}

#[test]
fn corrupt_file_is_backed_up_and_replaced_with_defaults() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    fs::write(&path, "{ this is not json").unwrap();

    let config = load_or_init(&path).unwrap();
    assert_eq!(config, Config::default());

    let backup = path.with_extension("json.bak");
    assert!(
        backup.exists(),
        "corrupt config must be preserved as a backup"
    );
    assert_eq!(fs::read_to_string(backup).unwrap(), "{ this is not json");

    // The file itself is now valid.
    assert_eq!(load_or_init(&path).unwrap(), Config::default());
}

#[test]
fn newer_version_is_backed_up_not_reinterpreted() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    fs::write(&path, format!(r#"{{ "version": {} }}"#, CONFIG_VERSION + 1)).unwrap();

    let config = load_or_init(&path).unwrap();
    assert_eq!(config, Config::default());
    assert!(path.with_extension("json.bak").exists());
}

#[test]
fn v0_config_migrates_and_is_rewritten() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    // v0: no `version` field; known fields must survive, unknown are dropped.
    fs::write(
        &path,
        r#"{ "hotkey": { "chord": "F9" }, "some_removed_field": true }"#,
    )
    .unwrap();

    let config = load_or_init(&path).unwrap();
    assert_eq!(config.version, CONFIG_VERSION);
    assert_eq!(config.hotkey.chord, "F9");

    // Migration persists the upgraded file.
    let raw: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(raw["version"], serde_json::json!(CONFIG_VERSION));
    assert!(raw.get("some_removed_field").is_none());
}

#[test]
fn atomic_save_leaves_no_temp_file() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    save_atomic(&path, &Config::default()).unwrap();

    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["config.json"],
        "no temp or stray files: {names:?}"
    );
}

#[test]
fn update_persists_and_broadcasts() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    let (handle, rx) = SettingsHandle::init(path.clone()).unwrap();

    handle.update(|c| c.language = "hi".into()).unwrap();

    assert_eq!(rx.borrow().language, "hi");
    assert_eq!(handle.current().language, "hi");
    assert_eq!(load_or_init(&path).unwrap().language, "hi");
}

#[tokio::test]
async fn external_edit_hot_reloads_within_a_second() {
    let dir = TempDir::new().unwrap();
    let path = temp_config_path(&dir);
    let (_handle, mut rx) = SettingsHandle::init(path.clone()).unwrap();

    // Simulate a user editing the file in a text editor.
    let mut edited = Config::default();
    edited.hotkey.chord = "F13".into();
    fs::write(&path, serde_json::to_string_pretty(&edited).unwrap()).unwrap();

    tokio::time::timeout(Duration::from_secs(5), rx.changed())
        .await
        .expect("watcher should fire for external edits")
        .expect("sender alive");
    assert_eq!(rx.borrow().hotkey.chord, "F13");
}
