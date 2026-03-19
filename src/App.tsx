import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { HistoryEntry, AppSettings } from "./types";
import logoUrl from "./assets/logo.png";

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

/** Map KeyboardEvent.code to Tauri shortcut key name */
function codeToTauriKey(code: string): string | null {
  if (code.startsWith("Key") && code.length === 4) return code.charAt(3);
  if (code.startsWith("Digit") && code.length === 6) return code.charAt(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  const map: Record<string, string> = {
    Space: "Space", Tab: "Tab", Enter: "Enter", Escape: "Escape",
    Backspace: "Backspace", Delete: "Delete",
    ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
    Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
    Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]",
    Backslash: "\\", Semicolon: ";", Quote: "'",
    Comma: ",", Period: ".", Slash: "/", Backquote: "`",
  };
  return map[code] ?? null;
}

function ShortcutInput({ shortcut, onCapture }: { shortcut: string; onCapture: (s: string) => void }) {
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pausedRef = useRef(false);

  useEffect(() => {
    return () => {
      if (pausedRef.current) {
        invoke("resume_shortcut");
        pausedRef.current = false;
      }
    };
  }, []);

  const handleClick = async () => {
    if (recording) return;
    if (!pausedRef.current) {
      pausedRef.current = true;
      await invoke("pause_shortcut");
    }
    setRecording(true);
    setError(null);
  };

  const handleBlur = async () => {
    setRecording(false);
    if (pausedRef.current) {
      pausedRef.current = false;
      await invoke("resume_shortcut");
    }
  };

  const handleKeyDown = async (e: React.KeyboardEvent) => {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;
    if (!e.metaKey && !e.ctrlKey && !e.altKey) {
      setError("Shortcut must include a modifier key (Cmd/Ctrl/Alt)");
      return;
    }
    const mainKey = codeToTauriKey(e.code);
    if (!mainKey) return;
    const parts: string[] = [];
    if (e.metaKey || e.ctrlKey) parts.push("CmdOrCtrl");
    if (e.shiftKey) parts.push("Shift");
    if (e.altKey) parts.push("Alt");
    parts.push(mainKey);
    setError(null);
    setRecording(false);
    onCapture(parts.join("+"));
    if (pausedRef.current) {
      pausedRef.current = false;
      await invoke("resume_shortcut");
    }
  };

  return (
    <div>
      <div
        tabIndex={0}
        className="w-full px-3 py-2 rounded-lg text-sm outline-none text-center"
        style={{
          background: "var(--card)",
          border: recording ? "1px solid var(--accent)" : error ? "1px solid #ff453a" : "1px solid var(--border)",
          color: "var(--text)",
          cursor: "pointer",
        }}
        onClick={handleClick}
        onBlur={handleBlur}
        onKeyDown={handleKeyDown}
      >
        {recording ? (
          <span style={{ color: "var(--accent)" }}>Press shortcut keys...</span>
        ) : (
          displayShortcut(shortcut)
        )}
      </div>
      {error && (
        <p className="text-xs mt-1" style={{ color: "#ff453a" }}>{error}</p>
      )}
    </div>
  );
}

