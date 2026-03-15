# NanoWhisper 架构设计文档

基于 Handy 项目源码分析 + Tauri 官方 Issue 追踪的调研结果。

---

## 问题 1：macOS 透明窗口拖拽

### 事实发现

**Tauri 已知 Bug（影响 NanoWhisper）：**

| Issue | 描述 |
|-------|------|
| [#13415](https://github.com/tauri-apps/tauri/issues/13415) | `.transparent(true)` 在 macOS bundled 后失效，窗口变白 |
| [#11605](https://github.com/tauri-apps/tauri/issues/11605) | 未聚焦窗口无法用 `data-tauri-drag-region` 拖拽 |
| [#9503](https://github.com/tauri-apps/tauri/issues/9503) | macOS Overlay titleBarStyle 下无法拖动窗口 |
| [#8255](https://github.com/tauri-apps/tauri/issues/8255) | macOS Sonoma 透明窗口失焦后出现渲染异常 |

**Handy 的解决方案（源码引用：`Handy/src-tauri/src/overlay.rs:280-314`）：**

macOS 上使用 `tauri-nspanel` crate 创建 NSPanel 替代普通窗口：

```rust
// Handy overlay.rs:285-304
PanelBuilder::<_, RecordingOverlayPanel>::new(app_handle, "recording_overlay")
    .url(WebviewUrl::App("src/overlay/index.html".into()))
    .level(PanelLevel::Status)           // 浮在所有窗口上方
    .transparent(true)                    // NSPanel 级透明
    .no_activate(true)                    // 不抢焦点
    .has_shadow(false)
    .corner_radius(0.0)
    .with_window(|w| w.decorations(false).transparent(true))
    .collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()        // 所有桌面可见
            .full_screen_auxiliary(),      // 全屏时也显示
    )
    .build()
```

Panel 配置（`overlay.rs:24-31`）：
```rust
tauri_panel! {
    panel!(RecordingOverlayPanel {
        config: {
            can_become_key_window: false,  // 不接收键盘事件
            is_floating_panel: true         // 浮动面板
        }
    })
}
```

**关键发现：Handy 的 overlay 不支持拖拽。** 它通过 `calculate_overlay_position()` 函数（`overlay.rs:197-216`）根据鼠标所在显示器自动计算位置，每次显示时重新定位。Overlay 是固定位置（屏幕顶部或底部居中），无需用户拖动。

**NanoWhisper 当前状态（`nanowhisper/src-tauri/src/lib.rs:186-204`）：**
- 使用普通 `WebviewWindowBuilder`，`decorations(false)` + `always_on_top(true)`
- 没有 `transparent(true)`（所以不存在透明失效问题）
- 使用 `.center()` 定位
- 每次录音都创建新窗口，录完销毁

### 推荐方案

**方案 A（推荐 - 最简单）：不拖拽，自动定位**

参考 Handy，overlay 固定在屏幕顶部居中，无需拖拽：

```rust
// 不用 transparent(true)，用 CSS 实现视觉效果
fn create_overlay(app_handle: &tauri::AppHandle) {
    // 计算屏幕顶部居中位置
    if let Some(monitor) = app_handle.primary_monitor().ok().flatten() {
        let scale = monitor.scale_factor();
        let monitor_width = monitor.size().width as f64 / scale;
        let x = (monitor_width - OVERLAY_WIDTH) / 2.0;
        let y = 46.0; // macOS menu bar 下方

        tauri::WebviewWindowBuilder::new(
            app_handle,
            "overlay",
            tauri::WebviewUrl::App("/src/overlay/index.html".into()),
        )
        .title("")
        .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
        .position(x, y)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .build()
        .ok();
    }
}
```

CSS 端实现视觉透明（不依赖窗口级 transparent）：
```css
html, body {
    background: transparent;
    margin: 0;
    overflow: hidden;
}
.overlay-container {
    background: rgba(0, 0, 0, 0.8);
    border-radius: 20px;
    backdrop-filter: blur(10px);
}
```

**方案 B（如果必须拖拽）：使用 tauri-nspanel**

需要添加依赖：
```toml
[target.'cfg(target_os = "macos")'.dependencies]
tauri-nspanel = { git = "https://github.com/ahkohd/tauri-nspanel", branch = "v2.1" }
```

并在 `lib.rs` 的 builder 中初始化：
```rust
#[cfg(target_os = "macos")]
{
    builder = builder.plugin(tauri_nspanel::init());
}
```

注意：使用 NSPanel 需要启用 `macos-private-api` feature（NanoWhisper 已有）。

### 结论

**推荐方案 A**。NanoWhisper 的 overlay 只是一个录音状态指示器（波形动画），没有理由让用户拖拽它。固定在屏幕顶部居中是最佳体验。如果不使用 `transparent(true)`，则完全规避了 Tauri 的 macOS 透明 bug。

---

## 问题 2：转录后自动粘贴到活跃窗口

### 事实发现

**版本差异：**

| 项目 | enigo 版本 | Send+Sync |
|------|-----------|-----------|
| NanoWhisper | 0.3 | 不支持（macOS `Enigo` 不是 Send+Sync） |
| Handy | 0.6.1 | 支持（0.4.1+ macOS 实现了 Send+Sync） |

**Handy 的 Enigo 架构（源码引用）：**

1. **包装类型**（`Handy/src-tauri/src/input.rs:7-15`）：
```rust
pub struct EnigoState(pub Mutex<Enigo>);

impl EnigoState {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {}", e))?;
        Ok(Self(Mutex::new(enigo)))
    }
}
```

2. **延迟初始化**（`Handy/src-tauri/src/commands/mod.rs:124-156`）：
```rust
// 前端在 onboarding 完成（用户授权 Accessibility）后调用
#[tauri::command]
pub fn initialize_enigo(app: AppHandle) -> Result<(), String> {
    if app.try_state::<EnigoState>().is_some() {
        return Ok(()); // 已初始化
    }
    match EnigoState::new() {
        Ok(enigo_state) => {
            app.manage(enigo_state);  // 注册到 Tauri 状态
            Ok(())
        }
        Err(e) => Err(format!("Failed to initialize input system: {}", e))
    }
}
```

3. **使用时从 state 获取**（`Handy/src-tauri/src/clipboard.rs:608-615`）：
```rust
let enigo_state = app_handle
    .try_state::<EnigoState>()
    .ok_or("Enigo state not initialized")?;
let mut enigo = enigo_state.0.lock()
    .map_err(|e| format!("Failed to lock Enigo: {}", e))?;
```

4. **macOS 粘贴实现**（`Handy/src-tauri/src/input.rs:28-52`）：
```rust
// macOS: Cmd + V（使用 keycode 9 = V 键）
let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9));

enigo.key(modifier_key, enigo::Direction::Press)?;
enigo.key(v_key_code, enigo::Direction::Click)?;
std::thread::sleep(std::time::Duration::from_millis(100));
enigo.key(modifier_key, enigo::Direction::Release)?;
```

**NanoWhisper 当前状态（`nanowhisper/src-tauri/src/paste.rs`）：**
- 每次调用创建新 `Enigo` 实例（不用 managed state）
- 使用相同的键码组合 `Key::Meta + Key::Other(9)`
- 有 `AXIsProcessTrusted()` 权限检查
- enigo 0.3 的 `Enigo` 不是 Send+Sync，**但 NanoWhisper 没有把它放进 managed state，所以当前不会编译报错**

**macOS 权限要求：**
- enigo 模拟按键需要 **Accessibility 权限**
- `AXIsProcessTrusted()` 检查当前进程是否被授权
- 首次使用会弹出系统权限对话框
- Handy 的做法：onboarding 流程中引导用户先授权，然后再初始化 enigo

### 推荐方案

**Step 1：升级 enigo 到 0.6.1**

```toml
# Cargo.toml
enigo = "0.6"  # 从 "0.3" 升级
```

API 变化不大，主要差异：
- `Enigo::new(&Settings::default())` 签名相同
- `Key` 枚举基本兼容
- 0.6 的 `Enigo` 在 macOS 上实现了 Send+Sync

**Step 2：采用 EnigoState 模式**

```rust
// src-tauri/src/paste.rs
use enigo::{Enigo, Key, Keyboard, Settings, Direction};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct EnigoState(pub Mutex<Enigo>);

impl EnigoState {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {}", e))?;
        Ok(Self(Mutex::new(enigo)))
    }
}

pub fn simulate_paste(app_handle: &AppHandle) -> Result<(), String> {
    let enigo_state = app_handle
        .try_state::<EnigoState>()
        .ok_or("Enigo not initialized (need Accessibility permission)")?;
    let mut enigo = enigo_state.0.lock()
        .map_err(|e| format!("Failed to lock Enigo: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(80));

    #[cfg(target_os = "macos")]
    let (modifier, v_key) = (Key::Meta, Key::Other(9));

    enigo.key(modifier, Direction::Press)
        .map_err(|e| format!("Failed to press modifier: {}", e))?;
    enigo.key(v_key, Direction::Click)
        .map_err(|e| format!("Failed to click V: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    enigo.key(modifier, Direction::Release)
        .map_err(|e| format!("Failed to release modifier: {}", e))?;

    Ok(())
}
```

**Step 3：延迟初始化（在 Accessibility 授权后）**

```rust
// src-tauri/src/commands.rs
#[tauri::command]
pub fn initialize_enigo(app: AppHandle) -> Result<(), String> {
    use crate::paste::EnigoState;
    if app.try_state::<EnigoState>().is_some() {
        return Ok(());
    }
    let state = EnigoState::new()?;
    app.manage(state);
    Ok(())
}
```

**Step 4：调用链修改**

```rust
// lib.rs 中 stop_and_transcribe 的改动
let _ = handle.clipboard().write_text(&text);
if let Err(e) = paste::simulate_paste(&handle) {
    eprintln!("[NanoWhisper] Paste failed: {}", e);
}
```

### 依赖变更

```toml
# 变更
enigo = "0.6"  # 从 "0.3" 升级到 "0.6"
```

---

## 问题 3：全局快捷键需按两次才生效

### 事实发现

**已确认的 Tauri Bug：**

| Issue | 描述 | 状态 |
|-------|------|------|
| [#10025](https://github.com/tauri-apps/tauri/issues/10025) | macOS 全局快捷键 handler 触发两次 | Closed (not planned) |
| [#1748](https://github.com/tauri-apps/plugins-workspace/issues/1748) | 注册的快捷键 handler 被调用两次 | Open |

**根因分析（NanoWhisper `lib.rs:137-146`）：**

```rust
let _ = app_handle.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
    if event.state != ShortcutState::Released {
        let h = handle.clone();
        std::thread::spawn(move || {
            toggle_recording(&h);  // 在新线程中 toggle
        });
    }
});
```

**Bug 触发机制：**
1. 用户按下快捷键
2. macOS 上 `on_shortcut` handler 被调用 **两次**（Tauri 已知 bug #10025）
3. 每次调用都 `std::thread::spawn` 一个新线程
4. 第 1 个线程：`toggle_recording` → `is_recording() == false` → 开始录音
5. 第 2 个线程（几毫秒后）：`toggle_recording` → `is_recording() == true` → 停止录音
6. **结果：录音被立即取消，用户看到"没反应"**
7. 第二次按键：同样的事情发生，但因为时序差异，有时能成功

这是一个 **竞态条件 + Tauri double-fire bug** 的组合问题。

**Handy 的解决方案（源码引用）：**

Handy 提供了两个快捷键实现，可运行时切换：

1. **Tauri 实现**（`Handy/src-tauri/src/shortcut/tauri_impl.rs:106-129`）：
   - 使用 `tauri-plugin-global-shortcut`
   - 区分 `ShortcutState::Pressed` 和 `Released`
   - 通过 `handle_shortcut_event` 统一处理

2. **handy-keys 实现**（`Handy/src-tauri/src/shortcut/handy_keys.rs`）：
   - 使用独立的 `handy-keys` crate（`handy-keys = "0.2.4"`）
   - 在专用线程中运行 `HotkeyManager`
   - 通过 mpsc channel 通信
   - 正确区分 `HotkeyState::Pressed` 和 `Released`

3. **统一事件处理**（`Handy/src-tauri/src/shortcut/handler.rs:29-70`）：
```rust
pub fn handle_shortcut_event(
    app: &AppHandle,
    binding_id: &str,
    hotkey_string: &str,
    is_pressed: bool,  // 区分按下和释放
) {
    // 分发给 TranscriptionCoordinator 处理状态转换
    if is_transcribe_binding(binding_id) {
        coordinator.send_input(binding_id, hotkey_string, is_pressed, settings.push_to_talk);
        return;
    }
    // ...
}
```

4. **状态机管理**：Handy 用 `TranscriptionCoordinator` 管理录音状态转换，不使用简单的 toggle，而是通过状态机避免竞态。

### 推荐方案

**方案 A（推荐 - 快速修复）：防抖 + AtomicBool 守卫**

这是最小改动方案，直接解决 double-fire 问题：

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SHORTCUT_PROCESSING: AtomicBool = AtomicBool::new(false);
static LAST_SHORTCUT_TIME: AtomicU64 = AtomicU64::new(0);

const DEBOUNCE_MS: u64 = 500; // 防抖间隔

fn register_shortcut(app_handle: &tauri::AppHandle, settings: &AppSettings) {
    let shortcut: Shortcut = match settings.shortcut.parse() {
        Ok(s) => s,
        Err(e) => { eprintln!("Invalid shortcut: {}", e); return; }
    };

    let handle = app_handle.clone();
    let _ = app_handle.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            // 防抖：忽略 500ms 内的重复事件
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            let last = LAST_SHORTCUT_TIME.load(Ordering::SeqCst);
            if now - last < DEBOUNCE_MS {
                return; // 忽略重复触发
            }
            LAST_SHORTCUT_TIME.store(now, Ordering::SeqCst);

            // CAS 守卫：防止并发 toggle
            if SHORTCUT_PROCESSING.compare_exchange(
                false, true, Ordering::SeqCst, Ordering::SeqCst
            ).is_err() {
                return; // 另一个 toggle 正在进行
            }

            let h = handle.clone();
            std::thread::spawn(move || {
                toggle_recording(&h);
                SHORTCUT_PROCESSING.store(false, Ordering::SeqCst);
            });
        }
    });
}
```

**方案 B（中期）：使用 rdev crate 替代 tauri-plugin-global-shortcut**

Handy 使用了 `rdev` crate（`rdev = { git = "https://github.com/rustdesk-org/rdev" }`）。虽然 Handy 主要用 rdev 做其他事情，但 `rdev` 可以监听全局键盘事件，不受 Tauri 的 double-fire bug 影响。

**方案 C（长期最优）：使用 handy-keys crate**

```toml
handy-keys = "0.2.4"
```

handy-keys 提供更可靠的全局快捷键支持，且正确区分 Pressed/Released。但它引入额外依赖和更大的架构改动，适合后续迭代。

### 结论

**推荐先实施方案 A**（防抖 + 原子守卫），这是 5 行代码的修复。如果问题持续，再考虑方案 B/C。

---

## 依赖变更总结

```toml
# Cargo.toml 需要的变更

[dependencies]
enigo = "0.6"          # 从 "0.3" 升级（解决 Send+Sync + macOS 兼容性）

# 可选（如果选择 NSPanel 方案）
# [target.'cfg(target_os = "macos")'.dependencies]
# tauri-nspanel = { git = "https://github.com/ahkohd/tauri-nspanel", branch = "v2.1" }
```

## 实施优先级

| 优先级 | 问题 | 方案 | 改动量 |
|--------|------|------|--------|
| P0 | 快捷键按两次 | 防抖 + AtomicBool 守卫 | ~10 行 |
| P0 | 自动粘贴 | 升级 enigo + EnigoState 模式 | ~30 行 |
| P1 | Overlay 窗口 | 自动定位，不拖拽 | ~20 行 |
