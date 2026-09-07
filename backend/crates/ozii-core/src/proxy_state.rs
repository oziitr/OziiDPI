#![allow(clippy::collapsible_if)]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OriginalProxyState {
    pub proxy_enable: Option<u32>,
    pub proxy_server: Option<String>,
    pub proxy_override: Option<String>,
    pub auto_config_url: Option<String>,
    pub wpad_enabled: Option<bool>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("Failed to read registry: {0}")]
    ReadError(String),
    #[error("Failed to write registry: {0}")]
    WriteError(String),
    #[error("Existing PAC configuration conflict detected: {0}")]
    ExistingPacConfigured(String),
    #[error("WPAD / AutoDetect configuration conflict detected. Unsafe to activate.")]
    WpadConflict,
}

/// Runtime override for the WPAD auto-detect conflict safety check.
/// Set from the persisted user preference (config.ignore_wpad); the
/// OZIIDPI_IGNORE_WPAD env var remains supported for CLI/test tooling.
pub static WPAD_IGNORE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn wpad_ignore_requested() -> bool {
    WPAD_IGNORE.load(std::sync::atomic::Ordering::SeqCst)
        || std::env::var("OZIIDPI_IGNORE_WPAD").is_ok()
}

/// Abstract trait to isolate Windows Registry modifications for testing safely
pub trait SystemProxy {
    /// Reads the current state of Windows proxy settings.
    fn read_original(&self) -> Result<OriginalProxyState, ProxyError>;

    /// Sets the Windows AutoConfigURL to point to our local PAC server.
    /// Returns the OriginalProxyState that was overwritten.
    fn set_pac(&self, pac_url: &str, proxy_server: &str) -> Result<OriginalProxyState, ProxyError>;

    /// Restores the exact state of the Windows proxy settings from the provided backup.
    fn restore_original(&self, state: &OriginalProxyState) -> Result<(), ProxyError>;

    /// Broadcasts the INTERNET_OPTION_SETTINGS_CHANGED event to Windows
    fn notify_system(&self);
}

#[cfg(target_os = "windows")]
pub struct RealSystemProxy;

