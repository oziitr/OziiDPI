//! Discover installed Discord versions and use only matching-version modules.
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const REQUIRED: [&str; 7] = [
    "discord_desktop_core",
    "discord_erlpack",
    "discord_notifications",
    "discord_spellcheck",
    "discord_utils",
    "discord_voice",
    "discord_zstd",
];

fn version_key(version: &str) -> Option<Vec<u32>> {
    let parts = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (parts.len() >= 3).then_some(parts)
}

pub fn version(exe: &Path) -> Option<String> {
    let info: Value =
        serde_json::from_slice(&fs::read(exe.parent()?.join("resources/build_info.json")).ok()?)
            .ok()?;
    let version = info.get("version")?.as_str()?;
    version_key(version)?;
    Some(version.to_string())
}

fn data_folder(exe: &Path) -> Result<&'static str, String> {
    let info: Value = serde_json::from_slice(
        &fs::read(
            exe.parent()
                .ok_or("Discord klasörü yok")?
                .join("resources/build_info.json"),
        )
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    match info
        .get("releaseChannel")
        .and_then(Value::as_str)
        .unwrap_or("stable")
    {
        "stable" => Ok("discord"),
        "ptb" => Ok("discordptb"),
        "canary" => Ok("discordcanary"),
        _ => Err("Desteklenmeyen Discord kanalı".into()),
    }
}

fn location_file() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("OziiDPI/discord-location.json"))
}

pub fn remember(exe: &Path) {
    if let Some(path) = location_file() {
        let _ = fs::create_dir_all(path.parent().unwrap());
        // Save the install root, not app-<version>, so updates are discovered.
        let parent = exe.parent().unwrap_or(exe);
        let root = if parent
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|s| s.starts_with("app-"))
        {
            parent.parent().unwrap_or(parent)
        } else {
            parent
        };
        if let Ok(bytes) = serde_json::to_vec(&json!({"root": root})) {
            let _ = fs::write(path, bytes);
        }
    }
}

#[cfg(windows)]
fn registry_roots() -> Vec<PathBuf> {
    use winreg::{enums::*, RegKey};
    let mut roots = Vec::new();
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let registry = RegKey::predef(hive);
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            if let Ok(app) = registry.open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\App Paths\Discord.exe",
                KEY_READ | view,
            ) {
                if let Ok(path) = app.get_value::<String, _>("") {
                    roots.push(PathBuf::from(path.trim_matches('"')));
                }
            }
            if let Ok(uninstall) = registry.open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
                KEY_READ | view,
            ) {
                for name in uninstall.enum_keys().flatten() {
                    let Ok(entry) = uninstall.open_subkey(name) else {
                        continue;
                    };
                    let display = entry
                        .get_value::<String, _>("DisplayName")
                        .unwrap_or_default();
                    if !["Discord", "Discord PTB", "Discord Canary"]
                        .iter()
                        .any(|n| display.eq_ignore_ascii_case(n))
                    {
                        continue;
                    }
                    if let Ok(path) = entry.get_value::<String, _>("InstallLocation") {
                        roots.push(PathBuf::from(path.trim_matches('"')));
                    }
                    if let Ok(icon) = entry.get_value::<String, _>("DisplayIcon") {
                        let value = if icon.starts_with('"') {
                            icon.trim_start_matches('"').split('"').next().unwrap_or("")
                        } else {
                            icon.split(',').next().unwrap_or("")
                        };
                        if let Some(parent) = Path::new(value).parent() {
                            roots.push(parent.to_path_buf());
                        }
                    }
                }
            }
        }
    }
    roots
}

#[cfg(not(windows))]
fn registry_roots() -> Vec<PathBuf> {
    Vec::new()
}

fn candidates(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for root in roots {
        let root = if root.is_file() {
            root.parent().unwrap_or(root)
        } else {
            root.as_path()
        };
        let mut directories = vec![root.to_path_buf()];
        if root
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("app-"))
        {
            if let Some(parent) = root.parent() {
                directories.push(parent.to_path_buf());
            }
        }
        for directory in directories {
            let mut possible = vec![directory.join("Discord.exe")];
            if let Ok(entries) = fs::read_dir(&directory) {
                possible.extend(
                    entries
                        .flatten()
                        .filter(|e| {
                            e.file_name()
                                .to_str()
                                .is_some_and(|s| s.starts_with("app-"))
                        })
                        .map(|e| e.path().join("Discord.exe")),
                );
            }
            for exe in possible {
                if exe.is_file()
                    && version(&exe).is_some()
                    && data_folder(&exe).is_ok()
                    && !result.contains(&exe)
                {
                    result.push(exe);
                }
            }
        }
    }
    result.sort_by_key(|exe| {
        std::cmp::Reverse((
            data_folder(exe).ok() == Some("discord"),
            version(exe).and_then(|v| version_key(&v)),
        ))
    });
    result
}

