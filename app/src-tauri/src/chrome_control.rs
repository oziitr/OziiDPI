use serde::Serialize;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use crate::per_app::{find_chrome_exe, proxy_reachable, PerAppManager};

const CONTROL_HOST: &str = "127.0.0.1";
const CONTROL_PORT: u16 = 39579;
const REGISTRY_PATH: &str = r"Software\OziiDPI";
const REGISTRY_VALUE: &str = "ChromeTunnelEnabled";

#[derive(Debug, Clone, Serialize)]
pub struct ChromeTunnelStatus {
    pub enabled: bool,
    pub extension_connected: bool,
    pub effective: bool,
    pub extension_path: String,
}

pub struct ChromeControl {
    enabled: AtomicBool,
    last_extension_seen: Mutex<Option<Instant>>,
}

impl ChromeControl {
    pub fn new() -> Self {
        Self {
            enabled: AtomicBool::new(read_enabled()),
            last_extension_seen: Mutex::new(None),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        write_enabled(enabled)?;
        self.enabled.store(enabled, Ordering::SeqCst);
        Ok(())
    }

    pub fn status(&self) -> ChromeTunnelStatus {
        let extension_connected = self
            .last_extension_seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some_and(|seen| seen.elapsed() < Duration::from_secs(5));
        ChromeTunnelStatus {
            enabled: self.enabled(),
            extension_connected,
            effective: self.enabled() && extension_connected && proxy_reachable(),
            extension_path: extension_directory()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        }
    }

    pub fn start_server(self: &Arc<Self>) -> Result<(), String> {
        let listener = TcpListener::bind((CONTROL_HOST, CONTROL_PORT))
            .map_err(|error| format!("Chrome kontrol kanalı başlatılamadı: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("Chrome kontrol kanalı ayarlanamadı: {error}"))?;
        let control = self.clone();
        thread::spawn(move || loop {
            match listener.accept() {
                Ok((mut stream, _)) => control.respond(&mut stream),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => thread::sleep(Duration::from_millis(250)),
            }
        });
        Ok(())
    }

    fn respond(&self, stream: &mut TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        let mut request = [0_u8; 64];
        if stream.read(&mut request).is_err() {
            return;
        }
        *self
            .last_extension_seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Instant::now());
        let ready = proxy_reachable();
        let response = serde_json::json!({
            "enabled": self.enabled() && ready,
            "requested": self.enabled(),
            "proxyReady": ready,
            "proxyHost": "127.0.0.1",
            "proxyPort": 39572
        });
        if let Ok(bytes) = serde_json::to_vec(&response) {
            let _ = stream.write_all(&bytes);
            let _ = stream.flush();
        }
    }

    pub fn prepare_extension_install(&self) -> Result<String, String> {
        let extension = extension_directory()
            .ok_or_else(|| "OziiDPI Chrome uzantısı uygulama klasöründe bulunamadı".to_string())?;
        let chrome = find_chrome_exe().ok_or_else(|| "Google Chrome bulunamadı".to_string())?;
        Command::new(&chrome)
            .arg("chrome://extensions/")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Chrome uzantılar sayfası açılamadı: {error}"))?;
        let manifest = extension.join("manifest.json");
        let _ = Command::new("explorer.exe")
            .arg(format!("/select,{}", manifest.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        Ok(format!(
            "Chrome'da Geliştirici modu > Paketlenmemiş öğe yükle deyip bu klasörü seçin: {}",
            extension.display()
        ))
    }
}

pub fn enable_tunnel(
    manager: &PerAppManager,
    control: &ChromeControl,
    enabled: bool,
) -> Result<String, String> {
    if enabled {
        manager.ensure_backend()?;
    }
    control.set_enabled(enabled)?;
    let message = if enabled {
        "Normal Chrome tüneli açıldı; açık sekmede sayfayı yenileyin".to_string()
    } else {
        "Normal Chrome tüneli kapatıldı; Chrome eski bağlantısına döndü".to_string()
    };
    manager.set_message(&message);
    Ok(message)
}

fn extension_directory() -> Option<PathBuf> {
    let directory = std::env::current_exe().ok()?.parent().map(PathBuf::from)?;
    let adjacent = directory.join("chrome-extension");
    if adjacent.join("manifest.json").is_file() {
        return Some(adjacent);
    }
    directory.ancestors().find_map(|ancestor| {
        let candidate = ancestor.join("chrome-extension");
        candidate
            .join("manifest.json")
            .is_file()
            .then_some(candidate)
    })
}

#[cfg(windows)]
fn read_enabled() -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(REGISTRY_PATH, KEY_READ)
        .ok()
        .and_then(|key| key.get_value::<u32, _>(REGISTRY_VALUE).ok())
        .is_some_and(|value| value == 1)
}

#[cfg(not(windows))]
fn read_enabled() -> bool {
    false
}

#[cfg(windows)]
fn write_enabled(enabled: bool) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(REGISTRY_PATH, KEY_WRITE)
        .map_err(|error| format!("Chrome tünel ayarı açılamadı: {error}"))?;
    key.set_value(REGISTRY_VALUE, &(enabled as u32))
        .map_err(|error| format!("Chrome tünel ayarı kaydedilemedi: {error}"))
}

#[cfg(not(windows))]
fn write_enabled(_enabled: bool) -> Result<(), String> {
    Err("Chrome tüneli yalnız Windows'ta destekleniyor".to_string())
}
