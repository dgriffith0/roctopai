use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use color_eyre::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub repo: String,
    #[serde(default)]
    pub verify_commands: HashMap<String, String>,
    #[serde(default)]
    pub editor_commands: HashMap<String, String>,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("roctopai")
        .join("config.json")
}

pub fn load_config() -> Option<Config> {
    let path = config_path();
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_config(repo: &str) -> Result<()> {
    // Load existing config to preserve verify_commands and editor_commands
    let existing = load_config();
    let verify_commands = existing
        .as_ref()
        .map(|c| c.verify_commands.clone())
        .unwrap_or_default();
    let editor_commands = existing
        .as_ref()
        .map(|c| c.editor_commands.clone())
        .unwrap_or_default();
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let config = Config {
        repo: repo.to_string(),
        verify_commands,
        editor_commands,
    };
    fs::write(path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

pub fn save_full_config(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

pub fn get_verify_command(repo: &str) -> Option<String> {
    let config = load_config()?;
    config.verify_commands.get(repo).cloned()
}

pub fn get_editor_command(repo: &str) -> Option<String> {
    let config = load_config()?;
    config.editor_commands.get(repo).cloned()
}

pub fn set_editor_command(repo: &str, command: &str) -> Result<()> {
    let mut config = load_config().unwrap_or(Config {
        repo: repo.to_string(),
        verify_commands: HashMap::new(),
        editor_commands: HashMap::new(),
    });
    config
        .editor_commands
        .insert(repo.to_string(), command.to_string());
    save_full_config(&config)
}

pub fn set_verify_command(repo: &str, command: &str) -> Result<()> {
    let mut config = load_config().unwrap_or(Config {
        repo: repo.to_string(),
        verify_commands: HashMap::new(),
        editor_commands: HashMap::new(),
    });
    config
        .verify_commands
        .insert(repo.to_string(), command.to_string());
    save_full_config(&config)
}
