import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { HistoryEntry, AppSettings } from "./types";

type View = "onboarding" | "history" | "settings";

const isMac = navigator.userAgent.includes("Mac");
const modKey = isMac ? "⌘" : "Ctrl";

function displayShortcut(s: string): string {
  return s
    .replace("CmdOrCtrl", modKey)
    .replace("Cmd", "⌘")
    .replace("Ctrl", "Ctrl")
    .replace("Shift", "⇧")
    .replace("Alt", isMac ? "⌥" : "Alt")
    .replace(/\+/g, " ");
}

function keyEventToShortcut(e: React.KeyboardEvent): string | null {
  const key = e.key;
  if (["Control", "Shift", "Alt", "Meta"].includes(key)) return null;
  const parts: string[] = [];
  if (e.metaKey || e.ctrlKey) parts.push("CmdOrCtrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");
  let mainKey = key.length === 1 ? key.toUpperCase() : key;
  if (mainKey === " ") mainKey = "Space";
  parts.push(mainKey);
  return parts.join("+");
}

function App() {
  const [view, setView] = useState<View>("history");
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [copied, setCopied] = useState<number | null>(null);
  const [retrying, setRetrying] = useState<number | null>(null);
  const [accessibilityOk, setAccessibilityOk] = useState(false);
  const [recordingShortcut, setRecordingShortcut] = useState(false);

  const loadHistory = useCallback(async () => {
    const entries = await invoke<HistoryEntry[]>("get_history");
    setHistory(entries);
  }, []);

  const loadSettings = useCallback(async () => {
    const s = await invoke<AppSettings>("get_settings");
    setSettings(s);
    if (!s.api_key) setView("onboarding");
  }, []);

  const checkAccessibility = useCallback(async () => {
    const ok = await invoke<boolean>("check_accessibility");
    setAccessibilityOk(ok);
  }, []);

  useEffect(() => {
    loadHistory();
    loadSettings();
    checkAccessibility();
    const unlisten = listen("history-updated", () => loadHistory());
    return () => { unlisten.then((f) => f()); };
  }, [loadHistory, loadSettings, checkAccessibility]);

  const copyText = async (text: string, id: number) => {
    await writeText(text);
    setCopied(id);
    setTimeout(() => setCopied(null), 1500);
  };

  const deleteEntry = async (id: number) => {
    await invoke("delete_history_entry", { id });
    setHistory((h) => h.filter((e) => e.id !== id));
  };

  const retryEntry = async (id: number) => {
    setRetrying(id);
    try {
      await invoke("retry_transcription", { id });
      await loadHistory();
    } catch (e) {
      console.error("Retry failed:", e);
    }
    setRetrying(null);
  };

  const saveSettings = async () => {
    if (!settings) return;
    await invoke("save_settings", { settings });
  };

  const formatTime = (ts: number) => {
    const d = new Date(ts * 1000);
    const now = new Date();
    if (d.toDateString() === now.toDateString()) {
      return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    }
    return d.toLocaleDateString([], { month: "short", day: "numeric" }) +
      " " + d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  };

  const formatDuration = (ms: number | null) => {
    if (!ms) return "";
    const secs = Math.round(ms / 1000);
    if (secs < 60) return `${secs}s`;
    const mins = Math.floor(secs / 60);
    const remainSecs = secs % 60;
    return `${mins}m${remainSecs}s`;
  };

  const ShortcutInput = () => (
    <div
      tabIndex={0}
      className="w-full px-3 py-2 rounded-lg text-sm outline-none text-center"
      style={{
        background: "var(--card)",
        border: recordingShortcut ? "1px solid var(--accent)" : "1px solid var(--border)",
        color: "var(--text)",
        cursor: "pointer",
      }}
      onClick={() => setRecordingShortcut(true)}
      onBlur={() => setRecordingShortcut(false)}
      onKeyDown={(e) => {
        if (!recordingShortcut || !settings) return;
        e.preventDefault();
        const shortcut = keyEventToShortcut(e);
        if (shortcut) {
          setSettings({ ...settings, shortcut });
          setRecordingShortcut(false);
        }
      }}
    >
      {recordingShortcut ? (
        <span style={{ color: "var(--accent)" }}>Press shortcut keys...</span>
      ) : (
        displayShortcut(settings?.shortcut || "")
      )}
    </div>
  );

  const ActionBtn = ({ onClick, label, accent }: { onClick: () => void; label: string; accent?: boolean }) => (
    <button
      onClick={onClick}
      className="text-xs px-3 py-1 rounded-md"
      style={{
        background: accent ? "var(--accent)" : "var(--border)",
        color: accent ? "white" : "var(--text)",
      }}
    >
      {label}
    </button>
  );

  // Onboarding
  if (view === "onboarding" && settings) {
    return (
      <div className="p-6 max-w-md mx-auto">
        <div className="text-center mb-6">
          <h1 className="text-xl font-semibold mb-1">NanoWhisper</h1>
          <p className="text-xs" style={{ color: "var(--text-secondary)" }}>Pure Whisper. Nothing else.</p>
        </div>

        <div className="space-y-5">
          <div>
            <div className="flex items-center gap-2 mb-2">
              <span className="text-xs font-medium px-2 py-0.5 rounded-full" style={{ background: "var(--accent)", color: "white" }}>1</span>
              <span className="text-sm font-medium">OpenAI API Key</span>
              {settings.api_key && <span style={{ color: "#34c759" }}>&#10003;</span>}
            </div>
            <input
              type="password"
              value={settings.api_key}
              onChange={(e) => setSettings({ ...settings, api_key: e.target.value })}
              placeholder="sk-proj-..."
              className="w-full px-3 py-2 rounded-lg text-sm outline-none"
              style={{ background: "var(--card)", border: "1px solid var(--border)", color: "var(--text)" }}
            />
          </div>

          <div>
            <div className="flex items-center gap-2 mb-2">
              <span className="text-xs font-medium px-2 py-0.5 rounded-full" style={{ background: "var(--accent)", color: "white" }}>2</span>
              <span className="text-sm font-medium">Accessibility</span>
              {accessibilityOk && <span style={{ color: "#34c759" }}>&#10003;</span>}
            </div>
            {accessibilityOk ? (
              <div className="px-3 py-2 rounded-lg text-sm" style={{ background: "var(--card)", border: "1px solid var(--border)", color: "#34c759" }}>
                Enabled
              </div>
            ) : (
              <button
                onClick={async () => {
                  await invoke("request_accessibility");
                  for (let i = 0; i < 10; i++) {
                    await new Promise((r) => setTimeout(r, 1000));
                    const ok = await invoke<boolean>("check_accessibility");
                    if (ok) { setAccessibilityOk(true); await invoke("initialize_enigo"); break; }
                  }
                }}
                className="w-full px-3 py-2 rounded-lg text-sm font-medium"
                style={{ background: "var(--accent)", color: "white" }}
              >
                Open System Settings
              </button>
            )}
            <p className="text-xs mt-1" style={{ color: "var(--text-secondary)" }}>
              Auto-paste transcribed text into active app. Optional.
            </p>
          </div>

          <div>
            <div className="flex items-center gap-2 mb-2">
              <span className="text-xs font-medium px-2 py-0.5 rounded-full" style={{ background: "var(--accent)", color: "white" }}>3</span>
              <span className="text-sm font-medium">Shortcut</span>
            </div>
            <ShortcutInput />
            <p className="text-xs mt-1" style={{ color: "var(--text-secondary)" }}>
              Press to record, again to stop. Escape to cancel.
            </p>
          </div>
        </div>

        <button
          onClick={() => { saveSettings(); setView("history"); }}
          disabled={!settings.api_key}
          className="w-full mt-6 py-2.5 rounded-lg text-sm font-medium"
          style={{
            background: settings.api_key ? "var(--accent)" : "var(--border)",
            color: settings.api_key ? "white" : "var(--text-secondary)",
            cursor: settings.api_key ? "pointer" : "not-allowed",
          }}
        >
          Get Started
        </button>
      </div>
    );
  }

  // Settings
  if (view === "settings" && settings) {
    return (
      <div className="p-4 max-w-md mx-auto">
        <div className="flex items-center justify-between mb-4">
          <h1 className="text-lg font-semibold">Settings</h1>
          <button onClick={() => { saveSettings(); setView("history"); }} className="text-sm" style={{ color: "var(--accent)" }}>Done</button>
        </div>
        <div className="space-y-4">
          <div>
            <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>API Key</label>
            <input type="password" value={settings.api_key} onChange={(e) => setSettings({ ...settings, api_key: e.target.value })} placeholder="sk-..." className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={{ background: "var(--card)", border: "1px solid var(--border)", color: "var(--text)" }} />
          </div>
          <div>
            <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Model</label>
            <select value={settings.model} onChange={(e) => setSettings({ ...settings, model: e.target.value })} className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={{ background: "var(--card)", border: "1px solid var(--border)", color: "var(--text)" }}>
              <option value="gpt-4o-transcribe">gpt-4o-transcribe</option>
              <option value="gpt-4o-mini-transcribe">gpt-4o-mini-transcribe</option>
              <option value="whisper-1">whisper-1</option>
            </select>
          </div>
          <div>
            <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Language</label>
            <select value={settings.language} onChange={(e) => setSettings({ ...settings, language: e.target.value })} className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={{ background: "var(--card)", border: "1px solid var(--border)", color: "var(--text)" }}>
              <option value="auto">Auto Detect</option>
              <option value="zh">Chinese</option>
              <option value="en">English</option>
              <option value="ja">Japanese</option>
              <option value="ko">Korean</option>
              <option value="es">Spanish</option>
              <option value="fr">French</option>
              <option value="de">German</option>
            </select>
          </div>
          <div>
            <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Shortcut</label>
            <ShortcutInput />
          </div>
          <div>
            <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Accessibility</label>
            {accessibilityOk ? (
              <div className="px-3 py-2 rounded-lg text-sm" style={{ background: "var(--card)", border: "1px solid var(--border)", color: "#34c759" }}>Enabled</div>
            ) : (
              <button onClick={async () => { await invoke("request_accessibility"); setTimeout(async () => { const ok = await invoke<boolean>("check_accessibility"); if (ok) { setAccessibilityOk(true); await invoke("initialize_enigo"); } }, 2000); }} className="w-full px-3 py-2 rounded-lg text-sm font-medium" style={{ background: "var(--accent)", color: "white" }}>
                Open System Settings
              </button>
            )}
          </div>
        </div>
      </div>
    );
  }

  // History
  return (
    <div className="p-4 max-w-md mx-auto">
      <div className="flex items-center justify-between mb-4">
        <h1 className="text-lg font-semibold">NanoWhisper</h1>
        <button onClick={() => setView("settings")} className="text-xl px-1" style={{ color: "var(--text-secondary)" }}>&#9881;</button>
      </div>

      {history.length === 0 ? (
        <p className="text-center py-8 text-sm" style={{ color: "var(--text-secondary)" }}>
          No transcriptions yet.
          <br />
          Press {displayShortcut(settings?.shortcut || "CmdOrCtrl+Shift+Space")} to start.
          <br />
          <span className="text-xs">Press again to stop. Escape to cancel.</span>
        </p>
      ) : (
        <div className="space-y-2">
          {history.map((entry) => (
            <div key={entry.id} className="rounded-lg p-3" style={{ background: "var(--card)", border: "1px solid var(--border)" }}>
              <div
                className="text-sm cursor-pointer"
                onClick={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
                style={{ userSelect: "text" }}
              >
                {expandedId === entry.id
                  ? entry.text
                  : entry.text.length > 100
                    ? entry.text.slice(0, 100) + "..."
                    : entry.text}
              </div>
              <div className="flex items-center justify-between mt-2">
                <span className="text-xs" style={{ color: "var(--text-secondary)" }}>
                  {entry.duration_ms ? formatDuration(entry.duration_ms) + " · " : ""}{formatTime(entry.timestamp)}
                </span>
                <div className="flex gap-1">
                  <ActionBtn onClick={() => copyText(entry.text, entry.id)} label={copied === entry.id ? "Copied!" : "Copy"} accent={copied === entry.id} />
                  {entry.audio_path && (
                    <ActionBtn
                      onClick={() => retryEntry(entry.id)}
                      label={retrying === entry.id ? "..." : "Retry"}
                    />
                  )}
                  <ActionBtn onClick={() => deleteEntry(entry.id)} label="Delete" />
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export default App;
