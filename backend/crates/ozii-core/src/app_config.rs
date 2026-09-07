use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub mode: String,
    pub chunk_size: u8,
    pub fake_count: u8,
    pub ignore_wpad: bool,
    #[serde(default = "default_true")]
    pub protect_discord_updater: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mode: "turbo".to_string(),
            chunk_size: 2,
            fake_count: 1,
            ignore_wpad: false,
            protect_discord_updater: true,
        }
    }
}

pub struct ConfigManager {
    config_path: PathBuf,
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigManager {
    pub fn new() -> Self {
        let local_app_data = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());

        let mut config_path = PathBuf::from(local_app_data);
        config_path.push("OziiDPI");
        std::fs::create_dir_all(&config_path).unwrap_or_default();
        config_path.push("config.json");

        Self { config_path }
    }

    pub fn load(&self) -> AppConfig {
        if let Ok(content) = std::fs::read_to_string(&self.config_path)
            && let Ok(config) = serde_json::from_str::<AppConfig>(&content)
        {
            return config;
        }
        AppConfig::default()
    }

    pub fn save(&self, config: &AppConfig) -> std::io::Result<()> {
        let content = serde_json::to_string_pretty(config)?;
        std::fs::write(&self.config_path, content)
    }
}
