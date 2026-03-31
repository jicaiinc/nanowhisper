export interface HistoryEntry {
  id: number;
  text: string;
  model: string;
  timestamp: number;
  duration_ms: number | null;
  audio_path: string | null;
}

export interface AppSettings {
  provider: string;
  api_key: string;
  gemini_api_key: string;
  /** OpenAI-compatible base URL (e.g. https://api.example.com/v1). Used when provider is `custom`. */
  custom_api_base_url: string;
  model: string;
  language: string;
  shortcut: string;
  sound_enabled: boolean;
  overlay_x: number | null;
  overlay_y: number | null;
}
