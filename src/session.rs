use std::fs;
use std::process::Command;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::git::{get_repo_name, trust_directory};
use crate::hooks::write_worktree_hook_config;
use crate::models::{Card, SessionStates};

/// Session name for the main worktree exploration session.
pub const MAIN_SESSION_NAME: &str = "main-explore";

/// Supported terminal multiplexers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Multiplexer {
    Tmux,
    Screen,
}

fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

impl Multiplexer {
    /// Detect the available multiplexer, preferring tmux over GNU Screen.
    pub fn detect() -> Option<Self> {
        if command_exists("tmux") {
            Some(Multiplexer::Tmux)
        } else if command_exists("screen") {
            Some(Multiplexer::Screen)
        } else {
            None
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Multiplexer::Tmux => "tmux",
            Multiplexer::Screen => "screen",
        }
    }

    /// List session names managed by this multiplexer.
    pub fn list_sessions(self) -> Vec<String> {
        match self {
            Multiplexer::Tmux => {
                let output = Command::new("tmux")
                    .args(["list-sessions", "-F", "#{session_name}"])
                    .output();
                match output {
                    Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect(),
                    _ => Vec::new(),
                }
            }
            Multiplexer::Screen => {
                let output = Command::new("screen").args(["-ls"]).output();
                // screen -ls exits with non-zero when sessions exist, so
                // just parse stdout regardless of exit code.
                match output {
                    Ok(o) => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        stdout
                            .lines()
                            .filter_map(|line| {
                                // Lines look like: "\t12345.session_name\t(Detached)"
                                let trimmed = line.trim();
                                if trimmed.contains('.')
                                    && (trimmed.contains("Detached")
                                        || trimmed.contains("Attached"))
                                {
                                    // Extract session name after the pid dot
                                    let after_tab = trimmed.split_whitespace().next()?;
                                    let name = after_tab.split('.').nth(1)?;
                                    Some(name.to_string())
                                } else {
                                    None
                                }
                            })
                            .collect()
                    }
                    Err(_) => Vec::new(),
                }
            }
        }
    }

    /// Capture the visible pane content for state detection.
    pub fn capture_pane(self, session: &str) -> Option<String> {
        match self {
            Multiplexer::Tmux => {
                let pane_target = format!("{}:.0", session);
                Command::new("tmux")
                    .args(["capture-pane", "-t", &pane_target, "-p"])
                    .output()
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).to_string())
                        } else {
                            None
                        }
                    })
            }
            Multiplexer::Screen => {
                let capture_file = format!("/tmp/octopai-screen-capture-{}.txt", session);
                // Tell screen to dump the current window to a file
                let _ = Command::new("screen")
                    .args(["-S", session, "-X", "hardcopy", &capture_file])
                    .output();
                let content = fs::read_to_string(&capture_file).ok();
                let _ = fs::remove_file(&capture_file);
                content
            }
        }
    }

    /// Create a new detached session with a shell in the given directory.
    pub fn create_session(self, name: &str, working_dir: &str) -> Result<(), String> {
        match self {
            Multiplexer::Tmux => {
                let output = Command::new("tmux")
                    .args(["new-session", "-d", "-s", name, "-c", working_dir])
                    .output()
                    .map_err(|e| format!("Failed to create tmux session: {}", e))?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(format!("tmux error: {}", stderr.trim()));
                }
                Ok(())
            }
            Multiplexer::Screen => {
                // GNU Screen doesn't have a built-in -c for working directory,
                // so we start a shell that cd's into the directory first.
                let shell_cmd = format!("cd '{}' && exec $SHELL", working_dir);
                let output = Command::new("screen")
                    .args(["-dmS", name, "sh", "-c", &shell_cmd])
                    .output()
                    .map_err(|e| format!("Failed to create screen session: {}", e))?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(format!("screen error: {}", stderr.trim()));
                }
                Ok(())
            }
        }
    }

    /// Send a command string to the session's active pane.
    pub fn send_keys(self, session: &str, cmd: &str) {
        match self {
            Multiplexer::Tmux => {
                let pane_target = format!("{}:.0", session);
                let _ = Command::new("tmux")
                    .args(["send-keys", "-t", &pane_target, "-l", cmd])
                    .output();
                let _ = Command::new("tmux")
                    .args(["send-keys", "-t", &pane_target, "Enter"])
                    .output();
            }
            Multiplexer::Screen => {
                // screen -X stuff sends literal characters; append \n for Enter.
                let stuffed = format!("{}\n", cmd);
                let _ = Command::new("screen")
                    .args(["-S", session, "-X", "stuff", &stuffed])
                    .output();
            }
        }
    }

    /// Attach to an existing session (blocks until detach).
    pub fn attach(self, session: &str) -> Result<(), String> {
        match self {
            Multiplexer::Tmux => {
                Command::new("tmux")
                    .args(["attach-session", "-t", session])
                    .stdin(std::process::Stdio::inherit())
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .map_err(|e| format!("Failed to attach: {}", e))?;
                Ok(())
            }
            Multiplexer::Screen => {
                Command::new("screen")
                    .args(["-r", session])
                    .stdin(std::process::Stdio::inherit())
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .status()
                    .map_err(|e| format!("Failed to attach: {}", e))?;
                Ok(())
            }
        }
    }

    /// Kill a session by name.
    pub fn kill_session(self, session: &str) {
        match self {
            Multiplexer::Tmux => {
                let _ = Command::new("tmux")
                    .args(["kill-session", "-t", session])
                    .output();
            }
            Multiplexer::Screen => {
                let _ = Command::new("screen")
                    .args(["-S", session, "-X", "quit"])
                    .output();
            }
        }
    }
}

