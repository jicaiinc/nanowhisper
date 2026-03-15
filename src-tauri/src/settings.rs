use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const STORE_PATH: &str = "settings.json";

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

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = match app.store(STORE_PATH) {
        Ok(s) => s,
        Err(_) => return AppSettings::default(),
    };

    match store.get("settings") {
        Some(value) => serde_json::from_value::<AppSettings>(value).unwrap_or_default(),
        None => {
            let defaults = AppSettings::default();
            let _ = store.set(
                "settings",
                serde_json::to_value(&defaults).unwrap(),
            );
            defaults
        }
    }
}

pub fn save_settings(app: &AppHandle, settings: &AppSettings) {
    if let Ok(store) = app.store(STORE_PATH) {
        let _ = store.set("settings", serde_json::to_value(settings).unwrap());
    }
}
