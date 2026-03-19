use enigo::{Direction, Enigo, Key, Keyboard, Settings};
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

/// Request accessibility permission using AXIsProcessTrustedWithOptions.
/// This adds the app to the Accessibility list and shows a system prompt.
pub fn request_accessibility_with_prompt() -> bool {
    #[cfg(target_os = "macos")]
    {
        if is_accessibility_trusted() {
            return true;
        }

        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;

        extern "C" {
            fn AXIsProcessTrustedWithOptions(options: core_foundation::base::CFTypeRef) -> bool;
        }

        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let value = CFBoolean::true_value();
        let options = CFDictionary::from_CFType_pairs(&[(key, value)]);

        unsafe { AXIsProcessTrustedWithOptions(options.as_CFTypeRef()) }
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Simulate Cmd+V (macOS) or Ctrl+V (Windows/Linux) to paste clipboard content.
/// Must be called from a dedicated OS thread, NOT from tokio async context.
pub fn simulate_paste(app_handle: &AppHandle) -> Result<(), String> {
    // Auto-initialize if not yet done but accessibility is granted
    if app_handle.try_state::<EnigoState>().is_none() {
        if !is_accessibility_trusted() {
            return Err("Accessibility not granted".into());
        }
        let state = EnigoState::new()?;
        app_handle.manage(state);
        log::info!("EnigoState auto-initialized");
    }

    let enigo_state = app_handle
        .try_state::<EnigoState>()
        .ok_or("Enigo not initialized")?;
    let mut enigo = enigo_state
        .0
        .lock()
        .map_err(|e| format!("Failed to lock Enigo: {}", e))?;

    #[cfg(target_os = "macos")]
    let (modifier, v_key) = (Key::Meta, Key::Other(9));

    #[cfg(target_os = "windows")]
    let (modifier, v_key) = (Key::Control, Key::Other(0x56));

    #[cfg(target_os = "linux")]
    let (modifier, v_key) = (Key::Control, Key::Unicode('v'));

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| format!("Failed to press modifier: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(20));
    enigo
        .key(v_key, Direction::Click)
        .map_err(|e| format!("Failed to click V: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(20));
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| format!("Failed to release modifier: {}", e))?;

    log::info!("Paste simulated");
    Ok(())
}
