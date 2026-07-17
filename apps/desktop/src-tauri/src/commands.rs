//! Tauri IPC surface. Thin: validate, delegate to crates, serialize.

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use openflow_core::SessionEvent;
use openflow_settings::{Config, SettingsHandle};
use openflow_store::{CustomCommand, DictEntry, HistoryEntry, Snippet, Store};

/// A clickable region of the HUD, in logical pixels relative to the HUD
/// window's top-left. The cursor-poll thread in `lib.rs` makes the window
/// click-through everywhere outside these.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HudZone {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Shared state managed by Tauri. `level_slot` is filled by the pipeline
/// bootstrap (and refreshed when the audio engine is rebuilt).
pub struct AppState {
    pub settings: SettingsHandle,
    pub store: Arc<Store>,
    pub level_slot: Arc<Mutex<Option<Arc<AtomicU32>>>>,
    /// 0 = idle, 1 = recording, 2 = processing (mirror of core::Status).
    pub status: Arc<AtomicU8>,
    /// Filled by the pipeline bootstrap; lets the HUD post session events.
    pub orchestrator: Arc<Mutex<Option<openflow_core::OrchestratorHandle>>>,
    /// Interactive regions reported by the HUD frontend for the current state.
    pub hud_zones: Arc<Mutex<Vec<HudZone>>>,
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Config {
    // First call = the settings UI mounted and reached the backend. Logged so
    // standalone runs can be verified without a visible screen.
    tracing::info!("ui: get_config (frontend mounted)");
    state.settings.current()
}

/// Lets any window report to the log file (used to trace HUD lifecycle on
/// headless/standalone runs where the webview console isn't visible).
#[tauri::command]
pub fn ui_log(msg: String) {
    tracing::info!("ui: {msg}");
}

#[tauri::command]
pub fn set_config(state: State<AppState>, config: Config) -> Result<(), String> {
    state
        .settings
        .update(|c| *c = config)
        .map_err(|e| e.to_string())
}

fn post_session_event(state: &State<AppState>, event: SessionEvent) {
    if let Some(handle) = state
        .orchestrator
        .lock()
        .expect("orchestrator slot")
        .as_ref()
    {
        handle.post(event);
    }
}

/// HUD pill clicked while idle: start dictating.
#[tauri::command]
pub fn hud_start(state: State<AppState>) {
    tracing::debug!("ui: hud start clicked");
    post_session_event(&state, SessionEvent::HudStart);
}

/// HUD stop button: finish the session and paste.
#[tauri::command]
pub fn hud_finish(state: State<AppState>) {
    tracing::debug!("ui: hud finish clicked");
    post_session_event(&state, SessionEvent::HudFinish);
}

/// HUD cancel button: discard the session.
#[tauri::command]
pub fn hud_cancel(state: State<AppState>) {
    tracing::debug!("ui: hud cancel clicked");
    post_session_event(&state, SessionEvent::HudCancel);
}

/// The HUD reports its clickable regions whenever its layout changes; the
/// cursor-poll thread uses them for selective click-through.
#[tauri::command]
pub fn hud_zones(state: State<AppState>, zones: Vec<HudZone>) {
    *state.hud_zones.lock().expect("hud zones") = zones;
}

#[tauri::command]
pub fn list_devices() -> Vec<String> {
    openflow_audio::list_input_devices()
}

#[tauri::command]
pub fn get_level(state: State<AppState>) -> f32 {
    state
        .level_slot
        .lock()
        .expect("level slot lock")
        .as_ref()
        .map(|bits| f32::from_bits(bits.load(Ordering::Relaxed)))
        .unwrap_or(0.0)
}

#[derive(Serialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub kind: String,
    pub size_bytes: u64,
    pub languages: String,
    pub installed: bool,
}