pub fn find_all() -> Vec<PathBuf> {
    let mut roots = registry_roots();
    for base in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(base) {
            for name in ["Discord", "DiscordPTB", "DiscordCanary"] {
                roots.push(PathBuf::from(&base).join(name));
            }
        }
    }
    if let Some(saved) = location_file()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    {
        if let Some(root) = saved.get("root").and_then(Value::as_str) {
            roots.push(PathBuf::from(root));
        }
    }
    // Also resolve user-created desktop/start-menu shortcuts for custom installs.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = "$ErrorActionPreference='SilentlyContinue'; $shell=New-Object -ComObject WScript.Shell; $places=@([Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('Programs'),[Environment]::GetFolderPath('CommonPrograms')); foreach($place in $places){Get-ChildItem -LiteralPath $place -Filter '*Discord*.lnk' -Recurse -File | ForEach-Object {$link=$shell.CreateShortcut($_.FullName); if($link.TargetPath){[Console]::WriteLine($link.TargetPath)}}}";
        if let Ok(output) = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x08000000)
            .output()
        {
            roots.extend(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| PathBuf::from(s.trim())),
            );
        }
    }
    candidates(&roots)
}

fn ready(root: &Path, name: &str) -> bool {
    let nonempty = |p: PathBuf| fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0);
    nonempty(root.join("index.js"))
        && (name != "discord_desktop_core" || nonempty(root.join("core.asar")))
}

