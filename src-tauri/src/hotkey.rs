//! Native single-key hotkey monitoring.
//!
//! Detects a "solo tap" of Right Command (macOS) or Right Control (Windows):
//!   1. Key pressed -> mark pending
//!   2. If any other key pressed while held -> cancel (it's a combo like Cmd+C)
//!   3. Key released within 400ms with no other keys -> trigger callback
//!
//! macOS: Uses NSEvent global/local monitors (runs on NSApplication main RunLoop,
//!        immune to App Nap — the OS keeps the app responsive while monitoring).
//! Windows: Uses SetWindowsHookExW low-level keyboard hook.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// Max duration (ms) between press and release to count as a "solo tap".
const SOLO_TAP_MAX_MS: u64 = 400;
/// Debounce interval (ms) to prevent double-fires.
const DEBOUNCE_MS: u64 = 500;

static CALLBACK: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static DEBOUNCE_LAST: AtomicU64 = AtomicU64::new(0);
static PAUSED: AtomicBool = AtomicBool::new(false);
static MONOTONIC_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn trigger_callback() {
    if PAUSED.load(Ordering::SeqCst) {
        return;
    }
    let now = now_ms();
    let last = DEBOUNCE_LAST.load(Ordering::SeqCst);
    if now.saturating_sub(last) < DEBOUNCE_MS {
        return;
    }
    DEBOUNCE_LAST.store(now, Ordering::SeqCst);

    if let Some(cb) = CALLBACK.get() {
        cb();
    }
}

fn now_ms() -> u64 {
    MONOTONIC_START
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis() as u64
}

/// Temporarily disable the native hotkey (e.g. while capturing a custom shortcut).
pub fn pause() {
    PAUSED.store(true, Ordering::SeqCst);
}

/// Re-enable the native hotkey.
pub fn resume() {
    PAUSED.store(false, Ordering::SeqCst);
}

/// Start the native hotkey monitor.
/// The callback is invoked when a solo tap is detected.
pub fn start(callback: impl Fn() + Send + Sync + 'static) {
    let _ = CALLBACK.set(Box::new(callback));
    platform::start();
}

// ── macOS: NSEvent global + local monitors ───────────────────────────────────
//
// Unlike CGEventTap (which runs on a background thread's CFRunLoop and gets
// throttled by App Nap when the app has no visible windows), NSEvent monitors
// are serviced by the NSApplication main RunLoop. The OS knows the app is
// actively monitoring global events and keeps it responsive.
//
// Two monitors are needed:
//   - Global: fires when OTHER apps are focused
//   - Local:  fires when OUR app is focused (global monitors don't fire for
//             events directed at the monitoring app's own windows)

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use std::ptr::NonNull;

    // NSEventMask values
    const NS_FLAGS_CHANGED_MASK: u64 = 1 << 12;
    const NS_KEY_DOWN_MASK: u64 = 1 << 10;

    // Virtual key code for Right Command
    const K_VK_RIGHT_COMMAND: u16 = 0x36;

    // NSEventModifierFlags — Command key bit
    const NS_COMMAND_KEY_MASK: u64 = 1 << 20;

    static KEY_DOWN: AtomicBool = AtomicBool::new(false);
    static KEY_TIME: AtomicU64 = AtomicU64::new(0);
    static OTHER_KEY: AtomicBool = AtomicBool::new(false);

    /// Shared handler for both global and local monitors.
    /// Works for both flagsChanged and keyDown events:
    ///   - keyCode == 0x36 → Right Command press/release (flagsChanged)
    ///   - any other keyCode while Right Cmd held → cancel solo tap
    fn handle_event(event: &AnyObject) {
        let keycode: u16 = unsafe { msg_send![event, keyCode] };
        let flags: u64 = unsafe { msg_send![event, modifierFlags] };

        if keycode == K_VK_RIGHT_COMMAND {
            let cmd_down = (flags & NS_COMMAND_KEY_MASK) != 0;
            if cmd_down {
                // Right Command pressed
                if !KEY_DOWN.swap(true, Ordering::SeqCst) {
                    KEY_TIME.store(now_ms(), Ordering::SeqCst);
                    OTHER_KEY.store(false, Ordering::SeqCst);
                }
            } else if KEY_DOWN.swap(false, Ordering::SeqCst) {
                // Right Command released — check for solo tap
                let held = now_ms().saturating_sub(KEY_TIME.load(Ordering::SeqCst));
                if !OTHER_KEY.load(Ordering::SeqCst) && held < SOLO_TAP_MAX_MS {
                    trigger_callback();
                }
            }
        } else if KEY_DOWN.load(Ordering::SeqCst) {
            // Another key/modifier pressed while Right Cmd held → not a solo tap
            OTHER_KEY.store(true, Ordering::SeqCst);
        }
    }

    pub fn start() {
        // flagsChanged: detects modifier key press/release (no special permissions)
        // keyDown: detects regular keys pressed during hold (needs Accessibility)
        let mask: u64 = NS_FLAGS_CHANGED_MASK | NS_KEY_DOWN_MASK;

        // Global monitor: fires when OTHER apps are focused
        let global_block = RcBlock::new(|event: NonNull<AnyObject>| {
            handle_event(unsafe { event.as_ref() });
        });

        // Local monitor: fires when OUR app is focused; returns event to pass through
        let local_block = RcBlock::new(|event: NonNull<AnyObject>| -> *mut AnyObject {
            handle_event(unsafe { event.as_ref() });
            event.as_ptr()
        });

        unsafe {
            let cls = AnyClass::get(c"NSEvent").expect("NSEvent class not found");

            let _: *mut AnyObject = msg_send![
                cls,
                addGlobalMonitorForEventsMatchingMask: mask,
                handler: &*global_block
            ];

            let _: *mut AnyObject = msg_send![
                cls,
                addLocalMonitorForEventsMatchingMask: mask,
                handler: &*local_block
            ];
        }

        // Leak blocks to keep monitors alive for the app's lifetime
        std::mem::forget(global_block);
        std::mem::forget(local_block);

        log::info!("Native hotkey started (Right Command via NSEvent monitors)");
    }
}

