use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;

use crate::protocol::HostPersonaMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRule {
    pub id: String,
    pub process_name: String,
    pub mode: HostPersonaMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionConfig {
    pub auto_switch_enabled: bool,
    pub start_minimized: bool,
    pub game_rules: Vec<GameRule>,
}

impl Default for CompanionConfig {
    fn default() -> Self {
        Self {
            auto_switch_enabled: true,
            start_minimized: false,
            game_rules: vec![
                GameRule {
                    id: "rule-1".to_string(),
                    process_name: "Cyberpunk2077.exe".to_string(),
                    mode: HostPersonaMode::Xbox,
                },
                GameRule {
                    id: "rule-2".to_string(),
                    process_name: "EldenRing.exe".to_string(),
                    mode: HostPersonaMode::Xbox,
                },
            ],
        }
    }
}

impl CompanionConfig {
    fn config_path() -> Option<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "DS5Bridge", "DS5BridgeCompanion")?;
        let config_dir = proj_dirs.config_dir();
        fs::create_dir_all(config_dir).ok()?;
        Some(config_dir.join("companion_config.json"))
    }

    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(config) = serde_json::from_str::<CompanionConfig>(&content) {
                        return config;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Ok(content) = serde_json::to_string_pretty(self) {
                let _ = fs::write(path, content);
            }
        }
    }
}
