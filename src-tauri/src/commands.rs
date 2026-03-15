use crate::history::{HistoryEntry, HistoryManager};
use crate::paste::EnigoState;
use crate::settings::{self, AppSettings};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[tauri::command]
pub fn get_history(history: State<'_, Arc<HistoryManager>>) -> Result<Vec<HistoryEntry>, String> {
    history.get_entries().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_history_entry(history: State<'_, Arc<HistoryManager>>, id: i64) -> Result<(), String> {
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
pub fn save_settings(settings: AppSettings) {
    settings::save_settings(&settings);
}

#[tauri::command]
pub fn check_accessibility() -> bool {
    crate::paste::is_accessibility_trusted()
}

#[tauri::command]
pub fn request_accessibility() -> bool {
    crate::paste::request_accessibility()
}

#[tauri::command]
pub fn initialize_enigo(app: AppHandle) -> Result<(), String> {
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

    // Get the entry to find audio_path
    let entries = history.get_entries().map_err(|e| e.to_string())?;
    let entry = entries.iter().find(|e| e.id == id)
        .ok_or("Entry not found")?;
    let audio_path = entry.audio_path.as_ref()
        .ok_or("No audio file for this entry")?;

    // Read WAV file
    let wav_data = std::fs::read(audio_path).map_err(|e| e.to_string())?;

    let settings = crate::settings::get_settings();
    if settings.api_key.is_empty() {
        return Err("API key not configured".into());
    }

    let lang = if settings.language == "auto" { None } else { Some(settings.language.as_str()) };
    let text = transcribe::transcribe_audio(&settings.api_key, &settings.model, wav_data, lang)
        .await
        .map_err(|e| e.to_string())?;

    // Update history entry — delete old, add new with same audio
    history.delete_entry(id).map_err(|e| e.to_string())?;
    let duration_ms = entry.duration_ms;
    history.add_entry(&text, &settings.model, duration_ms, Some(audio_path))
        .map_err(|e| e.to_string())?;

    // Copy + paste
    let _ = app.clipboard().write_text(&text);
    crate::paste::simulate_paste(&app).ok();

    let _ = app.emit("history-updated", ());

    Ok(text)
}
