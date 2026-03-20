# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

NanoWhisper is a minimal desktop speech-to-text app built with **Tauri v2** (Rust backend + React/TypeScript frontend). It captures microphone audio, sends it to OpenAI Whisper API, and auto-pastes the transcribed text into the active application.

## Development Commands

```bash
# Run in development mode (starts Vite dev server + Tauri native app)
npm run tauri dev

# Production build
npm run tauri build

# Frontend only (Vite dev server on port 1420)
npm run dev

# Type-check + bundle frontend
npm run build
```

No tests or linting are configured.

## Architecture

### Two-Process Model (Tauri v2)

- **Rust backend** (`src-tauri/src/`): Audio capture, OpenAI API calls, SQLite history, keyboard simulation, system tray, global shortcuts
- **Web frontend** (`src/`): React UI for settings, history, and recording overlay

### Two-Window Design

- **Main window** (`src/App.tsx`): Settings, history list, onboarding. Hides on close (tray app pattern).
- **Overlay window** (`src/overlay/`): Decorationless, always-on-top waveform visualization. Created/destroyed dynamically per recording session.

### Backend Modules (`src-tauri/src/`)

| File | Responsibility |
|------|---------------|
| `lib.rs` | Core app logic: window management, shortcut registration, recording flow orchestration |
| `commands.rs` | Tauri IPC command handlers (bridge between frontend and backend) |
| `recorder.rs` | Audio recording via `cpal` on a dedicated thread, real-time RMS events |
| `transcribe.rs` | OpenAI transcription API client (multipart form upload) |
| `history.rs` | SQLite storage with `rusqlite_migration` |
| `settings.rs` | JSON settings persistence |
| `paste.rs` | Auto-paste via `enigo` keyboard simulation, macOS Accessibility FFI |

### Recording Flow

1. Global shortcut (default: `Cmd+Shift+Space`) triggers `toggle_recording()`
2. **Start**: Creates overlay window → starts `cpal` audio stream → registers Escape for cancel
3. **Stop** (shortcut again): Stops recording → encodes WAV (16-bit mono) → calls OpenAI API → clipboard write → simulate Cmd+V → save to SQLite history → close overlay
4. **Cancel** (Escape): Stops recording, discards audio, closes overlay

### Data Storage

All persisted to `~/.nanowhisper/`:
- `settings.json` — API key, model, language, shortcut
- `history.db` — SQLite (table: `transcriptions`)
- `audio/` — WAV files (enables retry with different model/settings)

### Key Technical Decisions

- **Shortcut debounce**: 500ms debounce + `AtomicBool` CAS guard to prevent Tauri's known macOS double-fire bug
- **Opaque overlay**: Solid dark background (`#1c1c1e`) instead of window transparency, avoiding known Tauri macOS transparency bugs
- **All windows created programmatically** — none defined in `tauri.conf.json`
- **enigo v0.6** wrapped in `Mutex<Option<Enigo>>` (`EnigoState`), lazily initialized after Accessibility permission is granted
- `.env` loaded via `dotenvy` for dev convenience (gitignored)

### Frontend Stack

- React 18 + TypeScript (strict mode)
- Tailwind CSS v4 + custom CSS variables for light/dark theme
- Vite v6 with multi-entry build (main + overlay)
- Tauri IPC via `@tauri-apps/api` `invoke()` and `listen()`

### App Icons

Logo 源文件在 `src-tauri/logo/`：

- **macOS**: 使用 `appicon.png`（白底圆角），生成 `icons/icon.icns` 及各尺寸 PNG
- **Windows**: 使用 `appicon0.png`（透明背景），生成 `icons/icon.ico`（需包含 16/32/48/256 多尺寸）

两套图标独立，修改一方不影响另一方。

### CI/CD

GitHub Actions release workflow (`.github/workflows/release.yml`) triggered by `v*` tags. Builds for macOS ARM64, macOS x64, Linux x64, Windows x64.
