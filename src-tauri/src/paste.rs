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

/// Check if accessibility permission is granted (macOS)
pub fn is_accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        unsafe { AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Request accessibility with native macOS prompt dialog
/// This uses AXIsProcessTrustedWithOptions with kAXTrustedCheckOptionPrompt=true
/// which triggers the system dialog AND adds the app to the Accessibility list
pub fn request_accessibility_with_prompt() -> bool {
    #[cfg(target_os = "macos")]
    {
        // Don't prompt if already trusted
        if is_accessibility_trusted() {
            return true;
        }

        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;

        extern "C" {
            static kAXTrustedCheckOptionPrompt: *const std::ffi::c_void;
            fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
        }

        unsafe {
            let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt as *const _);
            let dict: CFDictionary<CFString, CFBoolean> =
                CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
            AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as *const _)
        }
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Simulate Cmd+V (macOS) or Ctrl+V (Windows/Linux) to paste clipboard content
pub fn simulate_paste(app_handle: &AppHandle) -> Result<(), String> {
    // Auto-initialize if not yet done but accessibility is granted
    if app_handle.try_state::<EnigoState>().is_none() {
        if !is_accessibility_trusted() {
            return Err("Accessibility not granted".into());
        }
        let state = EnigoState::new()?;
        app_handle.manage(state);
        println!("[NanoWhisper] EnigoState auto-initialized");
    }

    let enigo_state = app_handle
        .try_state::<EnigoState>()
        .ok_or("Enigo not initialized")?;
    let mut enigo = enigo_state.0.lock()
        .map_err(|e| format!("Failed to lock Enigo: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(80));

    #[cfg(target_os = "macos")]
    let (modifier, v_key) = (Key::Meta, Key::Other(9));

    #[cfg(target_os = "windows")]
    let (modifier, v_key) = (Key::Control, Key::Other(0x56));

    #[cfg(target_os = "linux")]
    let (modifier, v_key) = (Key::Control, Key::Unicode('v'));

    enigo.key(modifier, Direction::Press)
        .map_err(|e| format!("Failed to press modifier: {}", e))?;
    enigo.key(v_key, Direction::Click)
        .map_err(|e| format!("Failed to click V: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    enigo.key(modifier, Direction::Release)
        .map_err(|e| format!("Failed to release modifier: {}", e))?;

    println!("[NanoWhisper] Paste simulated");
    Ok(())
}
