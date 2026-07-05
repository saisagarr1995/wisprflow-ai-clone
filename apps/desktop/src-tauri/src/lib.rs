//! Tauri shell for OpenFlow: tray icon, windows, IPC, and pipeline wiring.
//! Application logic belongs in the `crates/*` workspace members.

mod commands;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};

use commands::AppState;
use openflow_core::{Callbacks, Orchestrator, Services, SessionEvent, Status};

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,ort=warn".into()),
        )
        .init();

    let (settings, config_rx) =
        openflow_settings::SettingsHandle::init(openflow_settings::config_path())
            .expect("settings must load or initialize");
    let store = Arc::new(
        openflow_store::Store::open(&openflow_store::default_db_path()).expect("store must open"),
    );
    let state = AppState {
        settings,
        store,
        level_slot: Arc::new(Mutex::new(None)),
        status: Arc::new(AtomicU8::new(0)),
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::list_devices,
            commands::get_level,
            commands::list_models,
            commands::download_model,
            commands::ollama_status,
            commands::dictionary_list,
            commands::dictionary_add,
            commands::dictionary_delete,
            commands::snippets_list,
            commands::snippets_add,
            commands::snippets_delete,
            commands::history_list,
            commands::history_delete,
            commands::history_clear,
            commands::commands_list,
            commands::commands_add,
            commands::commands_delete,
        ])
        .setup(move |app| {
            let show = MenuItem::with_id(app, "show", "Settings", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit OpenFlow", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().expect("bundled icon").clone())
                .tooltip("OpenFlow — starting…")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            // HUD overlay: transparent, click-through, never focusable — it
            // must not steal focus from the app the user is dictating into.
            // Small and always visible (the idle handle sits at the bottom,
            // Wispr Flow-style), expanding into a pill while recording.
            let hud =
                WebviewWindowBuilder::new(app, "hud", WebviewUrl::App("index.html#hud".into()))
                    .title("OpenFlow HUD")
                    .inner_size(380.0, 120.0)
                    .transparent(true)
                    .decorations(false)
                    .shadow(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .resizable(false)
                    .focused(false)
                    .visible(false)
                    .build()?;
            let _ = hud.set_ignore_cursor_events(true);
            position_hud_bottom_center(&hud);
            let _ = hud.show();

            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("openflow-bootstrap".into())
                .spawn(move || {
                    if let Err(e) = bootstrap_pipeline(handle.clone(), config_rx) {
                        tracing::error!(error = %e, "pipeline bootstrap failed");
                        set_tooltip(&handle, format!("OpenFlow — error: {e}"));
                    }
                })?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the settings window hides it; the tray keeps the app alive.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Builds every service and runs the orchestrator loop on this thread until
/// shutdown. Returns early (with the tray showing the error) on any failure.
fn bootstrap_pipeline(
    app: AppHandle,
    mut config_rx: tokio::sync::watch::Receiver<openflow_settings::Config>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let config = state.settings.current();
    let level_slot = state.level_slot.clone();
    let status_flag = state.status.clone();

    set_tooltip(&app, "OpenFlow — preparing models…".into());

    // Factories double as the initial constructors so rebuild == first build.
    let audio_factory: openflow_core::AudioFactory = {
        let level_slot = level_slot.clone();
        Box::new(move |device: Option<String>| {
            let engine =
                openflow_audio::AudioEngine::new(Box::new(openflow_audio::CpalSource::new(device)));
            *level_slot.lock().expect("level slot") = Some(engine.level_handle());
            engine
        })
    };
    let stt_factory: openflow_core::SttFactory = Box::new(|model_id: &str| {
        let path = openflow_models::ensure(model_id, |_| {}).map_err(|e| e.to_string())?;
        openflow_stt::WhisperEngine::from_file(&path)
            .map(|e| Box::new(e) as Box<dyn openflow_stt::SttEngine>)
            .map_err(|e| e.to_string())
    });

    let stt = stt_factory(&config.stt.model_id)?;
    let audio = audio_factory(config.audio.input_device.clone());

    let vad_path = openflow_models::ensure("silero-vad-v5", |_| {}).map_err(|e| e.to_string())?;
    let vad = match openflow_audio::SileroVad::from_file(&vad_path) {
        Ok(v) => Some(Box::new(v) as Box<dyn openflow_audio::Vad>),
        Err(e) => {
            tracing::warn!(error = %e, "vad unavailable; continuing without trim");
            None
        }
    };

    // Partials always use tiny (fast) regardless of the main model.
    let partials = match openflow_models::ensure("whisper-tiny", |_| {})
        .map_err(|e| e.to_string())
        .and_then(|p| openflow_stt::WhisperEngine::from_file(&p).map_err(|e| e.to_string()))
    {
        Ok(engine) => {
            let partial_app = app.clone();
            Some(openflow_core::PartialWorker::spawn(
                Box::new(engine),
                Box::new(move |text| {
                    let _ = partial_app.emit("partial", text);
                }),
                std::time::Duration::from_millis(700),
            ))
        }
        Err(e) => {
            tracing::warn!(error = %e, "partials unavailable");
            None
        }
    };

    let injector =
        openflow_inject::ClipboardPasteInjector::new(config.injection.clipboard_restore_delay_ms);

    // LLM cleanup via Ollama. The backend is cheap to construct; if Ollama is
    // down, every call falls back to the raw transcript (never blocks
    // dictation). Prewarm in the background so the first cleanup is fast.
    let cleanup = {
        let backend = openflow_llm::OllamaBackend::new(&config.llm.base_url);
        let service = openflow_llm::CleanupService::new(Box::new(backend));
        if config.llm.enabled {
            let warm_config = config.llm.clone();
            let warm_backend = openflow_llm::OllamaBackend::new(&config.llm.base_url);
            std::thread::Builder::new()
                .name("openflow-prewarm".into())
                .spawn(move || {
                    use openflow_llm::LlmBackend as _;
                    if warm_backend.reachable() {
                        warm_backend.prewarm(&warm_config.cleanup_model);
                    } else {
                        tracing::warn!("ollama unreachable; cleanup will fall back to raw text");
                    }
                })
                .map_err(|e| e.to_string())?;
        }
        Some(service)
    };
    let store = app.state::<AppState>().store.clone();

    let status_app = app.clone();
    let timing_app = app.clone();
    let (orchestrator, handle) = Orchestrator::new(
        Services {
            audio,
            stt,
            vad,
            injector: Box::new(injector),
            partials,
            stt_factory: Some(stt_factory),
            audio_factory: Some(audio_factory),
            cleanup,
            store: Some(store),
            context: Some(Box::new(openflow_context::WindowsContext)),
            terminal_injector: Some(Box::new(openflow_inject::KeystrokeInjector::new(true))),
            commands: if config.llm.enabled {
                Some(openflow_core::CommandRuntime {
                    backend: Box::new(openflow_llm::OllamaBackend::new(&config.llm.base_url)),
                    commands: merged_commands(&app.state::<AppState>().store),
                })
            } else {
                None
            },
        },
        config.clone(),
        Callbacks {
            on_status: Box::new(move |status| {
                status_flag.store(status_code(status), Ordering::Relaxed);
                let _ = status_app.emit("status", status_name(status));
                set_tooltip(&status_app, tooltip_for(status).into());
                update_hud(&status_app, status);
            }),
            on_timing: Some(Box::new(move |t| {
                let _ = timing_app.emit(
                    "timing",
                    serde_json::json!({
                        "audio_ms": t.audio_ms, "stt_ms": t.stt_ms, "llm_ms": t.llm_ms,
                        "inject_ms": t.inject_ms, "total_ms": t.total_ms, "chars": t.chars,
                    }),
                );
            })),
            on_assistant: Some({
                let app = app.clone();
                Box::new(move |answer| show_assistant_popover(&app, answer))
            }),
            on_flash: Some({
                let app = app.clone();
                Box::new(move |msg| {
                    let _ = app.emit("flash", msg);
                })
            }),
        },
    );

    // Level meter events while recording (HUD waveform + settings mic meter).
    {
        let app = app.clone();
        let state = app.state::<AppState>();
        let status = state.status.clone();
        let level_slot = state.level_slot.clone();
        std::thread::Builder::new()
            .name("openflow-level".into())
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(66));
                if status.load(Ordering::Relaxed) == 1 {
                    let level = level_slot
                        .lock()
                        .expect("level slot")
                        .as_ref()
                        .map(|b| f32::from_bits(b.load(Ordering::Relaxed)))
                        .unwrap_or(0.0);
                    let _ = app.emit("level", level);
                }
            })
            .map_err(|e| e.to_string())?;
    }

    // Hotkey hook → orchestrator bridge, rebuilt when the chord changes.
    let listener_slot: Arc<Mutex<Option<openflow_hotkey::HotkeyListener>>> =
        Arc::new(Mutex::new(None));
    start_hotkey(&config.hotkey.chord, handle.clone(), &listener_slot)?;

    // Config hot-reload → orchestrator + hotkey rebuild.
    {
        let config_handle = handle.clone();
        let listener_slot = listener_slot.clone();
        let mut last_chord = config.hotkey.chord.clone();
        tauri::async_runtime::spawn(async move {
            while config_rx.changed().await.is_ok() {
                let updated = config_rx.borrow().clone();
                if updated.hotkey.chord != last_chord {
                    match start_hotkey(&updated.hotkey.chord, config_handle.clone(), &listener_slot)
                    {
                        Ok(()) => last_chord = updated.hotkey.chord.clone(),
                        Err(e) => tracing::error!(error = %e, "hotkey rebind failed"),
                    }
                }
                config_handle.post(SessionEvent::ConfigChanged(Box::new(updated)));
            }
        });
    }

    tracing::info!("pipeline ready");
    orchestrator.run();
    Ok(())
}

fn start_hotkey(
    chord_str: &str,
    handle: openflow_core::OrchestratorHandle,
    slot: &Arc<Mutex<Option<openflow_hotkey::HotkeyListener>>>,
) -> Result<(), String> {
    let chord = openflow_hotkey::parse_chord(chord_str).map_err(|e| e.to_string())?;
    let (hk_tx, hk_rx) = mpsc::channel();
    let listener =
        openflow_hotkey::HotkeyListener::start(chord, hk_tx).map_err(|e| e.to_string())?;
    // Dropping the previous listener uninstalls its hook; its bridge thread
    // then ends because the channel sender is gone.
    *slot.lock().expect("listener slot") = Some(listener);
    std::thread::Builder::new()
        .name("openflow-hotkey-bridge".into())
        .spawn(move || {
            for event in hk_rx {
                handle.post(match event {
                    openflow_hotkey::HotkeyEvent::Pressed => SessionEvent::HotkeyPressed,
                    openflow_hotkey::HotkeyEvent::Released => SessionEvent::HotkeyReleased,
                });
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn status_code(s: Status) -> u8 {
    match s {
        Status::Idle => 0,
        Status::Recording => 1,
        Status::Processing => 2,
    }
}

fn status_name(s: Status) -> &'static str {
    match s {
        Status::Idle => "idle",
        Status::Recording => "recording",
        Status::Processing => "processing",
    }
}

fn tooltip_for(s: Status) -> &'static str {
    match s {
        Status::Idle => "OpenFlow — ready (hold your hotkey to dictate)",
        Status::Recording => "OpenFlow — recording…",
        Status::Processing => "OpenFlow — transcribing…",
    }
}

fn update_hud(app: &AppHandle, status: Status) {
    // The HUD stays visible always (idle handle → pill); the React side reacts
    // to the "status" event. We only re-assert position on activity in case
    // the display configuration changed.
    if matches!(status, Status::Recording) {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(hud) = app2.get_webview_window("hud") {
                position_hud_bottom_center(&hud);
                let _ = hud.show();
            }
        });
    }
}

fn position_hud_bottom_center(hud: &tauri::WebviewWindow) {
    let (Ok(Some(monitor)), Ok(size)) = (hud.primary_monitor(), hud.outer_size()) else {
        return;
    };
    let mpos = monitor.position();
    let msize = monitor.size();
    let scale = monitor.scale_factor();
    let x = mpos.x + (msize.width as i32 - size.width as i32) / 2;
    // Sit just above the taskbar / screen edge.
    let gap = (12.0 * scale) as i32;
    let y = mpos.y + msize.height as i32 - size.height as i32 - gap;
    let _ = hud.set_position(tauri::PhysicalPosition { x, y });
}

/// Built-in commands plus any the user defined in the store.
fn merged_commands(store: &openflow_store::Store) -> Vec<openflow_llm::Command> {
    let mut commands = openflow_llm::builtin_commands();
    if let Ok(custom) = store.commands_list() {
        for c in custom {
            let output = match c.output_mode.as_str() {
                "popover" => openflow_llm::OutputMode::Popover,
                "insert" => openflow_llm::OutputMode::Insert,
                _ => openflow_llm::OutputMode::Replace,
            };
            commands.push(openflow_llm::Command {
                name: c.name,
                triggers: c
                    .triggers
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect(),
                prompt_template: c.prompt,
                task: openflow_llm::TaskKind::Rewrite,
                output,
            });
        }
    }
    commands
}

/// Show the assistant's answer in a dedicated popover window (design doc §10:
/// answers are never injected into the user's app).
fn show_assistant_popover(app: &AppHandle, answer: String) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = app2.emit("assistant", &answer);
        if let Some(win) = app2.get_webview_window("assistant") {
            let _ = win.show();
            let _ = win.set_focus();
            return;
        }
        if let Ok(win) = WebviewWindowBuilder::new(
            &app2,
            "assistant",
            WebviewUrl::App("index.html#assistant".into()),
        )
        .title("OpenFlow — Assistant")
        .inner_size(460.0, 340.0)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()
        {
            // Re-emit once the window has had a moment to subscribe.
            let win_app = app2.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let _ = win_app.emit("assistant", &answer);
            });
            let _ = win.show();
        }
    });
}

fn set_tooltip(app: &AppHandle, tip: String) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = app2.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(&tip));
        }
    });
}