#[tauri::command]
pub fn list_models() -> Vec<ModelInfo> {
    let dir = openflow_models::models_dir();
    openflow_models::registry()
        .iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            kind: format!("{:?}", m.kind).to_lowercase(),
            size_bytes: m.size_bytes,
            languages: m.languages.clone(),
            installed: dir.join(&m.file).exists(),
        })
        .collect()
}

#[derive(Serialize, Clone)]
pub struct ModelProgress {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
    pub done: bool,
    pub error: Option<String>,
}

/// Kicks off a background download; progress arrives as `model-progress`
/// events so the UI stays responsive.
#[tauri::command]
pub fn download_model(app: AppHandle, id: String) -> Result<(), String> {
    std::thread::Builder::new()
        .name("openflow-model-dl".into())
        .spawn(move || {
            let emit = |p: ModelProgress| {
                let _ = app.emit("model-progress", p);
            };
            let id2 = id.clone();
            let result = openflow_models::ensure(&id, |p| {
                emit(ModelProgress {
                    id: id2.clone(),
                    downloaded: p.downloaded,
                    total: p.total,
                    done: false,
                    error: None,
                });
            });
            match result {
                Ok(_) => emit(ModelProgress {
                    id,
                    downloaded: 0,
                    total: 0,
                    done: true,
                    error: None,
                }),
                Err(e) => emit(ModelProgress {
                    id,
                    downloaded: 0,
                    total: 0,
                    done: true,
                    error: Some(e.to_string()),
                }),
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---- dictionary / snippets / history ----

#[tauri::command]
pub fn dictionary_list(state: State<AppState>) -> Result<Vec<DictEntry>, String> {
    state.store.dictionary_list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn dictionary_add(
    state: State<AppState>,
    word: String,
    hint: Option<String>,
) -> Result<(), String> {
    state
        .store
        .dictionary_add(&word, hint.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn dictionary_delete(state: State<AppState>, id: i64) -> Result<(), String> {
    state.store.dictionary_delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn snippets_list(state: State<AppState>) -> Result<Vec<Snippet>, String> {
    state.store.snippets_list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn snippets_add(
    state: State<AppState>,
    trigger: String,
    expansion: String,
) -> Result<(), String> {
    state
        .store
        .snippets_add(&trigger, &expansion)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn snippets_delete(state: State<AppState>, id: i64) -> Result<(), String> {
    state.store.snippets_delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn history_list(
    state: State<AppState>,
    search: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<HistoryEntry>, String> {
    state
        .store
        .history_list(
            search.as_deref().filter(|s| !s.is_empty()),
            limit.unwrap_or(100),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn history_delete(state: State<AppState>, id: i64) -> Result<(), String> {
    state.store.history_delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn history_clear(state: State<AppState>) -> Result<(), String> {
    state.store.history_clear().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn commands_list(state: State<AppState>) -> Result<Vec<CustomCommand>, String> {
    state.store.commands_list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn commands_add(
    state: State<AppState>,
    name: String,
    triggers: String,
    prompt: String,
    output_mode: String,
) -> Result<(), String> {
    state
        .store
        .commands_add(&name, &triggers, &prompt, &output_mode)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn commands_delete(state: State<AppState>, id: i64) -> Result<(), String> {
    state.store.commands_delete(id).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct OllamaStatusDto {
    pub reachable: bool,
    pub available: Vec<String>,
    pub missing: Vec<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn ollama_status(state: State<AppState>) -> OllamaStatusDto {
    let config = state.settings.current();
    let mut required: Vec<&str> = vec![&config.llm.cleanup_model, &config.llm.rewrite_model];
    if let Some(coding) = config.llm.coding_model.as_deref() {
        required.push(coding);
    }
    match openflow_models::check_ollama(&config.llm.base_url, &required) {
        Ok(s) => OllamaStatusDto {
            reachable: true,
            available: s.available,
            missing: s.missing,
            error: None,
        },
        Err(e) => OllamaStatusDto {
            reachable: false,
            available: vec![],
            missing: required.iter().map(|s| s.to_string()).collect(),
            error: Some(e.to_string()),
        },
    }
}