fn sources(root: &Path) -> BTreeMap<String, (PathBuf, u64)> {
    let mut modules = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let Some(folder) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some((name, revision)) = folder
                .rsplit_once('-')
                .and_then(|(n, v)| v.parse::<u64>().ok().map(|v| (n, v)))
            else {
                continue;
            };
            if !name.starts_with("discord_") || revision == 0 {
                continue;
            }
            let path = entry.path().join(name);
            if ready(&path, name) && modules.get(name).is_none_or(|(_, old)| revision > *old) {
                modules.insert(name.to_string(), (path, revision));
            }
        }
    }
    modules
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("Modül paketinde sembolik bağ desteklenmiyor".into());
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &target.join(entry.file_name()))?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target.join(entry.file_name())).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn ensure_at(exe: &Path, appdata: &Path, bundle: &Path) -> Result<bool, String> {
    let version = version(exe).ok_or("Discord sürüm bilgisi okunamadı")?;
    let target = appdata
        .join(data_folder(exe)?)
        .join(&version)
        .join("modules");
    let metadata = target.join("installed.json");
    let original = fs::read(&metadata).ok();
    let mut installed: Value = original
        .as_ref()
        .and_then(|b| serde_json::from_slice(b).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let mut available = sources(&exe.parent().ok_or("Discord klasörü yok")?.join("modules"));
    // Older distributions can have a same-version repair bundle, never another version's native code.
    for name in REQUIRED {
        let path = bundle.join(&version).join(name);
        if !available.contains_key(name) && ready(&path, name) {
            available.insert(name.into(), (path, 1));
        }
    }
    for name in REQUIRED {
        let known_revision = installed
            .get(name)
            .and_then(|m| m.get("installedVersion"))
            .and_then(Value::as_u64)
            .is_some_and(|v| v > 0);
        if !(ready(&target.join(name), name) && known_revision) && !available.contains_key(name) {
            return Err(format!("Discord {version}: {name} kurulumda eksik. Discord kurulumunu resmi yükleyiciyle tamamlayın."));
        }
    }
    let mut changed = false;
    for (name, (source, revision)) in available {
        let destination = target.join(&name);
        let known_revision = installed
            .get(&name)
            .and_then(|m| m.get("installedVersion"))
            .and_then(Value::as_u64)
            .is_some_and(|v| v > 0);
        if ready(&destination, &name) && known_revision {
            continue;
        }
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let staging = target.join(format!(".oziidpi-stage-{name}-{nonce}"));
        copy_tree(&source, &staging)?;
        let backup = target.join(format!(".oziidpi-backup-{name}-{nonce}"));
        if destination.exists() {
            fs::rename(&destination, &backup).map_err(|e| e.to_string())?;
        }
        if let Err(error) = fs::rename(&staging, &destination) {
            if backup.exists() {
                let _ = fs::rename(&backup, &destination);
            }
            return Err(format!("Modül yerleştirilemedi: {error}"));
        }
        installed[&name] = json!({"installedVersion": revision});
        changed = true;
    }
    if changed {
        if let Some(original) = original {
            let backup = target.join("installed.json.oziidpi-backup");
            if !backup.exists() {
                fs::write(backup, original).map_err(|e| e.to_string())?;
            }
        }
        fs::write(
            metadata,
            serde_json::to_vec_pretty(&installed).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(changed)
}

pub fn ensure(exe: &Path) -> Result<bool, String> {
    let appdata = PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA bulunamadı")?);
    let bundle = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("OziiDPI klasörü bulunamadı")?
        .join("discord-modules");
    ensure_at(exe, &appdata, &bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir()
                .join(format!("oziidpi-discovery-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn host(&self, version: &str) -> PathBuf {
            let directory = self.0.join("custom install").join(format!("app-{version}"));
            fs::create_dir_all(directory.join("resources")).unwrap();
            fs::write(directory.join("Discord.exe"), "fixture").unwrap();
            fs::write(
                directory.join("resources/build_info.json"),
                serde_json::to_vec(&json!({"version": version, "releaseChannel":"stable"}))
                    .unwrap(),
            )
            .unwrap();
            directory.join("Discord.exe")
        }
        fn modules(&self, exe: &Path, revision: u64) {
            for name in REQUIRED {
                let p = exe
                    .parent()
                    .unwrap()
                    .join("modules")
                    .join(format!("{name}-{revision}"))
                    .join(name);
                fs::create_dir_all(&p).unwrap();
                fs::write(p.join("index.js"), "fixture").unwrap();
                if name == "discord_desktop_core" {
                    fs::write(p.join("core.asar"), "fixture").unwrap();
                }
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn update_discovery_rescans_parent_of_stale_exe() {
        let f = Fixture::new();
        let old = f.host("1.0.9255");
        let new = f.host("1.0.9256");
        assert_eq!(candidates(&[old.parent().unwrap().to_path_buf()])[0], new);
    }
    #[test]
    fn versions_sort_numerically_and_incomplete_hosts_are_ignored() {
        let f = Fixture::new();
        f.host("1.0.999");
        let newest = f.host("1.0.1000");
        let broken = f.host("1.0.1001");
        fs::remove_file(broken.parent().unwrap().join("resources/build_info.json")).unwrap();
        assert_eq!(candidates(&[f.0.join("custom install")])[0], newest);
    }
    #[test]
    fn modern_modules_repair_without_a_bundled_version_and_preserve_revision() {
        let f = Fixture::new();
        let exe = f.host("1.0.9256");
        f.modules(&exe, 7);
        let data = f.0.join("appdata");
        assert!(ensure_at(&exe, &data, &f.0.join("no-bundle")).unwrap());
        let metadata = data.join("discord/1.0.9256/modules/installed.json");
        let content = fs::read(&metadata).unwrap();
        let installed: Value = serde_json::from_slice(&content).unwrap();
        assert_eq!(installed["discord_voice"]["installedVersion"], 7);
        assert!(!ensure_at(&exe, &data, &f.0.join("no-bundle")).unwrap());
        assert_eq!(fs::read(metadata).unwrap(), content);
    }
    #[test]
    fn missing_required_module_does_not_partially_write_user_data() {
        let f = Fixture::new();
        let exe = f.host("1.0.9256");
        f.modules(&exe, 1);
        fs::remove_file(
            exe.parent()
                .unwrap()
                .join("modules/discord_voice-1/discord_voice/index.js"),
        )
        .unwrap();
        let data = f.0.join("appdata");
        assert!(ensure_at(&exe, &data, &f.0.join("no-bundle")).is_err());
        assert!(!data.exists());
    }
    #[test]
    fn old_version_modules_are_never_used_for_a_new_host() {
        let f = Fixture::new();
        let old = f.host("1.0.9255");
        f.modules(&old, 1);
        let new = f.host("1.0.9256");
        assert!(ensure_at(&new, &f.0.join("appdata"), &f.0.join("no-bundle")).is_err());
    }
    #[test]
    fn reject_path_components_in_version() {
        assert!(version_key("../../1.0.9256").is_none());
        assert!(version_key("1.0.9256").is_some());
    }
}
