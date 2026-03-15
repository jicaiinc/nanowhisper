use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_api_key")]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
}

fn default_api_key() -> String {
    std::env::var("OPENAI_API_KEY").unwrap_or_default()
}
fn default_model() -> String {
    "gpt-4o-transcribe".to_string()
}
fn default_language() -> String {
    "auto".to_string()
}
fn default_shortcut() -> String {
    "CmdOrCtrl+Shift+Space".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            api_key: default_api_key(),
            model: default_model(),
            language: default_language(),
            shortcut: default_shortcut(),
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

pub fn save_settings(settings: &AppSettings) {
    let dir = crate::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("settings.json");
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(&path, json);
    }
}