#[cfg(target_os = "windows")]
impl SystemProxy for RealSystemProxy {
    fn read_original(&self) -> Result<OriginalProxyState, ProxyError> {
        use winapi::um::winbase::GlobalFree;
        use winapi::um::winhttp::{
            WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WinHttpGetIEProxyConfigForCurrentUser,
        };
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
            .map_err(|e| ProxyError::ReadError(e.to_string()))?;

        let proxy_enable = key.get_value("ProxyEnable").ok();
        let proxy_server = key.get_value("ProxyServer").ok();
        let proxy_override = key.get_value("ProxyOverride").ok();
        let auto_config_url = key.get_value("AutoConfigURL").ok();

        // Inspect WinHTTP config for WPAD
        let mut wpad_enabled = None;
        let mut config: WINHTTP_CURRENT_USER_IE_PROXY_CONFIG = unsafe { std::mem::zeroed() };
        let success = unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut config) };
        if success != 0 {
            wpad_enabled = Some(config.fAutoDetect != 0);
            unsafe {
                if !config.lpszProxy.is_null() {
                    GlobalFree(config.lpszProxy as *mut _);
                }
                if !config.lpszProxyBypass.is_null() {
                    GlobalFree(config.lpszProxyBypass as *mut _);
                }
                if !config.lpszAutoConfigUrl.is_null() {
                    GlobalFree(config.lpszAutoConfigUrl as *mut _);
                }
            }
        }

        let mut state = OriginalProxyState {
            proxy_enable,
            proxy_server,
            proxy_override,
            auto_config_url,
            wpad_enabled,
        };

        // A PAC pointing at 127.0.0.1/localhost is ALWAYS residual OziiDPI
        // state from a previous (possibly crashed) activation. Treat it as
        // "no proxy configured" so activation never re-persists it and crash
        // recovery never restores a dead proxy â€” the exact condition that
        // leaves browsers stuck with "no internet".
        if let Some(url) = state.auto_config_url.as_deref() {
            if url.contains("127.0.0.1") || url.contains("localhost") {
                state.auto_config_url = None;
                state.proxy_enable = Some(0);
                state.proxy_server = None;
            }
        }

        Ok(state)
    }

    fn set_pac(&self, pac_url: &str, proxy_server: &str) -> Result<OriginalProxyState, ProxyError> {
        use winreg::RegKey;
        use winreg::enums::*;

        let original = self.read_original()?;

        if let Some(true) = original.wpad_enabled {
            if !wpad_ignore_requested() {
                return Err(ProxyError::WpadConflict);
            } else {
                println!("[!] WARNING: Bypassing WPAD conflict due to user preference");
            }
        }

        if let Some(ref existing_pac) = original.auto_config_url {
            if !existing_pac.is_empty() && !existing_pac.contains("127.0.0.1") {
                return Err(ProxyError::ExistingPacConfigured(existing_pac.clone()));
            }
        }

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
            .map_err(|e| ProxyError::WriteError(e.to_string()))?;

        key.set_value("AutoConfigURL", &pac_url.to_string())
            .map_err(|e| ProxyError::WriteError(e.to_string()))?;

        key.set_value("ProxyEnable", &1u32)
            .map_err(|e| ProxyError::WriteError(e.to_string()))?;

        key.set_value("ProxyServer", &proxy_server.to_string())
            .map_err(|e| ProxyError::WriteError(e.to_string()))?;

        Ok(original)
    }

    fn restore_original(&self, state: &OriginalProxyState) -> Result<(), ProxyError> {
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
            .map_err(|e| ProxyError::WriteError(e.to_string()))?;

        if let Some(val) = &state.proxy_enable {
            key.set_value("ProxyEnable", val)
                .map_err(|e| ProxyError::WriteError(e.to_string()))?;
        } else {
            let _ = key.delete_value("ProxyEnable");
        }

        if let Some(val) = &state.proxy_server {
            key.set_value("ProxyServer", val)
                .map_err(|e| ProxyError::WriteError(e.to_string()))?;
        } else {
            let _ = key.delete_value("ProxyServer");
        }

        if let Some(val) = &state.proxy_override {
            key.set_value("ProxyOverride", val)
                .map_err(|e| ProxyError::WriteError(e.to_string()))?;
        } else {
            let _ = key.delete_value("ProxyOverride");
        }

        if let Some(val) = &state.auto_config_url {
            key.set_value("AutoConfigURL", val)
                .map_err(|e| ProxyError::WriteError(e.to_string()))?;
        } else {
            let _ = key.delete_value("AutoConfigURL");
        }

        Ok(())
    }

    fn notify_system(&self) {
        use std::ptr::null_mut;
        #[link(name = "wininet")]
        unsafe extern "system" {
            fn InternetSetOptionW(
                hInternet: *mut std::ffi::c_void,
                dwOption: u32,
                lpBuffer: *mut std::ffi::c_void,
                dwBufferLength: u32,
            ) -> i32;
        }

        const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
        const INTERNET_OPTION_REFRESH: u32 = 37;

        unsafe {
            InternetSetOptionW(null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, null_mut(), 0);
            InternetSetOptionW(null_mut(), INTERNET_OPTION_REFRESH, null_mut(), 0);
        }
    }
}

pub struct MockSystemProxy {
    state: std::sync::Mutex<OriginalProxyState>,
    pub notifications: std::sync::atomic::AtomicU32,
}