pub fn fetch_sessions(socket_states: &SessionStates, mux: Multiplexer) -> Vec<Card> {
    let session_names = mux.list_sessions();
    let states = socket_states.lock().unwrap_or_else(|e| e.into_inner());

    session_names
        .into_iter()
        .filter(|name| name.starts_with("issue-"))
        .map(|name| {
            // Use socket-derived state if available, otherwise fall back
            // to pane content detection.
            let claude_state = if let Some(status) = states.get(&name) {
                status.as_str()
            } else {
                let pane_content = mux.capture_pane(&name);

                if let Some(content) = pane_content {
                    let trimmed = content.trim_end();
                    let last_lines: Vec<&str> = trimmed.lines().rev().take(5).collect();
                    let has_permission = last_lines.iter().any(|l| {
                        let l = l.trim();
                        l.contains("Allow") || l.contains("Deny") || l.contains("allow once")
                    });
                    let has_prompt = last_lines.iter().any(|l| {
                        let l = l.trim();
                        l.starts_with('❯')
                            || l.starts_with('>')
                            || l.contains("What would you like")
                    });
                    if has_permission {
                        "permission"
                    } else if has_prompt {
                        "idle"
                    } else {
                        "working"
                    }
                } else {
                    "working"
                }
            };

            let (tag, tag_color, description) = match claude_state {
                "processing" => ("processing", Color::Cyan, "Processing..."),
                "working" => ("working", Color::Green, "Using tools..."),
                "permission" => ("permission", Color::Yellow, "Awaiting permission"),
                "idle" => ("idle", Color::DarkGray, "Waiting for prompt"),
                _ => ("unknown", Color::DarkGray, "Unknown state"),
            };

            // Link to the related issue card
            let related = vec![name.clone()];

            Card {
                id: format!("session-{}", name),
                title: name,
                description: description.to_string(),
                full_description: None,
                tag: tag.to_string(),
                tag_color,
                related,
                url: None,
                pr_number: None,
                is_draft: None,
                is_merged: None,
                head_branch: None,
            }
        })
        .collect()
}

/// Default command template for Claude Code sessions.
pub const DEFAULT_CLAUDE_COMMAND: &str =
    "claude \"$(cat '{prompt_file}')\" --allowedTools Read,Edit,Bash --max-turns 50";

/// Default command template for Cursor sessions.
pub const DEFAULT_CURSOR_COMMAND: &str = "cursor-agent \"$(cat '{prompt_file}')\"";

