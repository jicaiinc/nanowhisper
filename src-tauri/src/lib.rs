mod commands;
mod history;
mod hotkey;
pub mod paste;
mod permissions;
mod recorder;
mod settings;
mod sound;
mod transcribe;
mod updater;

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

/// Returns (x, y, width, height) of the screen containing the cursor,
/// in logical coordinates with top-left origin.
pub fn cursor_screen_bounds(app_handle: &tauri::AppHandle) -> (f64, f64, f64, f64) {
    #[cfg(target_os = "macos")]
    if let Some(bounds) = macos_cursor_screen_bounds() {
        return bounds;
    }
    if let Some(monitor) = app_handle.primary_monitor().ok().flatten() {
        let scale = monitor.scale_factor();
        let pos = monitor.position();
        let size = monitor.size();
        (
            pos.x as f64 / scale,
            pos.y as f64 / scale,
            size.width as f64 / scale,
            size.height as f64 / scale,
        )
    } else {
        (0.0, 0.0, 1920.0, 1080.0)
    }
}

/// Returns (x, y, width, height) of the screen containing the cursor,
/// in logical coordinates with top-left origin (Tauri coordinate system).
#[cfg(target_os = "macos")]
fn macos_cursor_screen_bounds() -> Option<(f64, f64, f64, f64)> {
    use objc2::encode::{Encode, Encoding};
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct CGPoint { x: f64, y: f64 }
    unsafe impl Encode for CGPoint {
        const ENCODING: Encoding =
            Encoding::Struct("CGPoint", &[Encoding::Double, Encoding::Double]);
    }
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct CGSize { width: f64, height: f64 }
    unsafe impl Encode for CGSize {
        const ENCODING: Encoding =
            Encoding::Struct("CGSize", &[Encoding::Double, Encoding::Double]);
    }
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct CGRect { origin: CGPoint, size: CGSize }
    unsafe impl Encode for CGRect {
        const ENCODING: Encoding =
            Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
    }

    unsafe {
        let mouse: CGPoint = msg_send![AnyClass::get(c"NSEvent")?, mouseLocation];
        let screens: *mut AnyObject = msg_send![AnyClass::get(c"NSScreen")?, screens];
        let count: usize = msg_send![screens, count];
        if count == 0 { return None; }

        // Main screen height for Y-axis flip (macOS bottom-left → Tauri top-left)
        let main: *mut AnyObject = msg_send![screens, objectAtIndex: 0usize];
        let main_frame: CGRect = msg_send![main, frame];

        for i in 0..count {
            let scr: *mut AnyObject = msg_send![screens, objectAtIndex: i];
            let frame: CGRect = msg_send![scr, frame];
            if mouse.x >= frame.origin.x
                && mouse.x < frame.origin.x + frame.size.width
                && mouse.y >= frame.origin.y
                && mouse.y < frame.origin.y + frame.size.height
            {
                let x = frame.origin.x;
                let y = main_frame.size.height - frame.origin.y - frame.size.height;
                return Some((x, y, frame.size.width, frame.size.height));
            }
        }
    }
    None
}

/// Set NSWindowCollectionBehavior on the overlay so it appears on all Spaces.
/// Called after window is already visible (no show/focus side effects).
#[cfg(target_os = "macos")]
fn macos_set_overlay_all_spaces(app_handle: &tauri::AppHandle, window: tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let _ = app_handle.run_on_main_thread(move || {
        if let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(&window) {
            if let raw_window_handle::RawWindowHandle::AppKit(h) = handle.as_raw() {
                unsafe {
                    let ns_view = h.ns_view.as_ptr() as *mut AnyObject;
                    let ns_window: *mut AnyObject = msg_send![ns_view, window];
                    if !ns_window.is_null() {
                        let existing: u64 = msg_send![ns_window, collectionBehavior];
                        let _: () = msg_send![
                            ns_window,
                            setCollectionBehavior: existing | (1u64 << 0) | (1u64 << 8)
                        ];
                    }
                }
            }
        }
    });
}