function App() {
  const [view, setView] = useState<View>("history");
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [copied, setCopied] = useState<number | null>(null);
  const [retrying, setRetrying] = useState<number | null>(null);
  const [microphoneOk, setMicrophoneOk] = useState(false);
  const [accessibilityOk, setAccessibilityOk] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const loadHistory = useCallback(async () => {
    const entries = await invoke<HistoryEntry[]>("get_history");
    setHistory(entries);
  }, []);

  const loadSettings = useCallback(async () => {
    const s = await invoke<AppSettings>("get_settings");
    setSettings(s);
    if (!s.api_key) setView("onboarding");
  }, []);

  const checkPermissions = useCallback(async () => {
    const [mic, acc] = await Promise.all([
      invoke<boolean>("check_microphone"),
      invoke<boolean>("check_accessibility"),
    ]);
    setMicrophoneOk(mic);
    setAccessibilityOk(acc);
  }, []);

  const waitForPermission = useCallback(
    async (
      command: "check_microphone" | "check_accessibility",
      setter: (value: boolean) => void,
      attempts = 15,
    ) => {
      for (let attempt = 0; attempt < attempts; attempt += 1) {
        const ok = await invoke<boolean>(command);
        setter(ok);
        if (ok) return true;
        if (attempt < attempts - 1) {
          await new Promise((resolve) => setTimeout(resolve, 1000));
        }
      }
      return false;
    },
    [],
  );

  useEffect(() => {
    loadHistory();
    loadSettings();
    checkPermissions();
    const unlisten1 = listen("history-updated", () => loadHistory());
    const unlisten2 = listen<string>("transcription-error", (e) => {
      setErrorMsg(e.payload);
      setTimeout(() => setErrorMsg(null), 5000);
    });
    return () => {
      unlisten1.then((f) => f());
      unlisten2.then((f) => f());
    };
  }, [loadHistory, loadSettings, checkPermissions]);

  // Poll permissions only until all granted
  useEffect(() => {
    if (microphoneOk && accessibilityOk) return;
    const permInterval = setInterval(checkPermissions, 2000);
    return () => clearInterval(permInterval);
  }, [microphoneOk, accessibilityOk, checkPermissions]);

  useEffect(() => {
    if (!accessibilityOk) return;
    invoke("initialize_enigo").catch((error) => {
      console.error("Failed to initialize auto-paste:", error);
    });
  }, [accessibilityOk]);

  const handleEnableMicrophone = useCallback(async () => {
    await invoke("request_microphone");
    await waitForPermission("check_microphone", setMicrophoneOk);
  }, [waitForPermission]);

  const handleEnableAccessibility = useCallback(async () => {
    await invoke("request_accessibility");
    await waitForPermission("check_accessibility", setAccessibilityOk);
  }, [waitForPermission]);

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

  const IconBtn = ({ onClick, title, children, accent }: { onClick: () => void; title: string; children: React.ReactNode; accent?: boolean }) => (
    <button
      onClick={onClick}
      title={title}
      className="p-1.5 rounded-md"
      style={{
        background: accent ? "var(--accent)" : "transparent",
        color: accent ? "white" : "var(--text-secondary)",
        lineHeight: 0,
      }}
    >
      {children}
    </button>
  );

  // Onboarding
  if (view === "onboarding" && settings) {
    const canProceed = settings.api_key && microphoneOk && (isMac ? accessibilityOk : true);
    return (
      <div className="p-6 max-w-md mx-auto">
        <div className="flex flex-col items-center mb-6">
          <div className="flex items-center gap-2 mb-1">
            <img src={logoUrl} alt="" width={28} height={28} />
            <h1 className="text-xl font-semibold">NanoWhisper</h1>
          </div>
          <p className="text-xs" style={{ color: "var(--text-secondary)" }}>Pure Whisper. Nothing else.</p>
        </div>

        <div className="space-y-5">
          {/* Step 1: API Key */}
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

          {/* Step 2: Microphone (required) */}
          <div>
            <div className="flex items-center gap-2 mb-2">
              <span className="text-xs font-medium px-2 py-0.5 rounded-full" style={{ background: "var(--accent)", color: "white" }}>2</span>
              <span className="text-sm font-medium">Microphone</span>
              {microphoneOk && <span style={{ color: "#34c759" }}>&#10003;</span>}
            </div>
            {microphoneOk ? (
              <div className="px-3 py-2 rounded-lg text-sm" style={{ background: "var(--card)", border: "1px solid var(--border)", color: "#34c759" }}>Enabled</div>
            ) : (
              <button
                onClick={handleEnableMicrophone}
                className="w-full px-3 py-2 rounded-lg text-sm font-medium"
                style={{ background: "var(--accent)", color: "white" }}
              >
                Allow Microphone
              </button>
            )}
            <p className="text-xs mt-1" style={{ color: "var(--text-secondary)" }}>Required for voice recording.</p>
          </div>

          {/* Step 3: Accessibility */}
          <div>
            <div className="flex items-center gap-2 mb-2">
              <span className="text-xs font-medium px-2 py-0.5 rounded-full" style={{ background: "var(--accent)", color: "white" }}>3</span>
              <span className="text-sm font-medium">Accessibility</span>
              {accessibilityOk && <span style={{ color: "#34c759" }}>&#10003;</span>}
            </div>
            {accessibilityOk ? (
              <div className="px-3 py-2 rounded-lg text-sm" style={{ background: "var(--card)", border: "1px solid var(--border)", color: "#34c759" }}>Enabled</div>
            ) : (
              <button
                onClick={handleEnableAccessibility}
                className="w-full px-3 py-2 rounded-lg text-sm font-medium"
                style={{ background: "var(--accent)", color: "white" }}
              >
                Allow Accessibility
              </button>
            )}
            <p className="text-xs mt-1" style={{ color: "var(--text-secondary)" }}>Required for auto-paste after transcription.</p>
          </div>

          {/* Step 4: Shortcut */}
          <div>
            <div className="flex items-center gap-2 mb-2">
              <span className="text-xs font-medium px-2 py-0.5 rounded-full" style={{ background: "var(--border)", color: "var(--text-secondary)" }}>4</span>
              <span className="text-sm font-medium">Shortcut</span>
            </div>
            <ShortcutInput shortcut={settings.shortcut} onCapture={(s) => setSettings({ ...settings, shortcut: s })} />
            <p className="text-xs mt-1" style={{ color: "var(--text-secondary)" }}>Press to record, again to stop. Escape to cancel.</p>
          </div>
        </div>

        <button
          onClick={() => {
            saveSettings();
            setView("history");
          }}
          disabled={!canProceed}
          className="w-full mt-6 py-2.5 rounded-lg text-sm font-medium"
          style={{
            background: canProceed ? "var(--accent)" : "var(--border)",
            color: canProceed ? "white" : "var(--text-secondary)",
            cursor: canProceed ? "pointer" : "not-allowed",
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
            <ShortcutInput shortcut={settings.shortcut} onCapture={(s) => setSettings({ ...settings, shortcut: s })} />
          </div>
          <div>
            <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Microphone</label>
            {microphoneOk ? (
              <div className="px-3 py-2 rounded-lg text-sm" style={{ background: "var(--card)", border: "1px solid var(--border)", color: "#34c759" }}>Enabled</div>
            ) : (
              <button
                onClick={handleEnableMicrophone}
                className="w-full px-3 py-2 rounded-lg text-sm font-medium"
                style={{ background: "var(--accent)", color: "white" }}
              >
                Enable Microphone
              </button>
            )}
          </div>
          <div>
            <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Accessibility</label>
            {accessibilityOk ? (
              <div className="px-3 py-2 rounded-lg text-sm" style={{ background: "var(--card)", border: "1px solid var(--border)", color: "#34c759" }}>Enabled</div>
            ) : (
              <button onClick={handleEnableAccessibility} className="w-full px-3 py-2 rounded-lg text-sm font-medium" style={{ background: "var(--accent)", color: "white" }}>
                Allow Accessibility
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
        <div className="flex items-center gap-2">
          <img src={logoUrl} alt="" width={24} height={24} />
          <h1 className="text-lg font-semibold">NanoWhisper</h1>
        </div>
        <button onClick={() => setView("settings")} className="text-xl px-1" style={{ color: "var(--text-secondary)" }}>&#9881;</button>
      </div>

      {errorMsg && (
        <div className="mb-3 px-3 py-2 rounded-lg text-xs" style={{ background: "#ff453a20", border: "1px solid #ff453a40", color: "#ff453a" }}>
          Transcription failed: {errorMsg}
        </div>
      )}

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
                <div className="flex items-center gap-2">
                  {entry.duration_ms ? (
                    <span className="text-xs font-medium px-1.5 py-0.5 rounded" style={{ background: "var(--border)", color: "var(--text)" }}>
                      {formatDuration(entry.duration_ms)}
                    </span>
                  ) : null}
                  <span className="text-xs" style={{ color: "var(--text-secondary)" }}>
                    {formatTime(entry.timestamp)}
                  </span>
                </div>
                <div className="flex gap-0.5">
                  <IconBtn onClick={() => copyText(entry.text, entry.id)} title="Copy" accent={copied === entry.id}>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <rect x="9" y="9" width="13" height="13" rx="2" />
                      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                    </svg>
                  </IconBtn>
                  {entry.audio_path && (
                    <IconBtn onClick={() => retryEntry(entry.id)} title="Retry">
                      {retrying === entry.id ? (
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ animation: "spin 1s linear infinite" }}>
                          <path d="M21 12a9 9 0 1 1-6.219-8.56" />
                        </svg>
                      ) : (
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                          <polyline points="23 4 23 10 17 10" />
                          <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
                        </svg>
                      )}
                    </IconBtn>
                  )}
                  <IconBtn onClick={() => deleteEntry(entry.id)} title="Delete">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <polyline points="3 6 5 6 21 6" />
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                    </svg>
                  </IconBtn>
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