/// Available template fields for the session command configuration.
/// Each tuple is (field_name, description).
pub const TEMPLATE_FIELDS: &[(&str, &str)] = &[
    (
        "{prompt_file}",
        "Path to temp file containing the prompt (issue number, repo, title, body, and instructions to implement a fix, commit, and open a PR)",
    ),
    ("{issue_number}", "GitHub issue number"),
    ("{repo}", "Full repo name (owner/repo)"),
    ("{title}", "Issue title"),
    ("{body}", "Cleaned issue body text"),
    ("{branch}", "Branch name (e.g. issue-42)"),
    ("{worktree_path}", "Path to the git worktree"),
];

/// Default editor command template. Users can override this entirely.
pub const DEFAULT_EDITOR_COMMAND: &str = "{alacritty} nvim";

/// Available template fields for editor and verify command configuration.
pub const EDITOR_TEMPLATE_FIELDS: &[(&str, &str)] =
    &[("{directory}", "Path to the worktree directory")];

/// Shortcut templates for session commands that expand to full AI assistant commands.
/// Each shortcut expands to include `{prompt_file}` which is then resolved
/// during template expansion.
pub const SESSION_SHORTCUTS: &[(&str, &str, &str)] = &[
    (
        "{claude}",
        DEFAULT_CLAUDE_COMMAND,
        "Claude Code CLI with prompt file",
    ),
    (
        "{cursor}",
        DEFAULT_CURSOR_COMMAND,
        "Cursor CLI with prompt file",
    ),
];

/// Shortcut templates that expand to common terminal emulator prefixes.
/// Each shortcut expands to include `{directory}` which is then resolved
/// in a second pass.
pub const COMMAND_SHORTCUTS: &[(&str, &str, &str)] = &[
    (
        "{alacritty}",
        "alacritty --working-directory {directory} -e",
        "Alacritty terminal with working directory",
    ),
    (
        "{kitty}",
        "kitty -d {directory} -e",
        "Kitty terminal with working directory",
    ),
    (
        "{wezterm}",
        "wezterm start --cwd {directory} --",
        "WezTerm terminal with working directory",
    ),
];

/// Known terminal emulators and their command prefixes for launching with
/// a working directory and running a command. Used for auto-detection when
/// no editor command is configured.
const KNOWN_TERMINALS: &[(&str, &str)] = &[
    ("alacritty", "alacritty --working-directory {directory} -e"),
    ("kitty", "kitty -d {directory} -e"),
    ("wezterm", "wezterm start --cwd {directory} --"),
];

/// Detect the default terminal emulator command prefix.
///
/// Checks `$TERMINAL` first, then probes for known terminals on `$PATH`.
/// Returns the command prefix with `{directory}` placeholder, or `None`.
pub fn detect_terminal() -> Option<String> {
    // Check $TERMINAL environment variable
    if let Ok(terminal) = std::env::var("TERMINAL") {
        // If it matches a known terminal, use the full prefix
        for (name, prefix) in KNOWN_TERMINALS {
            if terminal == *name || terminal.ends_with(&format!("/{name}")) {
                return Some(prefix.to_string());
            }
        }
        // Unknown terminal: assume it accepts a command as trailing args
        return Some(format!("{terminal} "));
    }

    // Probe for known terminals on PATH
    for (name, prefix) in KNOWN_TERMINALS {
        if Command::new("which")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(prefix.to_string());
        }
    }

    None
}

/// Build a default editor command from `$EDITOR` and a detected terminal.
///
/// Returns `Some("terminal_prefix $EDITOR {directory}")` when both are
/// available, or `None` otherwise.
pub fn default_editor_command() -> Option<String> {
    let editor = std::env::var("EDITOR").ok()?;
    let terminal_prefix = detect_terminal()?;
    Some(format!("{terminal_prefix} {editor} {{directory}}"))
}

/// Expand shortcut templates in a command string.
fn expand_shortcuts(template: &str) -> String {
    let mut result = template.to_string();
    for (shortcut, expansion, _) in COMMAND_SHORTCUTS {
        result = result.replace(shortcut, expansion);
    }
    result
}

