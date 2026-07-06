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

/// Set up logging to a daily rolling file in `<data_dir>/openflow/logs` plus
/// stdout. Returns the non-blocking writer guard, which must be kept alive for
/// the process lifetime (dropping it flushes and stops the writer thread).
fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,ort=warn".into());
    let stdout_layer = tracing_subscriber::fmt::layer();

    let log_dir = openflow_store::default_db_path()
        .parent()
        .map(|d| d.join("logs"));
    let (file_layer, guard) = match log_dir {
        Some(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            let appender = tracing_appender::rolling::daily(&dir, "openflow.log");
            let (nb, guard) = tracing_appender::non_blocking(appender);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(nb);
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();
    guard
}

pub fn run() {
    // Log to a rolling file under the data dir *and* stdout. The file matters
    // for standalone launches (GUI subsystem = no console) and for user support
    // ("copy debug bundle"); stdout is for `tauri dev`.
    let _log_guard = init_logging();

    // Panic hook that writes synchronously to crash.log — the async file logger
    // can lose buffered lines when the process dies, so panics need their own
    // durable path.
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let msg = format!("PANIC: {info}\n{bt}");
        tracing::error!("{msg}");
        if let Some(dir) = openflow_store::default_db_path().parent() {
            let _ = std::fs::write(dir.join("crash.log"), &msg);
        }
    }));

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
            commands::ui_log,
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
            // Critical: the HUD must never take foreground focus. If it did, the
            // orchestrator would see the focused app change to OpenFlow and
            // abort injection (design doc §7). WS_EX_NOACTIVATE keeps it from
            // ever becoming the active window, even when shown.
            make_non_activating(&hud);
            position_hud_bottom_center(&hud);
            let _ = hud.show();

            // The main window starts hidden (tray app). Show it once on first
            // run so a new user discovers the app; afterwards it lives quietly
            // in the tray and is opened from the tray menu.
            let marker = openflow_store::default_db_path()
                .parent()
                .map(|d| d.join(".shown"));
            let first_run = marker.as_ref().map(|m| !m.exists()).unwrap_or(false);
            if first_run {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
                if let Some(m) = &marker {
                    let _ = std::fs::write(m, "1");
                }
            }

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

/// Apply `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` so the window can never steal
/// foreground focus (and stays out of Alt-Tab). Used for the HUD overlay.
#[cfg(windows)]
fn make_non_activating(win: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    if let Ok(raw) = win.hwnd() {
        let hwnd = HWND(raw.0);
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ex | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize,
            );
        }
    }
}

#[cfg(not(windows))]
fn make_non_activating(_win: &tauri::WebviewWindow) {}

fn update_hud(app: &AppHandle, status: Status) {
    // The HUD stays visible always (idle handle → pill); the React side reacts
    // to the "status" event. Crucially we do NOT call show()/set_focus() here:
    // re-showing during recording would risk stealing foreground focus and
    // aborting injection. We only re-assert position (SetWindowPos does not
    // activate) in case the display configuration changed.
    if matches!(status, Status::Recording) {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(hud) = app2.get_webview_window("hud") {
                position_hud_bottom_center(&hud);
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
    // Anchor to the bottom of the *work area* (above the taskbar) so the pill
    // is never hidden behind it. Falls back to the full monitor height.
    let bottom = work_area_bottom().unwrap_or(mpos.y + msize.height as i32);
    let gap = (10.0 * scale) as i32;
    let y = bottom - size.height as i32 - gap;
    let _ = hud.set_position(tauri::PhysicalPosition { x, y });
}

/// Bottom edge (physical px) of the primary monitor's work area — the screen
/// minus the taskbar. Used to keep the HUD above the taskbar.
#[cfg(windows)]
fn work_area_bottom() -> Option<i32> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut rect = RECT::default();
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rect as *mut RECT as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_ok().then_some(rect.bottom)
}

#[cfg(not(windows))]
fn work_area_bottom() -> Option<i32> {
    None
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