impl MockSystemProxy {
    pub fn dummy_mock_marker(&self) -> u32 {
        self.notifications
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Reads the first configured system DNS server (IPv4) from the TCP/IP
/// interface registry, formatted as "ip:53". Returns None if not found.
pub fn read_system_dns() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::*;
    const PATH: &str = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces";
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let ifaces = hklm.open_subkey(PATH).ok()?;
    let mut best: Option<String> = None;
    for iface in ifaces.enum_keys().flatten() {
        let key = match ifaces.open_subkey(iface) {
            Ok(k) => k,
            Err(_) => continue,
        };
        if let Ok(dns_list) = key.get_value::<Vec<String>, _>("NameServer") {
            for entry in dns_list.iter() {
                let parts: Vec<&str> = entry.trim().split(',').collect();
                for part in parts {
                    let ip = part.trim();
                    if !ip.is_empty()
                        && ip.parse::<std::net::Ipv4Addr>().is_ok()
                        && !ip.starts_with("127.")
                    {
                        best = Some(format!("{ip}:53"));
                        break;
                    }
                }
                if best.is_some() {
                    break;
                }
            }
        }
        if best.is_some() {
            break;
        }
    }
    best
}

impl MockSystemProxy {
    pub fn new(initial_state: OriginalProxyState) -> Self {
        Self {
            state: std::sync::Mutex::new(initial_state),
            notifications: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

impl SystemProxy for MockSystemProxy {
    fn read_original(&self) -> Result<OriginalProxyState, ProxyError> {
        let state = self.state.lock().unwrap();
        Ok(state.clone())
    }

    fn set_pac(
        &self,
        pac_url: &str,
        _proxy_server: &str,
    ) -> Result<OriginalProxyState, ProxyError> {
        let mut state = self.state.lock().unwrap();
        let original = state.clone();

        if let Some(true) = original.wpad_enabled {
            return Err(ProxyError::WpadConflict);
        }

        if let Some(ref existing_pac) = state.auto_config_url {
            if !existing_pac.is_empty() && !existing_pac.contains("127.0.0.1") {
                return Err(ProxyError::ExistingPacConfigured(existing_pac.clone()));
            }
        }

        state.auto_config_url = Some(pac_url.to_string());
        state.proxy_enable = Some(1);
        Ok(original)
    }

    fn restore_original(&self, restored_state: &OriginalProxyState) -> Result<(), ProxyError> {
        let mut state = self.state.lock().unwrap();
        *state = restored_state.clone();
        Ok(())
    }

    fn notify_system(&self) {
        self.notifications
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_proxy_conflict() {
        let mock = MockSystemProxy::new(OriginalProxyState {
            proxy_enable: None,
            proxy_server: None,
            proxy_override: None,
            auto_config_url: Some("http://corporate.pac/proxy.pac".to_string()),
            wpad_enabled: None,
        });

        let result = mock.set_pac("http://127.0.0.1:8787/proxy.pac", "http://127.0.0.1:8788");
        assert!(matches!(result, Err(ProxyError::ExistingPacConfigured(_))));
    }

    #[test]
    fn test_mock_wpad_conflict() {
        let mock = MockSystemProxy::new(OriginalProxyState {
            proxy_enable: None,
            proxy_server: None,
            proxy_override: None,
            auto_config_url: None,
            wpad_enabled: Some(true),
        });

        let result = mock.set_pac("http://127.0.0.1:8787/proxy.pac", "http://127.0.0.1:8788");
        assert!(matches!(result, Err(ProxyError::WpadConflict)));
    }

    #[test]
    fn test_mock_proxy_restore() {
        let original = OriginalProxyState {
            proxy_enable: Some(1),
            proxy_server: Some("192.168.1.1:8080".to_string()),
            proxy_override: Some("<local>".to_string()),
            auto_config_url: None,
            wpad_enabled: Some(false),
        };
        let mock = MockSystemProxy::new(original.clone());

        mock.set_pac("http://127.0.0.1:8787/proxy.pac", "http://127.0.0.1:8788")
            .unwrap();

        let modified = mock.read_original().unwrap();
        assert_eq!(
            modified.auto_config_url,
            Some("http://127.0.0.1:8787/proxy.pac".to_string())
        );

        mock.restore_original(&original).unwrap();
        let restored = mock.read_original().unwrap();

        assert_eq!(restored, original);
    }
}
