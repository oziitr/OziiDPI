#[cfg(target_os = "windows")]
pub fn is_npcap_installed() -> bool {
    use winreg::RegKey;
    use winreg::enums::*;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(_key) = hklm.open_subkey_with_flags("SOFTWARE\\Npcap", KEY_READ) {
        // Double check if the driver service is actually registered.
        // Npcap usually registers a service named 'npcap'.
        // We can just rely on the registry key presence as an initial check.
        // If it exists, they installed it.
        return true;
    }

    // Also check WOW6432Node just in case
    if let Ok(_key) = hklm.open_subkey_with_flags("SOFTWARE\\WOW6432Node\\Npcap", KEY_READ) {
        return true;
    }

    false
}

#[cfg(not(target_os = "windows"))]
pub fn is_npcap_installed() -> bool {
    false // Npcap is Windows only
}
