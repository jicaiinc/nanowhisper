mod commands;
mod history;
pub mod paste;
mod permissions;
mod recorder;
mod settings;
mod transcribe;

use history::HistoryManager;
use recorder::{encode_wav, AudioRecorder};
use settings::AppSettings;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// ~/.nanowhisper/
pub fn data_dir() -> PathBuf {
    let home = dirs::home_dir().expect("Cannot determine home directory");
    home.join(".nanowhisper")
}

// Named constants
const OVERLAY_WIDTH: f64 = 320.0;
const OVERLAY_HEIGHT: f64 = 48.0;
const OVERLAY_BOTTOM_OFFSET: f64 = 80.0;
const PASTE_DELAY_MS: u64 = 350;

pub fn run() {
    // Load .env file if present (for development)
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::get_history,
            commands::delete_history_entry,
            commands::clear_history,
            commands::get_settings,
            commands::save_settings,
            commands::check_accessibility,
            commands::request_accessibility,
            commands::check_microphone,
            commands::request_microphone,
            commands::initialize_enigo,
            commands::retry_transcription,
            commands::save_overlay_position,
            commands::pause_shortcut,
            commands::resume_shortcut,
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Initialize history manager
            let history_manager =
                Arc::new(HistoryManager::new().expect("Failed to init history DB"));
            app.manage(history_manager.clone());

            // Initialize audio recorder
            let recorder = Arc::new(AudioRecorder::new());
            app.manage(recorder.clone());

            // Initialize shared HTTP client
            let http_client = reqwest::Client::new();
            app.manage(http_client);

            // Initialize enigo if accessibility is already granted
            if paste::is_accessibility_trusted() {
                if let Ok(enigo_state) = paste::EnigoState::new() {
                    app.manage(enigo_state);
                }
            }

            // Create main window
            let _main_window =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("/".into()))
                    .title("NanoWhisper")
                    .inner_size(420.0, 600.0)
                    .min_inner_size(380.0, 400.0)
                    .resizable(true)
                    .maximizable(false)
                    .visible(false)
                    .build()?;

            // System tray
            let show_i = tauri::menu::MenuItem::with_id(
                app,
                "show",
                "Show NanoWhisper",
                true,
                None::<&str>,
            )?;
            let quit_i = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show_i, &separator, &quit_i])?;

            tauri::tray::TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(true)
                .icon_as_template(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Register global shortcut
            let settings = settings::get_settings();
            register_shortcut(&app_handle, &settings);

            // Show main window
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
            }

            log::info!("App started. Shortcut: {}", settings.shortcut);
            log::info!("API key configured: {}", !settings.api_key.is_empty());

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (&app, &event);
            }
        });
}

static SHORTCUT_PROCESSING: AtomicBool = AtomicBool::new(false);
static LAST_SHORTCUT_TIME: AtomicU64 = AtomicU64::new(0);
const DEBOUNCE_MS: u64 = 500;

pub fn register_shortcut(app_handle: &tauri::AppHandle, settings: &AppSettings) {
    let shortcut_str = &settings.shortcut;
    let shortcut: Shortcut = match shortcut_str.parse() {
        Ok(s) => s,
        Err(e) => {
            log::error!("Invalid shortcut '{}': {}", shortcut_str, e);
            return;
        }
    };

    let handle = app_handle.clone();
    let _ = app_handle
        .global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                // Debounce: ignore duplicate events within 500ms
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                let last = LAST_SHORTCUT_TIME.load(Ordering::SeqCst);
                if now - last < DEBOUNCE_MS {
                    return;
                }
                LAST_SHORTCUT_TIME.store(now, Ordering::SeqCst);

                // CAS guard: prevent concurrent toggle
                if SHORTCUT_PROCESSING
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    return;
                }

                log::info!("Shortcut triggered");
                let h = handle.clone();
                std::thread::spawn(move || {
                    toggle_recording(&h);
                    SHORTCUT_PROCESSING.store(false, Ordering::SeqCst);
                });
            }
        });
}

/// Unregister old shortcut and register new one (called when settings change)
pub fn re_register_shortcut(
    app_handle: &tauri::AppHandle,
    old_shortcut_str: &str,
    new_settings: &AppSettings,
) {
    // Unregister old shortcut
    if let Ok(old) = old_shortcut_str.parse::<Shortcut>() {
        let _ = app_handle.global_shortcut().unregister(old);
        log::info!("Unregistered old shortcut: {}", old_shortcut_str);
    }
    // Register new shortcut
    register_shortcut(app_handle, new_settings);
    log::info!("Registered new shortcut: {}", new_settings.shortcut);
}

fn register_escape(app_handle: &tauri::AppHandle) {
    let escape: Shortcut = "Escape".parse().unwrap();
    let handle = app_handle.clone();
    let _ = app_handle
        .global_shortcut()
        .on_shortcut(escape, move |_app, _shortcut, event| {
            if event.state != ShortcutState::Released {
                log::info!("Escape triggered");
                let h = handle.clone();
                std::thread::spawn(move || {
                    cancel_recording(&h);
                });
            }
        });
}

fn unregister_escape(app_handle: &tauri::AppHandle) {
    if let Ok(escape) = "Escape".parse::<Shortcut>() {
        let _ = app_handle.global_shortcut().unregister(escape);
    }
}

fn toggle_recording(app_handle: &tauri::AppHandle) {
    let recorder = app_handle.state::<Arc<AudioRecorder>>();

    if recorder.is_recording() {
        log::info!("Stopping recording...");
        stop_and_transcribe(app_handle);
    } else {
        log::info!("Starting recording...");
        start_recording(app_handle);
    }
}