pub fn run() {
    // Load .env file if present (for development)
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            commands::validate_api_key,
            commands::retry_transcription,
            commands::save_overlay_position,
            commands::pause_shortcut,
            commands::resume_shortcut,
            commands::check_for_update,
            commands::restart_to_update,
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
                    .inner_size(420.0, 680.0)
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

            let tray_icon = tauri::image::Image::from_bytes(
                include_bytes!("../icons/tray-icon.png"),
            )
            .expect("Failed to load tray icon");

            tauri::tray::TrayIconBuilder::new()
                .icon(tray_icon)
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

            // Start native hotkey monitor (Right Command on macOS, Right Ctrl on Windows)
            // hotkey.rs already has its own 500ms debounce, so we only need the CAS guard here.
            let hotkey_handle = app_handle.clone();
            hotkey::start(move || {
                if SHORTCUT_PROCESSING
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    return;
                }

                log::info!("Native hotkey triggered");
                let h = hotkey_handle.clone();
                std::thread::spawn(move || {
                    toggle_recording(&h);
                    SHORTCUT_PROCESSING.store(false, Ordering::SeqCst);
                });
            });

            // Register global shortcut (secondary, user-configurable)
            let settings = settings::get_settings();
            register_shortcut(&app_handle, &settings);

            // Show main window
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
            }

            // Initialize auto-updater
            updater::init(&app_handle);

            log::info!("App started. Provider: {}, Shortcut: {}", settings.provider, settings.shortcut);
            let active_key_set = if settings.provider == "gemini" {
                !settings.gemini_api_key.is_empty()
            } else {
                !settings.api_key.is_empty()
            };
            log::info!("API key configured: {}", active_key_set);

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
    if shortcut_str.is_empty() {
        return; // No custom shortcut configured; native hotkey only
    }
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

    let saved = settings::get_settings();
    let (sx, sy, sw, sh) = cursor_screen_bounds(app_handle);
    let (pos_x, pos_y) = if let (Some(rx), Some(ry)) = (saved.overlay_rx, saved.overlay_ry) {
        (sx + rx * sw, sy + ry * sh)
    } else {
        (sx + (sw - OVERLAY_WIDTH) / 2.0, sy + sh - OVERLAY_HEIGHT - OVERLAY_BOTTOM_OFFSET)
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
    .shadow(false)
    .focused(false)
    .accept_first_mouse(true)
    .build()
    {
        Ok(w) => {
            log::info!("Overlay window created");
            #[cfg(target_os = "macos")]
            macos_set_overlay_all_spaces(app_handle, w);
        }
        Err(e) => log::error!("Failed to create overlay: {}", e),
    }

    // Play start sound BEFORE opening mic (blocking) so it won't be recorded
    if saved.sound_enabled {
        sound::play_start_sound();
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

    // Play stop sound AFTER mic is closed (async, won't be recorded)
    if settings::get_settings().sound_enabled {
        sound::play_stop_sound();
    }

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
    let is_gemini = settings.provider == "gemini";
    let active_key = if is_gemini {
        settings.gemini_api_key.clone()
    } else {
        settings.api_key.clone()
    };
    if active_key.is_empty() {
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
    let http_client = app_handle.state::<reqwest::Client>().inner().clone();

    log::info!("Calling {} API with model={}...", settings.provider, model);

    let duration_ms = if sample_rate > 0 {
        Some((sample_count as i64 * 1000) / sample_rate as i64)
    } else {
        None
    };

    tauri::async_runtime::spawn(async move {
        let lang = if language == "auto" {
            None
        } else {
            Some(language.as_str())
        };

        let result = if is_gemini {
            transcribe::transcribe_gemini(&http_client, &active_key, &model, wav_data, lang).await
        } else {
            transcribe::transcribe_audio(&http_client, &active_key, &model, wav_data, lang).await
        };

        match result {
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
                let _ = history.add_entry(&text, &model, duration_ms, Some(&audio_path_str));
            }
            Err(e) => {
                log::error!("Transcription failed: {}", e);

                // Save failed entry to history so user can retry
                let error_text = format!("[Error: {}]", e);
                let _ = history.add_entry(&error_text, &model, duration_ms, Some(&audio_path_str));

                let _ = handle.emit("transcription-error", e.to_string());
            }
        }

        close_overlay(&handle);
        // Notify main window to refresh (both success and failure)
        let _ = handle.emit("history-updated", ());
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
