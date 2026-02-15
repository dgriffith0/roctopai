use std::collections::HashSet;
use std::fs;
use std::process::Command;

use ratatui::style::Color;

use crate::github::fetch_merged_pr_branches;
use crate::models::Card;

pub fn get_repo_name(repo: &str) -> &str {
    repo.split('/').last().unwrap_or(repo)
}

pub fn fetch_worktrees() -> Vec<Card> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut cards = Vec::new();

    for block in stdout.split("\n\n") {
        let mut path = String::new();
        let mut branch = String::new();
        let mut is_bare = false;

        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = p.to_string();
            } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                branch = b.to_string();
            } else if line == "bare" {
                is_bare = true;
            }
        }

        if path.is_empty() || is_bare {
            continue;
        }

        let display_name = if branch.is_empty() {
            path.split('/').last().unwrap_or(&path).to_string()
        } else {
            branch.clone()
        };

        // Skip the main/master worktree — that's where we run from
        let is_main = display_name == "main" || display_name == "master";
        if is_main {
            continue;
        }
        let tag = "branch";
        let tag_color = Color::Yellow;

        // Link issue-N worktrees to issue cards
        let related = if let Some(num) = display_name.strip_prefix("issue-") {
            vec![format!("issue-{}", num)]
        } else {
            Vec::new()
        };

        cards.push(Card {
            id: format!("wt-{}", display_name),
            title: display_name,
            description: path,
            full_description: None,
            tag: tag.to_string(),
            tag_color,
            related,
            url: None,
            pr_number: None,
            is_draft: None,
            is_merged: None,
        });
    }

    cards
}

pub fn remove_worktree(path: &str, branch: &str) -> std::result::Result<(), String> {
    // Kill tmux session if it exists (named after branch)
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", branch])
        .output();

    let output = Command::new("git")
        .args(["worktree", "remove", "--force", path])
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree remove error: {}", stderr.trim()));
    }

    // Delete the branch
    let _ = Command::new("git").args(["branch", "-D", branch]).output();

    Ok(())
}

pub fn cleanup_merged_worktrees(repo: &str, worktrees: &[Card]) -> Vec<String> {
    let merged_branches = fetch_merged_pr_branches(repo);
    if merged_branches.is_empty() {
        return Vec::new();
    }

    let merged_set: HashSet<&str> = merged_branches.iter().map(|s| s.as_str()).collect();
    let mut cleaned = Vec::new();

    for wt in worktrees {
        // worktree title is the branch name, description is the path
        if merged_set.contains(wt.title.as_str())
            && remove_worktree(&wt.description, &wt.title).is_ok()
        {
            cleaned.push(wt.title.clone());
        }
    }

    cleaned
}

pub fn trust_directory(path: &str) -> std::result::Result<(), String> {
    let claude_json = dirs::home_dir()
        .ok_or("Could not find home directory")?
        .join(".claude.json");

    let mut config: serde_json::Value = if claude_json.exists() {
        let data = fs::read_to_string(&claude_json)
            .map_err(|e| format!("Failed to read .claude.json: {}", e))?;
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse .claude.json: {}", e))?
    } else {
        serde_json::json!({})
    };

    let abs_path = fs::canonicalize(path)
        .map_err(|e| format!("Failed to resolve path: {}", e))?
        .to_string_lossy()
        .to_string();

    let projects = config
        .as_object_mut()
        .ok_or("Invalid .claude.json format")?
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}));

    let project = projects
        .as_object_mut()
        .ok_or("Invalid projects format")?
        .entry(&abs_path)
        .or_insert_with(|| serde_json::json!({}));

    project["hasTrustDialogAccepted"] = serde_json::json!(true);

    fs::write(&claude_json, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| format!("Failed to write .claude.json: {}", e))?;

    Ok(())
}
