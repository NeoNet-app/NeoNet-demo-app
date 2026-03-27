use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub pseudo: String,
}

/// Read the daemon session token from ~/.neonet/session.token
pub fn load_token() -> Result<String, String> {
    let path = dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".neonet")
        .join("session.token");

    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))
}

fn config_path() -> Result<PathBuf, String> {
    Ok(dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".neonet-demo")
        .join("config.toml"))
}

/// Load pseudo from ~/.neonet-demo/config.toml if it exists.
pub fn load_config() -> Option<AppConfig> {
    let path = config_path().ok()?;
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

/// Save pseudo to ~/.neonet-demo/config.toml, creating dirs as needed.
pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {e}"))?;
    }
    let content = toml::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    std::fs::write(&path, content)
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))
}