// ── Windows: Low-level keyboard hook ─────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::ffi::c_void;
    use std::sync::atomic::AtomicPtr;
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
        WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    const VK_RCONTROL: u32 = 0xA3;

    static KEY_DOWN: AtomicBool = AtomicBool::new(false);
    static KEY_TIME: AtomicU64 = AtomicU64::new(0);
    static OTHER_KEY: AtomicBool = AtomicBool::new(false);
    static HOOK: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

    unsafe extern "system" fn hook_proc(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
        if code >= 0 {
            let kbd = *(l_param as *const KBDLLHOOKSTRUCT);
            let vk = kbd.vkCode;
            let is_down = w_param == WM_KEYDOWN as usize || w_param == WM_SYSKEYDOWN as usize;
            let is_up = w_param == WM_KEYUP as usize || w_param == WM_SYSKEYUP as usize;

            if vk == VK_RCONTROL {
                if is_down && !KEY_DOWN.load(Ordering::SeqCst) {
                    KEY_DOWN.store(true, Ordering::SeqCst);
                    KEY_TIME.store(now_ms(), Ordering::SeqCst);
                    OTHER_KEY.store(false, Ordering::SeqCst);
                } else if is_up && KEY_DOWN.swap(false, Ordering::SeqCst) {
                    let held = now_ms().saturating_sub(KEY_TIME.load(Ordering::SeqCst));
                    if !OTHER_KEY.load(Ordering::SeqCst) && held < SOLO_TAP_MAX_MS {
                        trigger_callback();
                    }
                }
            } else if is_down && KEY_DOWN.load(Ordering::SeqCst) {
                OTHER_KEY.store(true, Ordering::SeqCst);
            }
        }

        let h = HOOK.load(Ordering::SeqCst);
        unsafe { CallNextHookEx(h, code, w_param, l_param) }
    }

    pub fn start() {
        std::thread::spawn(|| unsafe {
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(hook_proc),
                std::ptr::null_mut(),
                0,
            );
            if hook.is_null() {
                log::error!("Failed to install keyboard hook");
                return;
            }
            HOOK.store(hook, Ordering::SeqCst);
            log::info!("Native hotkey started (Right Control)");

            // Message pump: required for low-level keyboard hook to receive events.
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
        });
    }
}

// ── Linux: no-op (use global_shortcut fallback) ──────────────────────────────

#[cfg(target_os = "linux")]
mod platform {
    pub fn start() {
        log::info!("Native hotkey not available on Linux; use global shortcut");
    }
}
