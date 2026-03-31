use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_api_key")]
    pub api_key: String,
    #[serde(default)]
    pub gemini_api_key: String,
    /// OpenAI-compatible API base (e.g. `https://api.example.com/v1`). Used when `provider` is `custom`.
    #[serde(default = "default_custom_api_base_url")]
    pub custom_api_base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
    #[serde(default = "default_sound_enabled")]
    pub sound_enabled: bool,
    #[serde(default)]
    pub overlay_rx: Option<f64>,
    #[serde(default)]
    pub overlay_ry: Option<f64>,
}

fn default_provider() -> String {
    "openai".to_string()
}
fn default_api_key() -> String {
    String::new()
}
fn default_custom_api_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}
fn default_model() -> String {
    "gpt-4o-transcribe".to_string()
}
fn default_language() -> String {
    "auto".to_string()
}
fn default_shortcut() -> String {
    String::new()
}
fn default_sound_enabled() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            api_key: default_api_key(),
            gemini_api_key: String::new(),
            custom_api_base_url: default_custom_api_base_url(),
            model: default_model(),
            language: default_language(),
            shortcut: default_shortcut(),
            sound_enabled: default_sound_enabled(),
            overlay_rx: None,
            overlay_ry: None,
        }
    }
}

fn settings_path() -> PathBuf {
    crate::data_dir().join("settings.json")
}

pub fn get_settings() -> AppSettings {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<AppSettings>(&content).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

/// Base URL for OpenAI-style `/v1/audio/transcriptions` (no trailing path segment).
pub fn openai_compatible_transcription_base(settings: &AppSettings) -> String {
    match settings.provider.as_str() {
        "kimi" => "https://api.moonshot.cn/v1".to_string(),
        "custom" => settings.custom_api_base_url.trim().trim_end_matches('/').to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

pub fn save_settings(settings: &AppSettings) {
    let dir = crate::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("settings.json");
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(&path, json);
    }
}
