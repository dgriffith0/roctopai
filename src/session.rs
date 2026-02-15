use std::fs;
use std::process::Command;

use ratatui::style::Color;

use crate::git::{get_repo_name, trust_directory};
use crate::hooks::write_worktree_hook_config;
use crate::models::{Card, SessionStates};

pub fn fetch_sessions(socket_states: &SessionStates) -> Vec<Card> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let states = socket_states.lock().unwrap_or_else(|e| e.into_inner());

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|name| !name.is_empty())
        .filter(|name| name.starts_with("issue-"))
        .map(|name| {
            // Use socket-derived state if available, otherwise fall back
            // to pane content detection.
            let claude_state = if let Some(status) = states.get(name) {
                status.as_str()
            } else {
                let pane_target = format!("{}:.1", name);
                let pane_content = Command::new("tmux")
                    .args(["capture-pane", "-t", &pane_target, "-p"])
                    .output()
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).to_string())
                        } else {
                            None
                        }
                    });

                if let Some(content) = pane_content {
                    let trimmed = content.trim_end();
                    let last_lines: Vec<&str> = trimmed.lines().rev().take(5).collect();
                    let waiting = last_lines.iter().any(|l| {
                        let l = l.trim();
                        l.starts_with('❯')
                            || l.starts_with('>')
                            || l.contains("What would you like")
                    });
                    if waiting {
                        "idle"
                    } else {
                        "working"
                    }
                } else {
                    "working"
                }
            };

            let (tag, tag_color, description) = match claude_state {
                "working" => ("working", Color::Green, "Claude is processing..."),
                "waiting" => ("waiting", Color::Yellow, "Waiting for input"),
                "idle" => ("idle", Color::Blue, "Ready for prompts"),
                _ => ("working", Color::Green, "Claude is processing..."),
            };

            // Link to the related issue card
            let related = vec![format!("{}", name)];

            Card {
                id: format!("session-{}", name),
                title: name.to_string(),
                description: description.to_string(),
                full_description: None,
                tag: tag.to_string(),
                tag_color,
                related,
                url: None,
                pr_number: None,
                is_draft: None,
                is_merged: None,
            }
        })
        .collect()
}

pub fn attach_tmux_session(session: &str) -> std::result::Result<(), String> {
    Command::new("tmux")
        .args(["attach-session", "-t", session])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| format!("Failed to attach: {}", e))?;
    Ok(())
}

pub fn create_worktree_and_session(
    repo: &str,
    number: u64,
    title: &str,
    body: &str,
    hook_script: Option<&str>,
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

    // Create tmux session with neovim in the first pane
    let output = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            &branch,
            "-c",
            &worktree_path,
            "nvim",
            ".",
        ])
        .output()
        .map_err(|e| format!("Failed to create tmux session: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tmux error: {}", stderr.trim()));
    }

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

    let prompt = format!(
        "You are working on GitHub issue #{} for the repo {}. Title: {}. {} Please investigate the codebase and implement a solution for this issue. When you are confident the problem is solved, commit your changes and open a draft pull request with a clear title and description that explains what was changed and why. Reference the issue with 'Closes #{}' in the PR body. Use '--assignee @me' when creating the pull request to auto-assign it.",
        number, repo, title, body_clean, number
    );

    // Write prompt to a temp file for safe shell expansion
    let prompt_file = format!("/tmp/roctopai-prompt-{}.txt", number);
    fs::write(&prompt_file, &prompt).map_err(|e| format!("Failed to write prompt file: {}", e))?;

    // Split right pane running Claude with the prompt via sh -c
    let shell_cmd = format!(
        "claude \"$(cat '{}')\" --allowedTools Read,Edit,Bash",
        prompt_file
    );

    let output = Command::new("tmux")
        .args(["split-window", "-h", "-t", &branch, "-c", &worktree_path])
        .output()
        .map_err(|e| format!("Failed to split tmux pane: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tmux split error: {}", stderr.trim()));
    }

    // Wait for shell to initialize, then send the command
    std::thread::sleep(std::time::Duration::from_millis(500));

    let pane_target = format!("{}:.1", branch);
    let _ = Command::new("tmux")
        .args(["send-keys", "-t", &pane_target, "-l", &shell_cmd])
        .output();
    let _ = Command::new("tmux")
        .args(["send-keys", "-t", &pane_target, "Enter"])
        .output();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tmux split error: {}", stderr.trim()));
    }

    Ok(())
}
