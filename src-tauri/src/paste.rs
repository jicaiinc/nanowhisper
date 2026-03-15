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

/// Prompt the user to grant accessibility permission (macOS)
pub fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .output();
        is_accessibility_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Simulate Cmd+V (macOS) or Ctrl+V (Windows/Linux) to paste clipboard content
pub fn simulate_paste(app_handle: &AppHandle) -> Result<(), String> {
    let enigo_state = app_handle
        .try_state::<EnigoState>()
        .ok_or("Enigo not initialized (need Accessibility permission)")?;
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
