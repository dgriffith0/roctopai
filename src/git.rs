use std::collections::HashSet;
use std::fs;
use std::process::Command;

use ratatui::style::Color;

use crate::github::fetch_merged_pr_branches;
use crate::models::Card;
use crate::session::Multiplexer;

pub fn get_repo_name(repo: &str) -> &str {
    repo.split('/').last().unwrap_or(repo)
}

/// Detect the GitHub "owner/repo" for the current working directory by
/// asking `gh` which repository this directory belongs to.
pub fn detect_current_repo() -> Option<String> {
    let output = Command::new("gh")
        .args([
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let repo = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if repo.is_empty() || !repo.contains('/') {
        return None;
    }

    Some(repo)
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
            head_branch: None,
        });
    }

    cards
}

pub fn remove_worktree(
    path: &str,
    branch: &str,
    mux: Multiplexer,
) -> std::result::Result<(), String> {
    // Kill session if it exists (named after branch)
    mux.kill_session(branch);

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

pub fn cleanup_merged_worktrees(repo: &str, worktrees: &[Card], mux: Multiplexer) -> Vec<String> {
    let merged_branches = fetch_merged_pr_branches(repo);
    if merged_branches.is_empty() {
        return Vec::new();
    }

    let merged_set: HashSet<&str> = merged_branches.iter().map(|s| s.as_str()).collect();
    let mut cleaned = Vec::new();

    for wt in worktrees {
        // worktree title is the branch name, description is the path
        if merged_set.contains(wt.title.as_str())
            && remove_worktree(&wt.description, &wt.title, mux).is_ok()
        {
            cleaned.push(wt.title.clone());
        }
    }

    cleaned
}

/// Check how many commits the local main/master branch is behind its remote tracking branch.
/// Runs `git fetch` first to ensure we have the latest remote state.
pub fn fetch_main_behind_count() -> usize {
    // Fetch latest from remote (quiet, don't fail if offline)
    let _ = Command::new("git").args(["fetch", "--quiet"]).output();

    // Determine main branch name
    let branch = if Command::new("git")
        .args(["rev-parse", "--verify", "refs/heads/main"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        "main"
    } else if Command::new("git")
        .args(["rev-parse", "--verify", "refs/heads/master"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        "master"
    } else {
        return 0;
    };

    // Count commits that are on the remote but not on the local branch
    let local = format!("refs/heads/{}", branch);
    let remote = format!("refs/remotes/origin/{}", branch);
    let output = Command::new("git")
        .args(["rev-list", "--count", &format!("{}..{}", local, remote)])
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse::<usize>()
            .unwrap_or(0),
        _ => 0,
    }
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
