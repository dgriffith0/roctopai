use std::process::Command;

use ratatui::style::Color;

use crate::models::{label_color, AssigneeFilter, Card, StateFilter};

pub fn fetch_repos(owner: &str) -> std::result::Result<Vec<String>, String> {
    let output = Command::new("gh")
        .args([
            "repo",
            "list",
            owner,
            "--json",
            "nameWithOwner",
            "--limit",
            "50",
            "-q",
            ".[].nameWithOwner",
        ])
        .output()
        .map_err(|e| format!("Failed to run gh: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh error: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let repos: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    if repos.is_empty() {
        return Err(format!("No repos found for '{}'", owner));
    }

    Ok(repos)
}

pub fn fetch_issues(repo: &str, state: StateFilter, assignee: AssigneeFilter) -> Vec<Card> {
    let mut args = vec![
        "issue".to_string(),
        "list".to_string(),
        "--repo".to_string(),
        repo.to_string(),
        "--state".to_string(),
        state.label().to_string(),
        "--json".to_string(),
        "number,title,body,labels,state".to_string(),
        "--limit".to_string(),
        "30".to_string(),
    ];
    if assignee == AssigneeFilter::Mine {
        args.push("--assignee".to_string());
        args.push("@me".to_string());
    }
    let output = Command::new("gh").args(&args).output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let issues: Vec<serde_json::Value> = match serde_json::from_str(&stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut cards: Vec<Card> = issues
        .into_iter()
        .map(|issue| {
            let number = issue["number"].as_u64().unwrap_or(0);
            let title = issue["title"].as_str().unwrap_or("").to_string();
            let body = issue["body"].as_str().unwrap_or("").to_string();
            let full_description = if body.is_empty() {
                None
            } else {
                Some(body.clone())
            };
            let description = if body.len() > 80 {
                format!("{}...", &body[..77])
            } else if body.is_empty() {
                "No description".to_string()
            } else {
                body
            };

            let labels = issue["labels"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|l| l["name"].as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let issue_state = issue["state"].as_str().unwrap_or("OPEN").to_uppercase();

            let (tag, tag_color) = if let Some(first) = labels.first() {
                (first.clone(), label_color(first))
            } else if issue_state == "CLOSED" {
                ("closed".to_string(), Color::Red)
            } else {
                ("open".to_string(), Color::Green)
            };

            Card {
                id: format!("issue-{}", number),
                title: format!("#{} {}", number, title),
                description,
                full_description,
                tag,
                tag_color,
                related: Vec::new(),
                url: None,
                pr_number: None,
                is_draft: None,
                is_merged: None,
                head_branch: None,
            }
        })
        .collect();
    // Reverse to show oldest first (gh returns newest first)
    cards.reverse();
    cards
}

pub fn fetch_prs(repo: &str, state: StateFilter, assignee: AssigneeFilter) -> Vec<Card> {
    let mut args = vec![
        "pr".to_string(),
        "list".to_string(),
        "--repo".to_string(),
        repo.to_string(),
        "--state".to_string(),
        state.label().to_string(),
        "--json".to_string(),
        "number,title,body,isDraft,url,headRefName,state,mergedAt".to_string(),
        "--limit".to_string(),
        "30".to_string(),
    ];
    if assignee == AssigneeFilter::Mine {
        args.push("--assignee".to_string());
        args.push("@me".to_string());
    }
    let output = Command::new("gh").args(&args).output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let prs: Vec<serde_json::Value> = match serde_json::from_str(&stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut cards: Vec<Card> = prs
        .into_iter()
        .map(|pr| {
            let number = pr["number"].as_u64().unwrap_or(0);
            let title = pr["title"].as_str().unwrap_or("").to_string();
            let body = pr["body"].as_str().unwrap_or("").to_string();
            let is_draft = pr["isDraft"].as_bool().unwrap_or(false);
            let url = pr["url"].as_str().unwrap_or("").to_string();
            let branch = pr["headRefName"].as_str().unwrap_or("").to_string();
            let is_merged = pr["mergedAt"].as_str().is_some();

            let description = if body.len() > 80 {
                format!("{}...", &body[..77])
            } else if body.is_empty() {
                branch.clone()
            } else {
                body
            };

            let (tag, tag_color) = if is_draft {
                ("draft", Color::DarkGray)
            } else {
                ("ready", Color::Green)
            };

            // Link to related issue if branch is issue-N
            let related = if let Some(num) = branch.strip_prefix("issue-") {
                vec![format!("issue-{}", num)]
            } else {
                Vec::new()
            };

            Card {
                id: format!("pr-{}", number),
                title: format!("#{} {}", number, title),
                description,
                full_description: None,
                tag: tag.to_string(),
                tag_color,
                related,
                url: Some(url),
                pr_number: Some(number),
                is_draft: Some(is_draft),
                is_merged: Some(is_merged),
                head_branch: Some(branch),
            }
        })
        .collect();
    // Reverse to show oldest first (gh returns newest first)
    cards.reverse();
    cards
}

pub fn create_issue(repo: &str, title: &str, body: &str) -> std::result::Result<u64, String> {
    let output = Command::new("gh")
        .args([
            "issue",
            "create",
            "--repo",
            repo,
            "--title",
            title,
            "--body",
            body,
            "--assignee",
            "@me",
        ])
        .output()
        .map_err(|e| format!("Failed to run gh: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh error: {}", stderr.trim()));
    }

    // gh issue create outputs a URL like https://github.com/owner/repo/issues/10
    let stdout = String::from_utf8_lossy(&output.stdout);
    let number = stdout
        .trim()
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| format!("Could not parse issue number from: {}", stdout.trim()))?;

    Ok(number)
}

pub fn close_issue(repo: &str, number: u64) -> std::result::Result<(), String> {
    let output = Command::new("gh")
        .args(["issue", "close", "--repo", repo, &number.to_string()])
        .output()
        .map_err(|e| format!("Failed to run gh: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh error: {}", stderr.trim()));
    }

    Ok(())
}

pub fn fetch_merged_pr_branches(repo: &str) -> Vec<String> {
    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--repo",
            repo,
            "--state",
            "merged",
            "--json",
            "headRefName",
            "--limit",
            "30",
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let prs: Vec<serde_json::Value> = match serde_json::from_str(&stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    prs.into_iter()
        .filter_map(|pr| pr["headRefName"].as_str().map(|s| s.to_string()))
        .collect()
}
