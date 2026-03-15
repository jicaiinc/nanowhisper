export interface HistoryEntry {
  id: number;
  text: string;
  model: string;
  timestamp: number;
  duration_ms: number | null;
  audio_path: string | null;
}

export interface AppSettings {
  api_key: string;
  model: string;
  language: string;
  shortcut: string;
}
