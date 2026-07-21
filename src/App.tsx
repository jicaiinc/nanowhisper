import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { HistoryEntry, AppSettings, StorageStats } from "./types";
import logoUrl from "./assets/logo.png";

type View = "onboarding" | "history" | "settings";

const isMac = navigator.userAgent.includes("Mac");
const modKey = isMac ? "⌘" : "Ctrl";

const defaultHotkey = isMac ? "Right ⌘" : "Right Ctrl";

const OPENAI_MODELS = [
  { value: "gpt-4o-transcribe", label: "gpt-4o-transcribe" },
  { value: "gpt-4o-mini-transcribe", label: "gpt-4o-mini-transcribe" },
  { value: "whisper-1", label: "whisper-1" },
];
const GEMINI_MODELS = [
  { value: "gemini-3.6-flash", label: "Gemini 3.6 Flash" },
  { value: "gemini-3-flash-preview", label: "Gemini 3 Flash" },
  { value: "gemini-3.1-pro-preview", label: "Gemini 3.1 Pro" },
];
const PROVIDER_CUSTOM_OPENAI = "custom_openai";
const DEFAULT_MODELS: Record<string, string> = {
  openai: "gpt-4o-transcribe",
  gemini: "gemini-3-flash-preview",
  [PROVIDER_CUSTOM_OPENAI]: "whisper-1",
};

const fieldStyle = {
  background: "var(--card)",
  border: "1px solid var(--border)",
  color: "var(--text)",
};

function isGeminiProvider(provider: string): boolean {
  return provider === "gemini";
}

function isCustomOpenAIProvider(provider: string): boolean {
  return provider === PROVIDER_CUSTOM_OPENAI;
}

function activeApiKey(settings: AppSettings): string {
  if (isGeminiProvider(settings.provider)) return settings.gemini_api_key;
  if (isCustomOpenAIProvider(settings.provider)) return settings.custom_api_key;
  return settings.api_key;
}

function canTestConnection(settings: AppSettings): boolean {
  if (!activeApiKey(settings).trim() || !settings.model.trim()) return false;
  if (isCustomOpenAIProvider(settings.provider) && !settings.custom_base_url.trim()) return false;
  if (settings.proxy_mode === "custom" && !settings.proxy_url.trim()) return false;
  return true;
}

function providerKeyLabel(provider: string): string {
  if (isGeminiProvider(provider)) return "Gemini API Key";
  if (isCustomOpenAIProvider(provider)) return "API Key";
  return "OpenAI API Key";
}

function providerKeyPlaceholder(provider: string): string {
  if (isGeminiProvider(provider)) return "AIza...";
  if (isCustomOpenAIProvider(provider)) return "sk-...";
  return "sk-proj-...";
}