fn start_recording(app_handle: &tauri::AppHandle) {
    let recorder = app_handle.state::<Arc<AudioRecorder>>();

    // Use saved overlay position, or default to bottom-center of screen
    let saved = settings::get_settings();
    let (pos_x, pos_y) = if let (Some(x), Some(y)) = (saved.overlay_x, saved.overlay_y) {
        (x, y)
    } else if let Some(monitor) = app_handle.primary_monitor().ok().flatten() {
        let scale = monitor.scale_factor();
        let monitor_width = monitor.size().width as f64 / scale;
        let monitor_height = monitor.size().height as f64 / scale;
        let x = (monitor_width - OVERLAY_WIDTH) / 2.0;
        let y = monitor_height - OVERLAY_HEIGHT - OVERLAY_BOTTOM_OFFSET;
        (x, y)
    } else {
        (400.0, 800.0)
    };

    // Hide main window to prevent it from appearing when overlay activates the app
    if let Some(w) = app_handle.get_webview_window("main") {
        let _ = w.hide();
    }

    match tauri::WebviewWindowBuilder::new(
        app_handle,
        "overlay",
        tauri::WebviewUrl::App("/src/overlay/index.html".into()),
    )
    .title("")
    .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
    .position(pos_x, pos_y)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .accept_first_mouse(true)
    .build()
    {
        Ok(_) => {
            log::info!("Overlay window created");
        }
        Err(e) => log::error!("Failed to create overlay: {}", e),
    }

    if let Err(e) = recorder.start(app_handle.clone()) {
        log::error!("Failed to start recording: {}", e);
        close_overlay(app_handle);
        return;
    }
    log::info!("Recording started");

    // Register Escape only while recording
    register_escape(app_handle);
}

fn stop_and_transcribe(app_handle: &tauri::AppHandle) {
    unregister_escape(app_handle);

    let recorder = app_handle.state::<Arc<AudioRecorder>>();
    let history = app_handle.state::<Arc<HistoryManager>>();

    // Notify overlay
    let _ = app_handle.emit("transcribing", ());

    let audio = match recorder.stop() {
        Ok(a) => a,
        Err(e) => {
            log::error!("Failed to stop recording: {}", e);
            close_overlay(app_handle);
            return;
        }
    };
    log::info!(
        "Got {} samples at {}Hz",
        audio.samples.len(),
        audio.sample_rate
    );

    let sample_count = audio.samples.len();
    let sample_rate = audio.sample_rate;

    let wav_data = match encode_wav(&audio) {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to encode WAV: {}", e);
            close_overlay(app_handle);
            return;
        }
    };
    log::info!("WAV size: {} bytes", wav_data.len());

    // Save WAV file to ~/.nanowhisper/audio/
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S%.3f").to_string();
    let audio_filename = format!("{}.wav", timestamp);
    let audio_path = history.audio_dir().join(&audio_filename);
    if let Err(e) = std::fs::write(&audio_path, &wav_data) {
        log::error!("Failed to save audio file: {}", e);
    } else {
        log::info!("Audio saved: {}", audio_path.display());
    }
    let audio_path_str = audio_path.to_string_lossy().to_string();

    let settings = settings::get_settings();
    if settings.api_key.is_empty() {
        log::error!("API key not configured!");
        close_overlay(app_handle);
        if let Some(w) = app_handle.get_webview_window("main") {
            let _ = w.show();
            let _ = w.set_focus();
        }
        return;
    }

    let handle = app_handle.clone();
    let history = history.inner().clone();
    let model = settings.model.clone();
    let language = settings.language.clone();
    let api_key = settings.api_key.clone();
    let http_client = app_handle.state::<reqwest::Client>().inner().clone();

    log::info!("Calling API with model={}...", model);

    tauri::async_runtime::spawn(async move {
        let lang = if language == "auto" {
            None
        } else {
            Some(language.as_str())
        };

        match transcribe::transcribe_audio(&http_client, &api_key, &model, wav_data, lang).await {
            Ok(text) => {
                log::info!("Transcription: {}", text);

                // Copy to clipboard and auto-paste into active app
                let _ = handle.clipboard().write_text(&text);
                // Close overlay first so the previously active app regains focus
                close_overlay(&handle);
                // Paste on a dedicated OS thread — must NOT run on tokio
                let paste_handle = handle.clone();
                std::thread::spawn(move || {
                    // Wait for previous app to regain focus
                    std::thread::sleep(Duration::from_millis(PASTE_DELAY_MS));
                    if let Err(e) = paste::simulate_paste(&paste_handle) {
                        log::error!("Paste failed: {}", e);
                    }
                });

                // Save to history
                let duration_ms = if sample_rate > 0 {
                    Some((sample_count as i64 * 1000) / sample_rate as i64)
                } else {
                    None
                };
                let _ = history.add_entry(&text, &model, duration_ms, Some(&audio_path_str));

                // Notify main window to refresh
                let _ = handle.emit("history-updated", ());
            }
            Err(e) => {
                log::error!("Transcription failed: {}", e);
                let _ = handle.emit("transcription-error", e.to_string());
            }
        }

        close_overlay(&handle);
    });
}

fn cancel_recording(app_handle: &tauri::AppHandle) {
    let recorder = app_handle.state::<Arc<AudioRecorder>>();
    if recorder.is_recording() {
        log::info!("Cancelling recording...");
        unregister_escape(app_handle);
        recorder.cancel();
        close_overlay(app_handle);
    }
}

fn close_overlay(app_handle: &tauri::AppHandle) {
    if let Some(w) = app_handle.get_webview_window("overlay") {
        let _ = w.close();
    }
}