/// Expand template fields in an editor or verify command string.
/// Shortcut templates (e.g. `{alacritty}`) are expanded first, then
/// `{directory}` is resolved.
pub fn expand_editor_command(template: &str, directory: &str) -> String {
    let expanded = expand_shortcuts(template);
    expanded.replace("{directory}", directory)
}

/// Expand template fields in a command string.
/// Session shortcuts (e.g. `{claude}`, `{cursor}`) are expanded first,
/// then all other template fields are resolved.
#[allow(clippy::too_many_arguments)]
fn expand_template(
    template: &str,
    prompt_file: &str,
    number: u64,
    repo: &str,
    title: &str,
    body: &str,
    branch: &str,
    worktree_path: &str,
) -> String {
    // Expand session shortcuts first (e.g. {claude} -> full claude command)
    let mut expanded = template.to_string();
    for (shortcut, expansion, _) in SESSION_SHORTCUTS {
        expanded = expanded.replace(shortcut, expansion);
    }
    expanded
        .replace("{prompt_file}", prompt_file)
        .replace("{issue_number}", &number.to_string())
        .replace("{repo}", repo)
        .replace("{title}", title)
        .replace("{body}", body)
        .replace("{branch}", branch)
        .replace("{worktree_path}", worktree_path)
}

/// Create a new multiplexer session for an existing worktree.
///
/// Unlike `create_worktree_and_session`, this does not create the worktree or
/// branch — it assumes they already exist. It sets up hooks, trusts the
/// directory, builds the prompt, and launches the session command.
#[allow(clippy::too_many_arguments)]
pub fn create_session_for_worktree(
    repo: &str,
    number: u64,
    title: &str,
    body: &str,
    branch: &str,
    worktree_path: &str,
    hook_script: Option<&str>,
    pr_ready: bool,
    auto_open_pr: bool,
    session_command: Option<&str>,
    mux: Multiplexer,
) -> std::result::Result<(), String> {
    // Pre-trust the worktree directory for Claude
    let _ = trust_directory(worktree_path);

    // Write Claude hook config for event socket integration
    if let Some(script) = hook_script {
        let _ = write_worktree_hook_config(worktree_path, script);
    }

    // Auto-assign the issue to the current user
    let _ = Command::new("gh")
        .args([
            "issue",
            "edit",
            "--repo",
            repo,
            &number.to_string(),
            "--add-assignee",
            "@me",
        ])
        .output();

    // Create session with a shell in the worktree directory
    mux.create_session(branch, worktree_path)?;

    // Build the Claude prompt and write to a temp file
    let body_clean = if body.is_empty() {
        "No description provided.".to_string()
    } else {
        body.lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };

    let prompt = if auto_open_pr {
        let pr_instruction = if pr_ready {
            "open a pull request"
        } else {
            "open a draft pull request"
        };
        format!(
            "You are working on GitHub issue #{} for the repo {}. Title: {}. {} Please investigate the codebase and implement a solution for this issue. When you are confident the problem is solved, commit your changes and {} with a clear title and description that explains what was changed and why. Reference the issue with 'Closes #{}' in the PR body. Use '--assignee @me' when creating the pull request to auto-assign it.",
            number, repo, title, body_clean, pr_instruction, number
        )
    } else {
        format!(
            "You are working on GitHub issue #{} for the repo {}. Title: {}. {} Please investigate the codebase and implement a solution for this issue. When you are confident the problem is solved, commit your changes and push the branch.",
            number, repo, title, body_clean
        )
    };

    // Write prompt to a temp file for safe shell expansion
    let prompt_file = format!("/tmp/octopai-prompt-{}.txt", number);
    fs::write(&prompt_file, &prompt).map_err(|e| format!("Failed to write prompt file: {}", e))?;

    // Send session command to the single pane
    let global_default = crate::config::get_default_session_command();
    let template = session_command
        .or(global_default.as_deref())
        .unwrap_or(DEFAULT_CLAUDE_COMMAND);
    let shell_cmd = expand_template(
        template,
        &prompt_file,
        number,
        repo,
        title,
        &body_clean,
        branch,
        worktree_path,
    );

    // Wait for shell to initialize, then send the command
    std::thread::sleep(std::time::Duration::from_millis(500));

    mux.send_keys(branch, &shell_cmd);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn create_worktree_and_session(
    repo: &str,
    number: u64,
    title: &str,
    body: &str,
    hook_script: Option<&str>,
    pr_ready: bool,
    auto_open_pr: bool,
    session_command: Option<&str>,
    mux: Multiplexer,
) -> std::result::Result<(), String> {
    let repo_name = get_repo_name(repo);
    let branch = format!("issue-{}", number);
    let worktree_path = format!("../{}-issue-{}", repo_name, number);

    // Create worktree with new branch
    let output = Command::new("git")
        .args(["worktree", "add", &worktree_path, "-b", &branch])
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree add error: {}", stderr.trim()));
    }

    // Pre-trust the worktree directory for Claude
    let _ = trust_directory(&worktree_path);

    // Write Claude hook config for event socket integration
    if let Some(script) = hook_script {
        let _ = write_worktree_hook_config(&worktree_path, script);
    }

    // Auto-assign the issue to the current user
    let _ = Command::new("gh")
        .args([
            "issue",
            "edit",
            "--repo",
            repo,
            &number.to_string(),
            "--add-assignee",
            "@me",
        ])
        .output();

    // Create session with a shell in the worktree directory
    mux.create_session(&branch, &worktree_path)?;

    // Build the Claude prompt and write to a temp file
    let body_clean = if body.is_empty() {
        "No description provided.".to_string()
    } else {
        body.lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };

    let prompt = if auto_open_pr {
        let pr_instruction = if pr_ready {
            "open a pull request"
        } else {
            "open a draft pull request"
        };
        format!(
            "You are working on GitHub issue #{} for the repo {}. Title: {}. {} Please investigate the codebase and implement a solution for this issue. When you are confident the problem is solved, commit your changes and {} with a clear title and description that explains what was changed and why. Reference the issue with 'Closes #{}' in the PR body. Use '--assignee @me' when creating the pull request to auto-assign it.",
            number, repo, title, body_clean, pr_instruction, number
        )
    } else {
        format!(
            "You are working on GitHub issue #{} for the repo {}. Title: {}. {} Please investigate the codebase and implement a solution for this issue. When you are confident the problem is solved, commit your changes and push the branch.",
            number, repo, title, body_clean
        )
    };

    // Write prompt to a temp file for safe shell expansion
    let prompt_file = format!("/tmp/octopai-prompt-{}.txt", number);
    fs::write(&prompt_file, &prompt).map_err(|e| format!("Failed to write prompt file: {}", e))?;

    // Send session command to the single pane
    let global_default = crate::config::get_default_session_command();
    let template = session_command
        .or(global_default.as_deref())
        .unwrap_or(DEFAULT_CLAUDE_COMMAND);
    let shell_cmd = expand_template(
        template,
        &prompt_file,
        number,
        repo,
        title,
        &body_clean,
        &branch,
        &worktree_path,
    );

    // Wait for shell to initialize, then send the command
    std::thread::sleep(std::time::Duration::from_millis(500));

    mux.send_keys(&branch, &shell_cmd);

    Ok(())
}

/// Create a Claude session on the main worktree for exploration (no prompt).
///
/// Returns `true` if a new session was created, `false` if one already existed.
pub fn ensure_main_session(mux: Multiplexer) -> Result<bool, String> {
    let existing = mux.list_sessions();
    if existing.iter().any(|s| s == MAIN_SESSION_NAME) {
        return Ok(false);
    }

    // Use the current working directory (the main worktree)
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?
        .to_string_lossy()
        .to_string();

    mux.create_session(MAIN_SESSION_NAME, &cwd)?;

    // Wait for shell to initialize, then launch claude with no prompt
    std::thread::sleep(std::time::Duration::from_millis(500));
    mux.send_keys(MAIN_SESSION_NAME, "claude");

    Ok(true)
}