function ConnectionSettings({
  settings,
  onChange,
  onTest,
  apiKeyStatus,
  apiKeyError,
  primaryTestButton,
}: {
  settings: AppSettings;
  onChange: (patch: Partial<AppSettings>) => void;
  onTest: () => void;
  apiKeyStatus: "untested" | "testing" | "ok" | "error";
  apiKeyError: string | null;
  primaryTestButton?: boolean;
}) {
  const key = activeApiKey(settings);
  const testable = canTestConnection(settings);

  const updateProvider = (provider: string) => {
    onChange({
      provider,
      model: DEFAULT_MODELS[provider] || "whisper-1",
    });
  };

  const updateApiKey = (apiKey: string) => {
    if (isGeminiProvider(settings.provider)) {
      onChange({ gemini_api_key: apiKey });
    } else if (isCustomOpenAIProvider(settings.provider)) {
      onChange({ custom_api_key: apiKey });
    } else {
      onChange({ api_key: apiKey });
    }
  };

  return (
    <div className="space-y-3">
      <div>
        <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Provider</label>
        <select
          value={settings.provider}
          onChange={(e) => updateProvider(e.target.value)}
          className="w-full px-3 py-2 rounded-lg text-sm outline-none"
          style={fieldStyle}
        >
          <option value="openai">OpenAI</option>
          <option value="gemini">Gemini</option>
          <option value={PROVIDER_CUSTOM_OPENAI}>OpenAI-compatible</option>
        </select>
      </div>

      {isCustomOpenAIProvider(settings.provider) && (
        <div>
          <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Base URL</label>
          <input
            type="url"
            value={settings.custom_base_url}
            onChange={(e) => onChange({ custom_base_url: e.target.value })}
            placeholder="https://api.example.com/v1"
            className="w-full px-3 py-2 rounded-lg text-sm outline-none"
            style={fieldStyle}
          />
        </div>
      )}

      <div>
        <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>{providerKeyLabel(settings.provider)}</label>
        <input
          type="password"
          value={key}
          onChange={(e) => updateApiKey(e.target.value)}
          placeholder={providerKeyPlaceholder(settings.provider)}
          className="w-full px-3 py-2 rounded-lg text-sm outline-none"
          style={fieldStyle}
        />
      </div>

      <div>
        <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Model</label>
        {isCustomOpenAIProvider(settings.provider) ? (
          <input
            type="text"
            value={settings.model}
            onChange={(e) => onChange({ model: e.target.value })}
            placeholder="whisper-1"
            className="w-full px-3 py-2 rounded-lg text-sm outline-none"
            style={fieldStyle}
          />
        ) : (
          <select
            value={settings.model}
            onChange={(e) => onChange({ model: e.target.value })}
            className="w-full px-3 py-2 rounded-lg text-sm outline-none"
            style={fieldStyle}
          >
            {(isGeminiProvider(settings.provider) ? GEMINI_MODELS : OPENAI_MODELS).map((m) => (
              <option key={m.value} value={m.value}>{m.label}</option>
            ))}
          </select>
        )}
      </div>

      <div>
        <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Network</label>
        <select
          value={settings.proxy_mode}
          onChange={(e) => onChange({ proxy_mode: e.target.value })}
          className="w-full px-3 py-2 rounded-lg text-sm outline-none"
          style={fieldStyle}
        >
          <option value="system">System Proxy</option>
          <option value="disabled">No Proxy</option>
          <option value="custom">Custom Proxy</option>
        </select>
      </div>

      {settings.proxy_mode === "custom" && (
        <div>
          <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Proxy URL</label>
          <input
            type="url"
            value={settings.proxy_url}
            onChange={(e) => onChange({ proxy_url: e.target.value })}
            placeholder="socks5://127.0.0.1:1080"
            className="w-full px-3 py-2 rounded-lg text-sm outline-none"
            style={fieldStyle}
          />
        </div>
      )}

      <button
        onClick={onTest}
        disabled={!testable || apiKeyStatus === "testing"}
        className={`w-full px-3 rounded-lg font-medium ${primaryTestButton ? "py-2 text-sm" : "py-1.5 text-xs"}`}
        style={{
          background: apiKeyStatus === "ok" ? "#34c75920" : primaryTestButton ? "var(--accent)" : "var(--border)",
          color: apiKeyStatus === "ok" ? "#34c759" : primaryTestButton ? "white" : "var(--text)",
          cursor: !testable || apiKeyStatus === "testing" ? "not-allowed" : "pointer",
          opacity: !testable || apiKeyStatus === "testing" ? 0.5 : 1,
        }}
      >
        {apiKeyStatus === "testing" ? "Testing..." : apiKeyStatus === "ok" ? "Connected" : "Test Connection"}
      </button>
      {apiKeyStatus === "error" && apiKeyError && (
        <p className="text-xs" style={{ color: "#ff453a" }}>{apiKeyError}</p>
      )}
    </div>
  );
}

