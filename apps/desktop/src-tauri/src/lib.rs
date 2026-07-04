//! Tauri shell for OpenFlow: tray icon, windows, and pipeline wiring only.
//! Application logic belongs in the `crates/*` workspace members.

use std::sync::mpsc;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

use openflow_core::{Orchestrator, SessionEvent, Status};

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
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

            // The entire dictation pipeline boots off the main thread; the UI
            // must be responsive even while models download/load.
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("openflow-bootstrap".into())
                .spawn(move || {
                    if let Err(e) = bootstrap_pipeline(handle.clone()) {
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
fn bootstrap_pipeline(app: AppHandle) -> Result<(), String> {
    let (settings, mut config_rx) =
        openflow_settings::SettingsHandle::init(openflow_settings::config_path())
            .map_err(|e| e.to_string())?;
    let config = settings.current();
    app.manage(std::sync::Mutex::new(settings));

    set_tooltip(&app, "OpenFlow — preparing models…".into());
    let stt_path = openflow_models::ensure(&config.stt.model_id, |p| {
        tracing::debug!(done = p.downloaded, total = p.total, "stt model download");
    })
    .map_err(|e| e.to_string())?;
    let vad_path = openflow_models::ensure("silero-vad-v5", |_| {}).map_err(|e| e.to_string())?;

    use openflow_stt::SttEngine as _;
    let stt = openflow_stt::WhisperEngine::from_file(&stt_path).map_err(|e| e.to_string())?;
    tracing::info!(backend = stt.describe(), "stt ready");
    let vad = match openflow_audio::SileroVad::from_file(&vad_path) {
        Ok(v) => Some(Box::new(v) as Box<dyn openflow_audio::Vad>),
        Err(e) => {
            tracing::warn!(error = %e, "vad unavailable; continuing without trim");
            None
        }
    };
    let audio = openflow_audio::AudioEngine::new(Box::new(openflow_audio::CpalSource::new(
        config.audio.input_device.clone(),
    )));
    let injector =
        openflow_inject::ClipboardPasteInjector::new(config.injection.clipboard_restore_delay_ms);

    let status_app = app.clone();
    let (orchestrator, handle) = Orchestrator::new(
        audio,
        Box::new(stt),
        vad,
        Box::new(injector),
        config.clone(),
        Box::new(move |status| {
            let tip = match status {
                Status::Idle => "OpenFlow — ready (hold Right Ctrl to dictate)",
                Status::Recording => "OpenFlow — recording…",
                Status::Processing => "OpenFlow — transcribing…",
            };
            set_tooltip(&status_app, tip.into());
        }),
    );

    // Hotkey hook → orchestrator bridge.
    let chord = openflow_hotkey::parse_chord(&config.hotkey.chord).map_err(|e| e.to_string())?;
    let (hk_tx, hk_rx) = mpsc::channel();
    let listener =
        openflow_hotkey::HotkeyListener::start(chord, hk_tx).map_err(|e| e.to_string())?;
    let bridge = handle.clone();
    std::thread::Builder::new()
        .name("openflow-hotkey-bridge".into())
        .spawn(move || {
            let _hook_lifetime = listener;
            for event in hk_rx {
                bridge.post(match event {
                    openflow_hotkey::HotkeyEvent::Pressed => SessionEvent::HotkeyPressed,
                    openflow_hotkey::HotkeyEvent::Released => SessionEvent::HotkeyReleased,
                });
            }
        })
        .map_err(|e| e.to_string())?;

    // Config hot-reload → orchestrator.
    let config_handle = handle.clone();
    tauri::async_runtime::spawn(async move {
        while config_rx.changed().await.is_ok() {
            let updated = config_rx.borrow().clone();
            config_handle.post(SessionEvent::ConfigChanged(Box::new(updated)));
        }
    });

    tracing::info!("pipeline ready");
    orchestrator.run();
    Ok(())
}

fn set_tooltip(app: &AppHandle, tip: String) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = app2.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(&tip));
        }
    });
}
