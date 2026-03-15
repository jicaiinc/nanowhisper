mod commands;
mod history;
pub mod paste;
mod recorder;
mod settings;
mod transcribe;

use history::HistoryManager;
use recorder::{encode_wav, AudioRecorder};
use settings::AppSettings;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// ~/.nanowhisper/
pub fn data_dir() -> PathBuf {
    let home = dirs::home_dir().expect("Cannot determine home directory");
    home.join(".nanowhisper")
}

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
            commands::initialize_enigo,
            commands::retry_transcription,
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

            // Initialize enigo if accessibility is already granted
            if paste::is_accessibility_trusted() {
                if let Ok(enigo_state) = paste::EnigoState::new() {
                    app.manage(enigo_state);
                }
            }


            // Create main window
            let _main_window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("/".into()),
            )
            .title("NanoWhisper")
            .inner_size(420.0, 600.0)
            .min_inner_size(380.0, 400.0)
            .resizable(true)
            .maximizable(false)
            .visible(false)
            .build()?;

            // System tray
            let show_i =
                tauri::menu::MenuItem::with_id(app, "show", "Show NanoWhisper", true, None::<&str>)?;
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

            println!("[NanoWhisper] App started. Shortcut: {}", settings.shortcut);
            println!("[NanoWhisper] API key configured: {}", !settings.api_key.is_empty());

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
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
}

static SHORTCUT_PROCESSING: AtomicBool = AtomicBool::new(false);
static LAST_SHORTCUT_TIME: AtomicU64 = AtomicU64::new(0);
const DEBOUNCE_MS: u64 = 500;

fn register_shortcut(app_handle: &tauri::AppHandle, settings: &AppSettings) {
    let shortcut_str = &settings.shortcut;
    let shortcut: Shortcut = match shortcut_str.parse() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[NanoWhisper] Invalid shortcut '{}': {}", shortcut_str, e);
            return;
        }
    };

    let handle = app_handle.clone();
    let _ = app_handle.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
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
            if SHORTCUT_PROCESSING.compare_exchange(
                false, true, Ordering::SeqCst, Ordering::SeqCst
            ).is_err() {
                return;
            }

            println!("[NanoWhisper] Shortcut triggered");
            let h = handle.clone();
            std::thread::spawn(move || {
                toggle_recording(&h);
                SHORTCUT_PROCESSING.store(false, Ordering::SeqCst);
            });
        }
    });
}

fn register_escape(app_handle: &tauri::AppHandle) {
    let escape: Shortcut = "Escape".parse().unwrap();
    let handle = app_handle.clone();
    let _ = app_handle.global_shortcut().on_shortcut(escape, move |_app, _shortcut, event| {
        if event.state != ShortcutState::Released {
            println!("[NanoWhisper] Escape triggered");
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
        println!("[NanoWhisper] Stopping recording...");
        stop_and_transcribe(app_handle);
    } else {
        println!("[NanoWhisper] Starting recording...");
        start_recording(app_handle);
    }
}

fn start_recording(app_handle: &tauri::AppHandle) {
    let recorder = app_handle.state::<Arc<AudioRecorder>>();

    println!("[NanoWhisper] Creating overlay window...");

    // Create overlay window at top-center of screen
    let overlay_width = 320.0;
    let overlay_height = 48.0;
    let (pos_x, pos_y) = if let Some(monitor) = app_handle.primary_monitor().ok().flatten() {
        let scale = monitor.scale_factor();
        let monitor_width = monitor.size().width as f64 / scale;
        let x = (monitor_width - overlay_width) / 2.0;
        (x, 46.0)
    } else {
        (400.0, 46.0)
    };

    match tauri::WebviewWindowBuilder::new(
        app_handle,
        "overlay",
        tauri::WebviewUrl::App("/src/overlay/index.html".into()),
    )
    .title("")
    .inner_size(overlay_width, overlay_height)
    .position(pos_x, pos_y)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .build()
    {
        Ok(_) => {
            println!("[NanoWhisper] Overlay window created");
        }
        Err(e) => eprintln!("[NanoWhisper] Failed to create overlay: {}", e),
    }

    println!("[NanoWhisper] Starting audio recorder...");
    if let Err(e) = recorder.start(app_handle.clone()) {
        eprintln!("[NanoWhisper] Failed to start recording: {}", e);
        close_overlay(app_handle);
        return;
    }
    println!("[NanoWhisper] Recording started");

    // Register Escape only while recording
    register_escape(app_handle);
}

fn stop_and_transcribe(app_handle: &tauri::AppHandle) {
    unregister_escape(app_handle);

    let recorder = app_handle.state::<Arc<AudioRecorder>>();
    let history = app_handle.state::<Arc<HistoryManager>>();

    // Notify overlay
    let _ = app_handle.emit("transcribing", ());

    println!("[NanoWhisper] Stopping recorder...");
    let audio = match recorder.stop() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[NanoWhisper] Failed to stop recording: {}", e);
            close_overlay(app_handle);
            return;
        }
    };
    println!("[NanoWhisper] Got {} samples at {}Hz", audio.samples.len(), audio.sample_rate);

    let sample_count = audio.samples.len();
    let sample_rate = audio.sample_rate;

    println!("[NanoWhisper] Encoding WAV...");
    let wav_data = match encode_wav(&audio) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[NanoWhisper] Failed to encode WAV: {}", e);
            close_overlay(app_handle);
            return;
        }
    };
    println!("[NanoWhisper] WAV size: {} bytes", wav_data.len());

    // Save WAV file to ~/.nanowhisper/audio/
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let audio_filename = format!("{}.wav", timestamp);
    let audio_path = history.audio_dir().join(&audio_filename);
    if let Err(e) = std::fs::write(&audio_path, &wav_data) {
        eprintln!("[NanoWhisper] Failed to save audio file: {}", e);
    } else {
        println!("[NanoWhisper] Audio saved: {}", audio_path.display());
    }
    let audio_path_str = audio_path.to_string_lossy().to_string();

    let settings = settings::get_settings();
    if settings.api_key.is_empty() {
        eprintln!("[NanoWhisper] API key not configured!");
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

    println!("[NanoWhisper] Calling API with model={}...", model);

    tauri::async_runtime::spawn(async move {
        let lang = if language == "auto" {
            None
        } else {
            Some(language.as_str())
        };

        match transcribe::transcribe_audio(&api_key, &model, wav_data, lang).await {
            Ok(text) => {
                println!("[NanoWhisper] Transcription: {}", text);

                // Copy to clipboard and auto-paste into active app
                let _ = handle.clipboard().write_text(&text);
                if let Err(e) = paste::simulate_paste(&handle) {
                    eprintln!("[NanoWhisper] Paste failed: {}", e);
                }

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
                eprintln!("[NanoWhisper] Transcription failed: {}", e);
            }
        }

        close_overlay(&handle);
    });
}

fn cancel_recording(app_handle: &tauri::AppHandle) {
    let recorder = app_handle.state::<Arc<AudioRecorder>>();
    if recorder.is_recording() {
        println!("[NanoWhisper] Cancelling recording...");
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
