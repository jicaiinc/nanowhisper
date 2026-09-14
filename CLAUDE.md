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

1. Native hotkey (default: Right Command on macOS / Right Control on Windows, solo tap) triggers `toggle_recording()`
2. **Start**: Creates overlay window → plays start sound → starts `cpal` audio stream → registers Escape for cancel
3. **Stop** (hotkey again): Unregisters Escape → stops recording → encodes WAV (16-bit mono) → calls OpenAI API → clipboard write → closes overlay → waits 350ms → simulate Cmd+V → save to SQLite history
4. **Cancel** (Escape): Stops recording, discards audio, closes overlay

### Data Storage

All persisted to `~/.nanowhisper/`:
- `settings.json` — API key, model, language, shortcut
- `history.db` — SQLite (table: `transcriptions`)
- `audio/` — WAV files (enables retry with different model/settings)

### Key Technical Decisions

- **Shortcut debounce**: 500ms debounce + `AtomicBool` CAS guard to prevent Tauri's known macOS double-fire bug
- **Transparent overlay**: Window uses `.transparent(true)` with semi-transparent background (`rgba(28, 28, 30, 0.92)`)
- **All windows created programmatically** — none defined in `tauri.conf.json`
- **enigo v0.6** wrapped in `Mutex<Enigo>` (`EnigoState`), initialized after Accessibility permission is granted
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

### Auto-Update (`src-tauri/src/updater.rs`)

使用 `tauri-plugin-updater`，基于 GitHub Releases 的 `latest.json` 检测更新。

- **检查时机**：启动后 10 秒首次检查，之后每 4 小时；dev 模式跳过
- **流程**：后台静默 `check()` → `download()` → 存入 `UpdateState` → 前端横幅提示 → 用户点击 Restart → `install()` → `app.restart()`
- **录音保护**：`restart_to_update` 检查 `recorder.is_recording()`，录音中拒绝重启
- **签名**：更新包用 ed25519 密钥签名（`~/.tauri/nanowhisper.key`），公钥在 `tauri.conf.json` 的 `plugins.updater.pubkey`

### CI/CD

GitHub Actions release workflow (`.github/workflows/release.yml`) triggered by `v*` tags. Builds for macOS ARM64, macOS x64, Linux x64.

### Windows 打包与签名（已停止维护，仅作历史参考）

**2026-06-11 起不再维护 Windows**：Trusted Signing 证书身份验证到期（签名 403），决定停止 Windows 支持。Windows 已从 release 矩阵移除，新功能无需考虑 Windows 适配。以下为历史参考。

Tauri v2 的 Windows 打包涉及两套独立的签名体系：

1. **Authenticode 代码签名**（Windows SmartScreen 信任）
   - 通过 Azure Trusted Signing 完成
   - 使用微软官方 `signtool.exe` + `Azure.CodeSigning.Dlib.dll`（来自 NuGet 包 `Microsoft.ArtifactSigning.Client`）
   - CI 中生成 `nanowhisper-sign.cmd` 包装脚本，通过 `bundle.windows.signCommand` 集成到 `tauri build` 流程
   - 认证通过 `AZURE_CLIENT_ID/SECRET/TENANT_ID` 环境变量（EnvironmentCredential）

2. **Tauri updater ed25519 签名**（自动更新完整性验证）
   - 通过 `TAURI_SIGNING_PRIVATE_KEY` 环境变量
   - Tauri 在构建末尾自动生成 `.sig` 文件

**关键顺序约束**：`signCommand` 在 `.sig` 生成之前执行。这意味着 `.sig` 是基于已 Authenticode 签名的文件生成的，两者一致。如果改用 post-build 签名（如 `azure/trusted-signing-action`），Authenticode 会修改文件内容，导致 `.sig` 与实际文件不匹配，updater 验签失败。

**`createUpdaterArtifacts: true`（v2 模式）的 Windows 产物**：
- updater 直接复用 NSIS installer `.exe`（不是 `.nsis.zip`，那是 v1Compatible 模式）
- 产物为 `*-setup.exe` + `*-setup.exe.sig`


# 注意事项
- 【绝对禁止】本地的录音文件和转录文本 json 等配置文件对我极其重要，任何时刻不许删除。