mod chrome_control;
mod discord_install;
mod native_messaging;
mod per_app;

use chrome_control::{enable_tunnel, ChromeControl, ChromeTunnelStatus};
use per_app::{PerAppManager, PerAppStatus};
use std::sync::Arc;
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, State,
};

#[cfg(windows)]
fn enforce_single_instance() {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::synchapi::CreateMutexW;
    use winapi::um::winnt::HANDLE;
    use winapi::um::winuser::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE};

    extern "system" {
        fn GetLastError() -> u32;
    }

    let mutex_wide: Vec<u16> = "OziiDPI-SingleInstance-Mutex\0".encode_utf16().collect();
    let title_wide: Vec<u16> = "OziiDPI\0".encode_utf16().collect();
    unsafe {
        let mutex = CreateMutexW(std::ptr::null_mut(), 0, mutex_wide.as_ptr());
        let already_running = !mutex.is_null() && GetLastError() == 183;
        if already_running {
            let window = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if !window.is_null() {
                ShowWindow(window, SW_RESTORE);
                SetForegroundWindow(window);
            }
            if !mutex.is_null() {
                CloseHandle(mutex as HANDLE);
            }
            std::process::exit(0);
        }
    }
}

#[cfg(not(windows))]
fn enforce_single_instance() {}

#[tauri::command]
fn get_app_status(manager: State<'_, Arc<PerAppManager>>) -> PerAppStatus {
    manager.status()
}

#[tauri::command]
async fn launch_discord(manager: State<'_, Arc<PerAppManager>>) -> Result<String, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.launch_discord())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
fn stop_discord(manager: State<'_, Arc<PerAppManager>>) -> Result<String, String> {
    manager.stop_discord()
}

#[tauri::command]
fn get_chrome_tunnel_status(control: State<'_, Arc<ChromeControl>>) -> ChromeTunnelStatus {
    control.status()
}

#[tauri::command]
fn set_chrome_tunnel(
    enabled: bool,
    manager: State<'_, Arc<PerAppManager>>,
    control: State<'_, Arc<ChromeControl>>,
) -> Result<String, String> {
    enable_tunnel(manager.inner(), control.inner(), enabled)
}

#[tauri::command]
fn prepare_chrome_extension(
    manager: State<'_, Arc<PerAppManager>>,
    control: State<'_, Arc<ChromeControl>>,
) -> Result<String, String> {
    let message = control.prepare_extension_install()?;
    manager.set_message(&message);
    Ok(message)
}

#[cfg(windows)]
fn startup_value() -> Result<String, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("OziiDPI uygulama yolu okunamadı: {error}"))?;
    Ok(format!("\"{}\" --startup", executable.display()))
}

#[cfg(windows)]
fn auto_start_enabled() -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;
    let Ok(expected) = startup_value() else {
        return false;
    };
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_READ)
        .ok()
        .and_then(|key| key.get_value::<String, _>("OziiDPI").ok())
        .is_some_and(|value| value.eq_ignore_ascii_case(&expected))
}

#[cfg(not(windows))]
fn auto_start_enabled() -> bool {
    false
}

#[tauri::command]
fn get_auto_start() -> bool {
    auto_start_enabled()
}

#[cfg(windows)]
fn write_auto_start(enabled: bool) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_WRITE)
        .map_err(|error| format!("Windows başlangıç ayarı açılamadı: {error}"))?;
    if enabled {
        key.set_value("OziiDPI", &startup_value()?)
            .map_err(|error| format!("Başlangıç ayarı kaydedilemedi: {error}"))
    } else {
        match key.delete_value("OziiDPI") {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Başlangıç ayarı kaldırılamadı: {error}")),
        }
    }
}

#[cfg(not(windows))]
fn write_auto_start(_enabled: bool) -> Result<(), String> {
    Err("Başlangıç ayarı yalnız Windows'ta destekleniyor".to_string())
}

#[tauri::command]
fn set_auto_start(enabled: bool) -> Result<(), String> {
    write_auto_start(enabled)
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle, manager: State<'_, Arc<PerAppManager>>) {
    manager.stop_backend();
    app.exit(0);
}

fn position_bottom_right(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(Some(monitor)) = window.current_monitor() {
            let screen = monitor.size();
            let scale = monitor.scale_factor();
            let logical = tauri::LogicalSize::new(380.0_f64, 620.0_f64);
            let _ = window.set_size(tauri::Size::Logical(logical));
            let window_size = window
                .inner_size()
                .unwrap_or(tauri::PhysicalSize::new(380, 620));
            let x = screen.width as i32 - window_size.width as i32 - (24.0 * scale) as i32;
            let y = screen.height as i32 - window_size.height as i32 - (48.0 * scale) as i32;
            let _ = window.set_position(tauri::PhysicalPosition::new(x.max(0), y.max(0)));
        }
    }
}

fn spawn_discord(manager: Arc<PerAppManager>) {
    std::thread::spawn(move || {
        let _ = manager.launch_discord();
    });
}

fn spawn_chrome_toggle(manager: Arc<PerAppManager>, control: Arc<ChromeControl>) {
    std::thread::spawn(move || {
        let _ = enable_tunnel(&manager, &control, !control.enabled());
    });
}

fn spawn_backend(manager: Arc<PerAppManager>) {
    std::thread::spawn(move || {
        let _ = manager.ensure_backend();
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let arguments: Vec<String> = std::env::args().collect();
    if native_messaging::requested(&arguments) {
        let exit_code = if native_messaging::run().is_ok() {
            0
        } else {
            1
        };
        std::process::exit(exit_code);
    }
    enforce_single_instance();
    let startup_mode = arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--startup"));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let manager = Arc::new(PerAppManager::default());
            let chrome_control = Arc::new(ChromeControl::new());
            chrome_control.start_server()?;
            app.manage(manager.clone());
            app.manage(chrome_control.clone());
            position_bottom_right(app.handle());

            let show_item =
                tauri::menu::MenuItem::with_id(app, "show", "OziiDPI'yi Aç", true, None::<&str>)?;
            let discord_item = tauri::menu::MenuItem::with_id(
                app,
                "discord",
                "Discord'u Başlat",
                true,
                None::<&str>,
            )?;
            let chrome_item = tauri::menu::MenuItem::with_id(
                app,
                "chrome",
                "Normal Chrome Tünelini Aç/Kapat",
                true,
                None::<&str>,
            )?;
            let quit_item =
                tauri::menu::MenuItem::with_id(app, "quit", "Tamamen Kapat", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(
                app,
                &[&show_item, &discord_item, &chrome_item, &quit_item],
            )?;
            TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("OziiDPI — ozii")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "discord" => {
                        let manager = app.state::<Arc<PerAppManager>>().inner().clone();
                        spawn_discord(manager);
                    }
                    "chrome" => {
                        let manager = app.state::<Arc<PerAppManager>>().inner().clone();
                        let control = app.state::<Arc<ChromeControl>>().inner().clone();
                        spawn_chrome_toggle(manager, control);
                    }
                    "quit" => {
                        let manager = app.state::<Arc<PerAppManager>>().inner().clone();
                        manager.stop_backend();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            if startup_mode {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
                spawn_discord(manager);
            } else if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                if chrome_control.enabled() {
                    spawn_backend(manager);
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            launch_discord,
            stop_discord,
            get_chrome_tunnel_status,
            set_chrome_tunnel,
            prepare_chrome_extension,
            get_auto_start,
            set_auto_start,
            quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("OziiDPI başlatılamadı");
}
