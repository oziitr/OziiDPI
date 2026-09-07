use crate::discord_install;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

const PROXY_HOST: &str = "127.0.0.1";
const PROXY_PORT: u16 = 39572;
#[derive(Debug, Clone, Serialize)]
pub struct PerAppStatus {
    pub backend_ready: bool,
    pub discord_running: bool,
    pub proxy_url: String,
    pub last_message: String,
}

#[derive(Default)]
struct Children {
    backend: Option<Child>,
    discord: Option<Child>,
    last_message: String,
}

#[derive(Default)]
pub struct PerAppManager {
    children: Mutex<Children>,
    launch_lock: Mutex<()>,
}

pub(crate) fn proxy_reachable() -> bool {
    let address = format!("{PROXY_HOST}:{PROXY_PORT}");
    address
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| addresses.next())
        .and_then(|address| TcpStream::connect_timeout(&address, Duration::from_millis(500)).ok())
        .is_some()
}

fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn current_directory() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

fn find_backend_cli() -> Option<PathBuf> {
    let directory = current_directory()?;
    let adjacent = directory.join("ozii-cli.exe");
    if adjacent.is_file() {
        return Some(adjacent);
    }
    for ancestor in directory.ancestors() {
        for relative in [
            "backend/target/release/ozii-cli.exe",
            "dist/product/ozii-cli.exe",
        ] {
            let candidate = ancestor.join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub(crate) fn find_chrome_exe() -> Option<PathBuf> {
    [
        ("ProgramFiles", "Google/Chrome/Application/chrome.exe"),
        ("ProgramFiles(x86)", "Google/Chrome/Application/chrome.exe"),
        ("LOCALAPPDATA", "Google/Chrome/Application/chrome.exe"),
    ]
    .into_iter()
    .find_map(|(base, relative)| {
        std::env::var(base).ok().and_then(|root| {
            let candidate = PathBuf::from(root).join(relative);
            candidate.is_file().then_some(candidate)
        })
    })
}

fn discord_pids() -> Vec<u32> {
    let mut command = hidden_command("tasklist");
    command
        .args(["/FI", "IMAGENAME eq Discord.exe", "/FO", "CSV", "/NH"])
        .stdout(std::process::Stdio::piped());
    let output = command.output();
    let mut result = Vec::new();
    if let Ok(output) = output {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let cells: Vec<&str> = line
                .split(',')
                .map(|cell| cell.trim().trim_matches('"'))
                .collect();
            if cells.len() >= 2 && cells[0].eq_ignore_ascii_case("Discord.exe") {
                if let Ok(pid) = cells[1].parse::<u32>() {
                    result.push(pid);
                }
            }
        }
    }
    result
}

fn launch_discord_process(executable: &Path) -> Result<Child, String> {
    let resources = executable
        .parent()
        .ok_or_else(|| "Discord uygulama klasörü bulunamadı".to_string())?
        .join("resources");
    let build_info = resources.join("build_info.json");
    let backup = resources.join("build_info.json.oziidpi-backup");
    if backup.is_file() {
        let saved = fs::read(&backup)
            .map_err(|error| format!("Discord başlangıç yedeği okunamadı: {error}"))?;
        fs::write(&build_info, saved)
            .map_err(|error| format!("Discord yedeği geri yüklenemedi: {error}"))?;
        let _ = fs::remove_file(&backup);
    }
    let original = fs::read(&build_info)
        .map_err(|error| format!("Discord build_info.json okunamadı: {error}"))?;
    let mut value: Value = serde_json::from_slice(&original)
        .map_err(|error| format!("Discord build_info.json geçersiz: {error}"))?;
    value
        .as_object_mut()
        .ok_or_else(|| "Discord build_info.json nesne değil".to_string())?
        .insert("disableUpdater".to_string(), Value::Bool(true));
    fs::write(&backup, &original)
        .map_err(|error| format!("Discord başlangıç yedeği yazılamadı: {error}"))?;
    fs::write(
        &build_info,
        serde_json::to_vec_pretty(&value)
            .map_err(|error| format!("Discord başlangıç ayarı hazırlanamadı: {error}"))?,
    )
    .map_err(|error| format!("Discord güncelleyici atlaması uygulanamadı: {error}"))?;

    let mut launched = Command::new(executable)
        .arg(format!("--proxy-server=http://{PROXY_HOST}:{PROXY_PORT}"))
        .arg("--disable-quic")
        .current_dir(executable.parent().unwrap_or_else(|| Path::new(".")))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if launched.is_ok() {
        thread::sleep(Duration::from_secs(2));
    }
    let mut restore_error = None;
    for _ in 0..3 {
        match fs::write(&build_info, &original) {
            Ok(()) => {
                restore_error = None;
                break;
            }
            Err(error) => {
                restore_error = Some(error);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    if restore_error.is_none() {
        let _ = fs::remove_file(&backup);
    }
    if let Some(error) = restore_error {
        return Err(format!("Discord build_info.json geri yüklenemedi: {error}"));
    }
    if let Ok(child) = &mut launched {
        if let Ok(Some(exit)) = child.try_wait() {
            return Err(format!(
                "Discord başlangıç sırasında kapandı ({exit}); modül uyumluluğunu kontrol edin"
            ));
        }
    }
    launched.map_err(|error| format!("Discord başlatılamadı: {error}"))
}

impl PerAppManager {
    pub fn status(&self) -> PerAppStatus {
        let children = self
            .children
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        PerAppStatus {
            backend_ready: proxy_reachable(),
            discord_running: !discord_pids().is_empty(),
            proxy_url: format!("http://{PROXY_HOST}:{PROXY_PORT}"),
            last_message: children.last_message.clone(),
        }
    }

    pub(crate) fn set_message(&self, message: impl Into<String>) {
        self.children
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .last_message = message.into();
    }

    pub(crate) fn ensure_backend(&self) -> Result<(), String> {
        if proxy_reachable() {
            return Ok(());
        }
        let cli = find_backend_cli().ok_or_else(|| {
            "ozii-cli.exe bulunamadı; OziiDPI ile aynı klasörde olmalı".to_string()
        })?;
        let child = hidden_command(&cli)
            .args([
                "local",
                "--mode",
                "balanced",
                "--chunk-size",
                "2",
                "--no-guard",
            ])
            .spawn()
            .map_err(|error| format!("OziiDPI backend başlatılamadı: {error}"))?;
        self.children
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .backend = Some(child);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            if proxy_reachable() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(250));
        }
        Err("Yerel tünel 20 saniye içinde hazır olmadı".to_string())
    }

    pub fn launch_discord(&self) -> Result<String, String> {
        let _launch = self
            .launch_lock
            .try_lock()
            .map_err(|_| "Discord zaten hazırlanıyor".to_string())?;
        self.set_message("Discord kurulumu aranıyor...");
        let result = self.launch_discord_inner();
        match &result {
            Ok(message) => self.set_message(message),
            Err(error) => self.set_message(format!("Hata: {error}")),
        }
        result
    }

    fn launch_discord_inner(&self) -> Result<String, String> {
        self.ensure_backend()?;
        let candidates = discord_install::find_all();
        if candidates.is_empty() {
            return Err("Discord bulunamadı. Kurulum dizinleri, kayıt defteri ve Discord kısayolları tarandı.".into());
        }
        if !discord_pids().is_empty() {
            let _ = hidden_command("taskkill")
                .args(["/F", "/IM", "Discord.exe"])
                .status();
            thread::sleep(Duration::from_secs(2));
            if !discord_pids().is_empty() {
                return Err("Discord kapatılamadı; modüller kullanımda".into());
            }
        }
        let mut failures = Vec::new();
        for executable in candidates {
            let version = discord_install::version(&executable).unwrap_or_default();
            self.set_message(format!("Discord {version} modülleri denetleniyor..."));
            let repaired = match discord_install::ensure(&executable) {
                Ok(repaired) => repaired,
                Err(error) => {
                    failures.push(error);
                    continue;
                }
            };
            // Do not silently retry another host after a process has been spawned.
            let child = launch_discord_process(&executable)?;
            let pid = child.id();
            self.children
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .discord = Some(child);
            discord_install::remember(&executable);
            return Ok(format!(
                "Discord {version} {}tünelden başlatıldı (PID {pid})",
                if repaired {
                    "kendi kurulumundan onarıldı ve "
                } else {
                    ""
                }
            ));
        }
        Err(failures.join(" | "))
    }

    pub fn stop_discord(&self) -> Result<String, String> {
        let _ = hidden_command("taskkill")
            .args(["/F", "/IM", "Discord.exe"])
            .status();
        self.children
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .discord = None;
        let message = "Discord kapatıldı".to_string();
        self.set_message(&message);
        Ok(message)
    }

    pub fn stop_backend(&self) {
        if let Some(cli) = find_backend_cli() {
            let _ = hidden_command(cli).arg("stop").status();
        }
        let mut children = self
            .children
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mut child) = children.backend.take() {
            let _ = child.kill();
        }
    }
}
