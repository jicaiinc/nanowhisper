use crate::history::{HistoryEntry, HistoryManager};
use crate::paste::EnigoState;
use crate::recorder::AudioRecorder;
use crate::settings::{self, AppSettings};
use crate::updater;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[tauri::command]
pub fn get_history(history: State<'_, Arc<HistoryManager>>) -> Result<Vec<HistoryEntry>, String> {
    history.get_entries().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_history_entry(
    history: State<'_, Arc<HistoryManager>>,
    id: i64,
) -> Result<(), String> {
    history.delete_entry(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history(history: State<'_, Arc<HistoryManager>>) -> Result<(), String> {
    history.clear_all().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings() -> AppSettings {
    settings::get_settings()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) {
    let old_settings = settings::get_settings();
    settings::save_settings(&settings);

    // Hot-reload shortcut if changed
    if settings.shortcut != old_settings.shortcut {
        crate::re_register_shortcut(&app, &old_settings.shortcut, &settings);
    }
}

#[tauri::command]
pub fn check_accessibility() -> bool {
    crate::paste::is_accessibility_trusted()
}

#[tauri::command]
pub fn request_accessibility() -> bool {
    crate::paste::request_accessibility_with_prompt()
}

#[tauri::command]
pub fn check_microphone() -> bool {
    crate::permissions::check_microphone_permission()
}

#[tauri::command]
pub fn request_microphone() -> bool {
    crate::permissions::request_microphone_permission()
}

#[tauri::command]
pub async fn validate_api_key(
    app: AppHandle,
    api_key: String,
    provider: String,
    custom_base_url: String,
    model: String,
) -> Result<(), String> {
    let client = app
        .try_state::<reqwest::Client>()
        .ok_or("HTTP client not initialized")?;
    let vm = if model.trim().is_empty() {
        "whisper-1"
    } else {
        model.trim()
    };
    match provider.as_str() {
        "gemini" => crate::transcribe::validate_gemini_api_key(&client, &api_key)
            .await
            .map_err(|e| e.to_string()),
        "kimi" => crate::transcribe::validate_openai_compatible_key(
            &client,
            "https://api.moonshot.cn/v1",
            &api_key,
            vm,
        )
        .await
        .map_err(|e| e.to_string()),
        "custom" => {
            let base = custom_base_url.trim();
            if base.is_empty() {
                return Err("API base URL is required for custom provider".into());
            }
            crate::transcribe::validate_openai_compatible_key(&client, base, &api_key, vm)
                .await
                .map_err(|e| e.to_string())
        }
        _ => crate::transcribe::validate_openai_compatible_key(
            &client,
            "https://api.openai.com/v1",
            &api_key,
            vm,
        )
        .await
        .map_err(|e| e.to_string()),
    }
}

#[tauri::command]
pub fn pause_shortcut(app: AppHandle) {
    crate::hotkey::pause();
    let settings = settings::get_settings();
    if let Ok(shortcut) = settings.shortcut.parse::<Shortcut>() {
        let _ = app.global_shortcut().unregister(shortcut);
    }
    log::info!("Shortcuts paused for capture");
}

#[tauri::command]
pub fn resume_shortcut(app: AppHandle) {
    crate::hotkey::resume();
    let settings = settings::get_settings();
    crate::register_shortcut(&app, &settings);
    log::info!("Shortcuts resumed");
}

#[tauri::command]
pub fn save_overlay_position(app: AppHandle, x: f64, y: f64) {
    let (sx, sy, sw, sh) = crate::cursor_screen_bounds(&app);
    let mut s = settings::get_settings();
    s.overlay_rx = Some(((x - sx) / sw).clamp(0.0, 1.0));
    s.overlay_ry = Some(((y - sy) / sh).clamp(0.0, 1.0));
    settings::save_settings(&s);
}

#[tauri::command]
pub fn initialize_enigo(app: AppHandle) -> Result<(), String> {
    if !crate::paste::is_accessibility_trusted() {
        return Err("Accessibility not granted".into());
    }
    if app.try_state::<EnigoState>().is_some() {
        return Ok(());
    }
    let state = EnigoState::new()?;
    app.manage(state);
    Ok(())
}

#[tauri::command]
pub async fn retry_transcription(
    app: AppHandle,
    history: State<'_, Arc<HistoryManager>>,
    id: i64,
) -> Result<String, String> {
    use crate::transcribe;

    // Get the specific entry by ID
    let entry = history
        .get_entry_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or("Entry not found")?;
    let audio_path = entry
        .audio_path
        .as_ref()
        .ok_or("No audio file for this entry")?;

    // Read WAV file
    let wav_data = std::fs::read(audio_path).map_err(|e| e.to_string())?;

    let settings = crate::settings::get_settings();
    let is_gemini = settings.provider == "gemini";
    let active_key = if is_gemini { &settings.gemini_api_key } else { &settings.api_key };
    if active_key.is_empty() {
        return Err("API key not configured".into());
    }

    let lang = if settings.language == "auto" {
        None
    } else {
        Some(settings.language.as_str())
    };

    let client = app
        .try_state::<reqwest::Client>()
        .ok_or("HTTP client not initialized")?;
    let text = if is_gemini {
        transcribe::transcribe_gemini(&client, active_key, &settings.model, wav_data, lang)
            .await
            .map_err(|e| e.to_string())?
    } else {
        let base = crate::settings::openai_compatible_transcription_base(&settings);
        if settings.provider == "custom" && base.is_empty() {
            return Err("API base URL is not configured".into());
        }
        transcribe::transcribe_openai_compatible(
            &client,
            &base,
            active_key,
            &settings.model,
            wav_data,
            lang,
        )
        .await
        .map_err(|e| e.to_string())?
    };

    // Update entry in place (preserves ID and audio_path)
    history
        .update_entry(id, &text, &settings.model)
        .map_err(|e| e.to_string())?;

    // Copy + paste
    let _ = app.clipboard().write_text(&text);
    crate::paste::simulate_paste(&app).ok();

    let _ = app.emit("history-updated", ());

    Ok(text)
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<String>, String> {
    updater::check_and_download(&app)
        .await
        .map_err(|e| e.to_string())?;
    let state = app.state::<updater::UpdateState>();
    let version = state
        .pending
        .lock()
        .unwrap()
        .as_ref()
        .map(|u| u.version.clone());
    Ok(version)
}

#[tauri::command]
pub fn restart_to_update(
    app: AppHandle,
    recorder: State<'_, Arc<AudioRecorder>>,
) -> Result<(), String> {
    if recorder.is_recording() {
        return Err("Recording in progress".into());
    }
    let state = app.state::<updater::UpdateState>();
    let mut guard = state.pending.lock().unwrap();
    if let Some(p) = guard.as_ref() {
        // Install first; only remove pending data on success
        p.update.install(&p.bytes).map_err(|e| e.to_string())?;
        guard.take();
        drop(guard);
        app.restart();
    }
    Ok(())
}
