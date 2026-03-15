use enigo::{Enigo, Key, Keyboard, Settings, Direction};

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
pub fn simulate_paste() {
    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("[NanoWhisper] Failed to create enigo: {}", e);
            return;
        }
    };

    std::thread::sleep(std::time::Duration::from_millis(80));

    #[cfg(target_os = "macos")]
    let (modifier, v_key) = (Key::Meta, Key::Other(9));

    #[cfg(target_os = "windows")]
    let (modifier, v_key) = (Key::Control, Key::Other(0x56));

    #[cfg(target_os = "linux")]
    let (modifier, v_key) = (Key::Control, Key::Unicode('v'));

    if let Err(e) = enigo.key(modifier, Direction::Press) {
        eprintln!("[NanoWhisper] Failed to press modifier: {}", e);
        return;
    }
    if let Err(e) = enigo.key(v_key, Direction::Click) {
        eprintln!("[NanoWhisper] Failed to click V: {}", e);
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    let _ = enigo.key(modifier, Direction::Release);

    println!("[NanoWhisper] Paste simulated");
}
