use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use color_eyre::Result;
use serde::{Deserialize, Serialize};

use crate::session::Multiplexer;

fn default_auto_open_pr() -> HashMap<String, bool> {
    HashMap::new()
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub repo: String,
    #[serde(default)]
    pub verify_commands: HashMap<String, String>,
    #[serde(default)]
    pub editor_commands: HashMap<String, String>,
    #[serde(default)]
    pub pr_ready: HashMap<String, bool>,
    #[serde(default = "default_auto_open_pr")]
    pub auto_open_pr: HashMap<String, bool>,
    #[serde(default, alias = "claude_commands")]
    pub session_commands: HashMap<String, String>,
    #[serde(default)]
    pub multiplexer: Option<Multiplexer>,
    /// Global default session command template used when no per-repo override exists.
    /// Set during initial setup based on which AI tools are installed.
    #[serde(default)]
    pub default_session_command: Option<String>,
    /// Whether to operate in local-only mode (no GitHub API calls).
    #[serde(default)]
    pub local_mode: Option<bool>,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("octopai")
        .join("config.json")
}

pub fn load_config() -> Option<Config> {
    let path = config_path();
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_config(repo: &str) -> Result<()> {
    // Load existing config to preserve verify_commands, editor_commands, pr_ready
    let existing = load_config();
    let verify_commands = existing
        .as_ref()
        .map(|c| c.verify_commands.clone())
        .unwrap_or_default();
    let editor_commands = existing
        .as_ref()
        .map(|c| c.editor_commands.clone())
        .unwrap_or_default();
    let pr_ready = existing
        .as_ref()
        .map(|c| c.pr_ready.clone())
        .unwrap_or_default();
    let auto_open_pr = existing
        .as_ref()
        .map(|c| c.auto_open_pr.clone())
        .unwrap_or_default();
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let session_commands = existing
        .as_ref()
        .map(|c| c.session_commands.clone())
        .unwrap_or_default();
    let default_session_command = existing
        .as_ref()
        .and_then(|c| c.default_session_command.clone());
    let local_mode = existing.as_ref().and_then(|c| c.local_mode);
    let multiplexer = existing.and_then(|c| c.multiplexer);
    let config = Config {
        repo: repo.to_string(),
        verify_commands,
        editor_commands,
        pr_ready,
        auto_open_pr,
        session_commands,
        multiplexer,
        default_session_command,
        local_mode,
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
    let config = load_config();
    let saved = config.and_then(|c| c.editor_commands.get(repo).cloned());
    if saved.is_some() {
        return saved;
    }
    // Fall back to detected terminal + $EDITOR
    crate::session::default_editor_command()
}

pub fn set_editor_command(repo: &str, command: &str) -> Result<()> {
    let mut config = load_config().unwrap_or(Config {
        repo: repo.to_string(),
        verify_commands: HashMap::new(),
        editor_commands: HashMap::new(),
        pr_ready: HashMap::new(),
        auto_open_pr: HashMap::new(),
        session_commands: HashMap::new(),
        multiplexer: None,
        default_session_command: None,
        local_mode: None,
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
        pr_ready: HashMap::new(),
        auto_open_pr: HashMap::new(),
        session_commands: HashMap::new(),
        multiplexer: None,
        default_session_command: None,
        local_mode: None,
    });
    config
        .verify_commands
        .insert(repo.to_string(), command.to_string());
    save_full_config(&config)
}

pub fn get_pr_ready(repo: &str) -> bool {
    load_config()
        .and_then(|c| c.pr_ready.get(repo).copied())
        .unwrap_or(false)
}

pub fn get_auto_open_pr(repo: &str) -> bool {
    load_config()
        .and_then(|c| c.auto_open_pr.get(repo).copied())
        .unwrap_or(false)
}

pub fn get_session_command(repo: &str) -> Option<String> {
    let config = load_config()?;
    config.session_commands.get(repo).cloned()
}

pub fn get_multiplexer() -> Option<Multiplexer> {
    load_config()?.multiplexer
}

pub fn get_default_session_command() -> Option<String> {
    load_config()?.default_session_command
}

pub fn set_default_session_command(command: &str) -> Result<()> {
    let mut config = load_config().unwrap_or(Config {
        repo: String::new(),
        verify_commands: HashMap::new(),
        editor_commands: HashMap::new(),
        pr_ready: HashMap::new(),
        auto_open_pr: HashMap::new(),
        session_commands: HashMap::new(),
        multiplexer: None,
        default_session_command: None,
        local_mode: None,
    });
    config.default_session_command = Some(command.to_string());
    save_full_config(&config)
}

pub fn get_local_mode() -> bool {
    load_config().and_then(|c| c.local_mode).unwrap_or(false)
}

pub fn set_local_mode(enabled: bool) -> Result<()> {
    let mut config = load_config().unwrap_or(Config {
        repo: String::new(),
        verify_commands: HashMap::new(),
        editor_commands: HashMap::new(),
        pr_ready: HashMap::new(),
        auto_open_pr: HashMap::new(),
        session_commands: HashMap::new(),
        multiplexer: None,
        default_session_command: None,
        local_mode: None,
    });
    config.local_mode = Some(enabled);
    save_full_config(&config)
}