function displayShortcut(s: string): string {
  if (!s) return defaultHotkey;
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
  const [apiKeyStatus, setApiKeyStatus] = useState<"untested" | "testing" | "ok" | "error">("untested");
  const [apiKeyError, setApiKeyError] = useState<string | null>(null);
  const [updateVersion, setUpdateVersion] = useState<string | null>(null);
  const [updateBlocked, setUpdateBlocked] = useState(false);
  const [storageStats, setStorageStats] = useState<StorageStats | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearing, setClearing] = useState(false);

  const loadHistory = useCallback(async () => {
    const entries = await invoke<HistoryEntry[]>("get_history");
    setHistory(entries);
  }, []);

  const loadSettings = useCallback(async () => {
    const s = await invoke<AppSettings>("get_settings");
    setSettings(s);
    const key = activeApiKey(s);
    if (!key) setView("onboarding");
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
    const unlisten3 = listen<{ version: string }>("update-downloaded", (e) => {
      setUpdateVersion(e.payload.version);
    });
    return () => {
      unlisten1.then((f) => f());
      unlisten2.then((f) => f());
      unlisten3.then((f) => f());
    };
  }, [loadHistory, loadSettings, checkPermissions]);

  // Refresh storage stats whenever the Settings view opens
  useEffect(() => {
    if (view !== "settings") return;
    invoke<StorageStats>("get_storage_stats").then(setStorageStats).catch(() => {});
  }, [view]);

  const handleClearAll = async () => {
    setClearing(true);
    try {
      await invoke("clear_history");
      await loadHistory();
      const s = await invoke<StorageStats>("get_storage_stats");
      setStorageStats(s);
    } catch (e) {
      console.error("Clear history failed:", e);
    }
    setClearing(false);
    setConfirmClear(false);
  };

  const formatBytes = (b: number) => {
    if (b < 1024) return `${b} B`;
    if (b < 1024 ** 2) return `${(b / 1024).toFixed(0)} KB`;
    if (b < 1024 ** 3) return `${(b / 1024 ** 2).toFixed(1)} MB`;
    return `${(b / 1024 ** 3).toFixed(2)} GB`;
  };

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

  const updateConnectionSettings = (patch: Partial<AppSettings>) => {
    setSettings((current) => current ? { ...current, ...patch } : current);
    setApiKeyStatus("untested");
    setApiKeyError(null);
  };

  const testConnection = async () => {
    if (!settings || !canTestConnection(settings)) return;
    setApiKeyStatus("testing");
    setApiKeyError(null);
    try {
      await invoke("validate_api_key", { settings });
      setApiKeyStatus("ok");
    } catch (e) {
      setApiKeyStatus("error");
      setApiKeyError(String(e));
    }
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
    const canProceed = apiKeyStatus === "ok" && microphoneOk && (isMac ? accessibilityOk : true);
    return (
      <div className="p-6 max-w-md mx-auto">
        <div className="flex flex-col items-center mb-6">
          <div className="flex items-center gap-2 mb-1">
            <img src={logoUrl} alt="" width={28} height={28} />
            <h1 className="text-xl font-semibold">NanoWhisper</h1>
          </div>
          <p className="text-xs" style={{ color: "var(--text-secondary)" }}>Speech to text, simply.</p>
        </div>

        <div className="space-y-5">
          {/* Step 1: Speech Provider */}
          <div>
            <div className="flex items-center gap-2 mb-2">
              <span className="text-xs font-medium px-2 py-0.5 rounded-full" style={{ background: "var(--accent)", color: "white" }}>1</span>
              <span className="text-sm font-medium">Speech Provider</span>
              {apiKeyStatus === "ok" && <span style={{ color: "#34c759" }}>&#10003;</span>}
            </div>
            <ConnectionSettings
              settings={settings}
              onChange={updateConnectionSettings}
              onTest={testConnection}
              apiKeyStatus={apiKeyStatus}
              apiKeyError={apiKeyError}
              primaryTestButton
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
            <p className="text-xs mt-1" style={{ color: "var(--text-secondary)" }}>Required for auto-paste and keyboard shortcut.</p>
          </div>

          {/* Step 4: Shortcut */}
          <div>
            <div className="flex items-center gap-2 mb-2">
              <span className="text-xs font-medium px-2 py-0.5 rounded-full" style={{ background: "var(--border)", color: "var(--text-secondary)" }}>4</span>
              <span className="text-sm font-medium">Shortcut</span>
            </div>
            <p className="text-xs mb-1.5" style={{ color: "var(--text-secondary)" }}>
              Default: {defaultHotkey}. Press to record, again to stop. Escape to cancel.
            </p>
            <ShortcutInput shortcut={settings.shortcut} onCapture={(s) => setSettings({ ...settings, shortcut: s })} />
            <p className="text-xs mt-1" style={{ color: "var(--text-secondary)" }}>Optionally set a custom shortcut above.</p>
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
          <ConnectionSettings
            settings={settings}
            onChange={updateConnectionSettings}
            onTest={testConnection}
            apiKeyStatus={apiKeyStatus}
            apiKeyError={apiKeyError}
          />
          <div>
            <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Language</label>
            <select value={settings.language} onChange={(e) => setSettings({ ...settings, language: e.target.value })} className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={{ background: "var(--card)", border: "1px solid var(--border)", color: "var(--text)" }}>
              <option value="auto">Auto Detect</option>
              <option value="zh-Hans">Chinese (Simplified)</option>
              <option value="zh-Hant">Chinese (Traditional)</option>
              <option value="en">English</option>
              <option value="ja">Japanese</option>
              <option value="ko">Korean</option>
              <option value="es">Spanish</option>
              <option value="fr">French</option>
              <option value="de">German</option>
            </select>
          </div>
          <div>
            <div className="flex items-center justify-between">
              <label className="block text-xs" style={{ color: "var(--text-secondary)" }}>Sound Effects</label>
              <button
                onClick={() => setSettings({ ...settings, sound_enabled: !settings.sound_enabled })}
                className="relative w-10 h-5 rounded-full transition-colors"
                style={{ background: settings.sound_enabled ? "var(--accent)" : "var(--border)" }}
              >
                <span
                  className="absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform"
                  style={{ left: settings.sound_enabled ? "calc(100% - 18px)" : "2px" }}
                />
              </button>
            </div>
          </div>
          {isMac && (
            <div>
              <div className="flex items-center justify-between">
                <label className="block text-xs" style={{ color: "var(--text-secondary)" }}>Auto-Restore Mic Volume</label>
                <button
                  onClick={() => setSettings({ ...settings, mic_min_gain: settings.mic_min_gain > 0 ? 0 : 85 })}
                  className="relative w-10 h-5 rounded-full transition-colors"
                  style={{ background: settings.mic_min_gain > 0 ? "var(--accent)" : "var(--border)" }}
                >
                  <span
                    className="absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform"
                    style={{ left: settings.mic_min_gain > 0 ? "calc(100% - 18px)" : "2px" }}
                  />
                </button>
              </div>
              <p className="text-xs mt-1" style={{ color: "var(--text-secondary)", opacity: 0.7 }}>
                Raises system mic input gain at recording start when another app (e.g. WeChat) lowered it
              </p>
            </div>
          )}
          <div>
            <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Shortcut</label>
            <p className="text-xs mb-1.5" style={{ color: "var(--text-secondary)" }}>
              Default: {defaultHotkey}
            </p>
            <ShortcutInput shortcut={settings.shortcut} onCapture={(s) => setSettings({ ...settings, shortcut: s })} />
            {settings.shortcut && (
              <button
                onClick={() => setSettings({ ...settings, shortcut: "" })}
                className="text-xs mt-1"
                style={{ color: "var(--accent)" }}
              >
                Reset to default
              </button>
            )}
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
          <div>
            <label className="block text-xs mb-1" style={{ color: "var(--text-secondary)" }}>Storage</label>
            <div className="px-3 py-2 rounded-lg text-sm flex items-center justify-between" style={fieldStyle}>
              <span style={{ color: "var(--text-secondary)" }}>
                {storageStats
                  ? `${storageStats.count} transcription${storageStats.count === 1 ? "" : "s"} · ${formatBytes(storageStats.audio_bytes)}`
                  : "Calculating..."}
              </span>
              {storageStats !== null && storageStats.count > 0 && (
                <button
                  onClick={() => setConfirmClear(true)}
                  className="text-xs font-medium"
                  style={{ color: "#ff453a" }}
                >
                  Clear All...
                </button>
              )}
            </div>
          </div>
          <p className="text-center text-xs pt-2" style={{ color: "var(--text-secondary)", opacity: 0.5 }}>v{__APP_VERSION__}</p>
        </div>

        {confirmClear && (
          <div className="fixed inset-0 flex items-center justify-center" style={{ background: "rgba(0, 0, 0, 0.4)", zIndex: 50 }}>
            <div className="rounded-xl p-4 mx-6 w-full" style={{ background: "var(--bg)", border: "1px solid var(--border)", maxWidth: "300px" }}>
              <h2 className="text-sm font-semibold mb-1">Delete all transcriptions?</h2>
              <p className="text-xs mb-4" style={{ color: "var(--text-secondary)" }}>
                This deletes {storageStats?.count} transcriptions and {formatBytes(storageStats?.audio_bytes ?? 0)} of audio files. This cannot be undone. Your settings are not affected.
              </p>
              <div className="flex gap-2">
                <button
                  onClick={() => setConfirmClear(false)}
                  disabled={clearing}
                  className="flex-1 py-2 rounded-lg text-sm font-medium"
                  style={{ background: "var(--border)", color: "var(--text)" }}
                >
                  Cancel
                </button>
                <button
                  onClick={handleClearAll}
                  disabled={clearing}
                  className="flex-1 py-2 rounded-lg text-sm font-medium"
                  style={{ background: "#ff453a", color: "white", opacity: clearing ? 0.6 : 1 }}
                >
                  {clearing ? "Deleting..." : "Delete All"}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    );
  }

  // History
  return (
    <div className="flex flex-col h-screen max-w-md mx-auto">
      <div className="flex items-center justify-between p-4 pb-2" style={{ background: "var(--bg)" }}>
        <div className="flex items-center gap-2">
          <img src={logoUrl} alt="" width={24} height={24} />
          <h1 className="text-lg font-semibold">NanoWhisper</h1>
        </div>
        <button onClick={() => setView("settings")} className="text-xl px-1" style={{ color: "var(--text-secondary)" }}>&#9881;</button>
      </div>

      {updateVersion && (
        <div className="mx-4 mb-0 mt-1 px-3 py-2 rounded-lg text-xs flex items-center justify-between"
          style={{ background: "#34c75920", border: "1px solid #34c75940", color: "#34c759" }}>
          <span>v{updateVersion} ready</span>
          <button onClick={async () => {
            try {
              await invoke("restart_to_update");
            } catch (e) {
              if (String(e).includes("Recording")) {
                setUpdateBlocked(true);
                setTimeout(() => setUpdateBlocked(false), 3000);
              } else {
                setErrorMsg(String(e));
                setTimeout(() => setErrorMsg(null), 5000);
              }
            }
          }} className="font-medium" style={{ color: "#34c759" }}>
            {updateBlocked ? "Recording..." : "Restart to update"}
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-4 pb-4 pt-2">
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
          {history.map((entry) => {
            const isFailed = entry.text.startsWith("[Error:");
            return (
            <div key={entry.id} className="rounded-lg p-3" style={{ background: "var(--card)", border: isFailed ? "1px solid #ff453a40" : "1px solid var(--border)" }}>
              {isFailed ? (
                <div className="text-xs" style={{ color: "#ff453a" }}>
                  {entry.text.slice(1, -1)}
                </div>
              ) : (
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
              )}
              <div className="flex items-center justify-between mt-2">
                <div className="flex items-center gap-2">
                  {entry.duration_ms ? (
                    <span className="text-xs font-medium px-1.5 py-0.5 rounded" style={{ background: "var(--border)", color: "var(--text)" }}>
                      {formatDuration(entry.duration_ms)}
                    </span>
                  ) : null}
                  <span className="text-xs" style={{ color: "var(--text-secondary)" }}>
                    {formatTime(entry.started_at ? Math.floor(entry.started_at / 1000) : entry.timestamp)}
                  </span>
                </div>
                <div className="flex gap-0.5">
                  {!isFailed && (
                  <IconBtn onClick={() => copyText(entry.text, entry.id)} title="Copy" accent={copied === entry.id}>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <rect x="9" y="9" width="13" height="13" rx="2" />
                      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                    </svg>
                  </IconBtn>
                  )}
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
          );
          })}
        </div>
      )}
      </div>
    </div>
  );
}

export default App;
