use std::collections::HashSet;
use std::fs;
use std::process::Command;

use ratatui::style::Color;

use crate::github::fetch_merged_pr_branches;
use crate::models::Card;
use crate::session::Multiplexer;

pub fn get_repo_name(repo: &str) -> &str {
    repo.split('/').next_back().unwrap_or(repo)
}

/// Extract the issue number from an issue-style identifier like "issue-42" or "local-issue-42".
/// Returns `None` if the string doesn't match either pattern.
pub fn extract_issue_number(s: &str) -> Option<u64> {
    s.strip_prefix("local-issue-")
        .or_else(|| s.strip_prefix("issue-"))
        .and_then(|n| n.parse().ok())
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
            path.split('/').next_back().unwrap_or(&path).to_string()
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

        // Link issue-N / local-issue-N worktrees to issue cards
        let related = if display_name.starts_with("local-issue-") {
            vec![display_name.clone()]
        } else if let Some(num) = display_name.strip_prefix("issue-") {
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
            is_assigned: None,
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

/// Pull the latest changes for the local main/master branch from origin.
/// Returns Ok(branch_name) on success or Err(message) on failure.
pub fn pull_main() -> std::result::Result<String, String> {
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
        return Err("Could not determine main branch".to_string());
    };

    let output = Command::new("git")
        .args(["pull", "origin", branch])
        .output()
        .map_err(|e| format!("Failed to run git pull: {}", e))?;

    if output.status.success() {
        Ok(branch.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("git pull failed: {}", stderr.trim()))
    }
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

/// Detect the repository name from the git remote origin URL.
/// Works without the `gh` CLI by parsing `git remote get-url origin`.
/// Returns a "owner/repo" style string derived from the URL.
pub fn detect_repo_from_git() -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_repo_from_url(&url)
}

/// Parse "owner/repo" from a git remote URL.
/// Handles both SSH (git@github.com:owner/repo.git) and HTTPS (https://github.com/owner/repo.git) formats,
/// as well as non-GitHub remotes (extracts the last two path segments).
fn parse_repo_from_url(url: &str) -> Option<String> {
    let url = url.trim_end_matches(".git");
    if let Some(path) = url.strip_prefix("git@") {
        // git@github.com:owner/repo
        let path = path.split(':').nth(1)?;
        Some(path.to_string())
    } else if url.starts_with("https://") || url.starts_with("http://") {
        // https://github.com/owner/repo
        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() >= 2 {
            let repo = parts[parts.len() - 1];
            let owner = parts[parts.len() - 2];
            Some(format!("{}/{}", owner, repo))
        } else {
            None
        }
    } else {
        None
    }
}

/// Merge a branch into the current branch (main/master) using git merge.
/// Used for local PR merging when not connected to GitHub.
pub fn merge_branch(branch: &str) -> std::result::Result<(), String> {
    let output = Command::new("git")
        .args(["merge", branch])
        .output()
        .map_err(|e| format!("Failed to run git merge: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("git merge failed: {}", stderr.trim()))
    }
}

/// Check if a branch has any commits ahead of main/master.
/// Used to detect if Claude has finished work on a local branch.
pub fn branch_has_commits(branch: &str) -> bool {
    // Determine main branch name
    let main_branch = if Command::new("git")
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
        return false;
    };

    let output = Command::new("git")
        .args([
            "rev-list",
            "--count",
            &format!("{}..{}", main_branch, branch),
        ])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let count: usize = String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse()
                .unwrap_or(0);
            count > 0
        }
        _ => false,
    }
}

/// Get the first commit message on a branch (ahead of main/master).
/// Used to generate a PR title for auto-created local PRs.
pub fn first_commit_summary(branch: &str) -> Option<String> {
    let main_branch = if Command::new("git")
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
        return None;
    };

    let output = Command::new("git")
        .args([
            "log",
            "--format=%s",
            "--reverse",
            &format!("{}..{}", main_branch, branch),
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().map(|s| s.to_string())
}

/// Clean up worktrees whose branches have been locally merged.
pub fn cleanup_local_merged_worktrees(
    merged_branches: &[String],
    worktrees: &[Card],
    mux: Multiplexer,
) -> Vec<String> {
    if merged_branches.is_empty() {
        return Vec::new();
    }

    let merged_set: HashSet<&str> = merged_branches.iter().map(|s| s.as_str()).collect();
    let mut cleaned = Vec::new();

    for wt in worktrees {
        if merged_set.contains(wt.title.as_str())
            && remove_worktree(&wt.description, &wt.title, mux).is_ok()
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
