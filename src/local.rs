use std::fs;
use std::path::PathBuf;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::models::{label_color, AssigneeFilter, Card, StateFilter};

#[derive(Serialize, Deserialize, Clone)]
pub struct LocalIssue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String, // "open" or "closed"
    pub labels: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LocalPr {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub branch: String,
    pub state: String, // "open" or "merged"
    pub is_draft: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct LocalStore {
    pub issues: Vec<LocalIssue>,
    pub prs: Vec<LocalPr>,
    pub next_issue_number: u64,
    pub next_pr_number: u64,
}

fn repo_slug(repo: &str) -> String {
    repo.replace('/', "--")
}

fn store_dir(repo: &str) -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("octopai")
        .join("local")
        .join(repo_slug(repo))
}

fn store_path(repo: &str) -> PathBuf {
    store_dir(repo).join("store.json")
}

fn load_store(repo: &str) -> LocalStore {
    let path = store_path(repo);
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        LocalStore {
            next_issue_number: 1,
            next_pr_number: 1,
            ..Default::default()
        }
    }
}

fn save_store(repo: &str, store: &LocalStore) -> Result<(), String> {
    let dir = store_dir(repo);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create local store dir: {}", e))?;
    let path = store_path(repo);
    let data =
        serde_json::to_string_pretty(store).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(path, data).map_err(|e| format!("Failed to write local store: {}", e))?;
    Ok(())
}

pub fn fetch_local_issues(repo: &str, state: StateFilter, _assignee: AssigneeFilter) -> Vec<Card> {
    let store = load_store(repo);
    let state_label = state.label();
    let mut cards: Vec<Card> = store
        .issues
        .iter()
        .filter(|i| i.state == state_label)
        .map(|issue| {
            let description = if issue.body.len() > 80 {
                format!("{}...", &issue.body[..77])
            } else if issue.body.is_empty() {
                "No description".to_string()
            } else {
                issue.body.clone()
            };

            let full_description = if issue.body.is_empty() {
                None
            } else {
                Some(issue.body.clone())
            };

            let (tag, tag_color) = if let Some(first) = issue.labels.first() {
                (first.clone(), label_color(first))
            } else if issue.state == "closed" {
                ("closed".to_string(), Color::Red)
            } else {
                ("local".to_string(), Color::Cyan)
            };

            Card {
                id: format!("local-issue-{}", issue.number),
                title: format!("#{} {}", issue.number, issue.title),
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
                is_assigned: None,
            }
        })
        .collect();
    cards.reverse();
    cards
}

pub fn create_local_issue(repo: &str, title: &str, body: &str) -> Result<u64, String> {
    let mut store = load_store(repo);
    let number = store.next_issue_number;
    store.next_issue_number += 1;
    store.issues.push(LocalIssue {
        number,
        title: title.to_string(),
        body: body.to_string(),
        state: "open".to_string(),
        labels: Vec::new(),
    });
    save_store(repo, &store)?;
    Ok(number)
}

pub fn fetch_local_issue(repo: &str, number: u64) -> Result<(String, String), String> {
    let store = load_store(repo);
    store
        .issues
        .iter()
        .find(|i| i.number == number)
        .map(|i| (i.title.clone(), i.body.clone()))
        .ok_or_else(|| format!("Local issue #{} not found", number))
}

pub fn edit_local_issue(repo: &str, number: u64, title: &str, body: &str) -> Result<(), String> {
    let mut store = load_store(repo);
    if let Some(issue) = store.issues.iter_mut().find(|i| i.number == number) {
        issue.title = title.to_string();
        issue.body = body.to_string();
        save_store(repo, &store)?;
        Ok(())
    } else {
        Err(format!("Local issue #{} not found", number))
    }
}

pub fn close_local_issue(repo: &str, number: u64) -> Result<(), String> {
    let mut store = load_store(repo);
    if let Some(issue) = store.issues.iter_mut().find(|i| i.number == number) {
        issue.state = "closed".to_string();
        save_store(repo, &store)?;
        Ok(())
    } else {
        Err(format!("Local issue #{} not found", number))
    }
}

pub fn fetch_local_prs(repo: &str, state: StateFilter, _assignee: AssigneeFilter) -> Vec<Card> {
    let store = load_store(repo);
    let state_label = match state {
        StateFilter::Open => "open",
        StateFilter::Closed => "merged",
    };
    let mut cards: Vec<Card> = store
        .prs
        .iter()
        .filter(|pr| pr.state == state_label)
        .map(|pr| {
            let description = if pr.body.len() > 80 {
                format!("{}...", &pr.body[..77])
            } else if pr.body.is_empty() {
                pr.branch.clone()
            } else {
                pr.body.clone()
            };

            let (tag, tag_color) = if pr.is_draft {
                ("draft", Color::DarkGray)
            } else if pr.state == "merged" {
                ("merged", Color::Magenta)
            } else {
                ("local", Color::Cyan)
            };

            let related = if pr.branch.starts_with("local-issue-") {
                vec![pr.branch.clone()]
            } else if let Some(num) = pr.branch.strip_prefix("issue-") {
                vec![format!("issue-{}", num)]
            } else {
                Vec::new()
            };

            Card {
                id: format!("pr-{}", pr.number),
                title: format!("#{} {}", pr.number, pr.title),
                description,
                full_description: None,
                tag: tag.to_string(),
                tag_color,
                related,
                url: None,
                pr_number: Some(pr.number),
                is_draft: Some(pr.is_draft),
                is_merged: Some(pr.state == "merged"),
                head_branch: Some(pr.branch.clone()),
                is_assigned: None,
            }
        })
        .collect();
    cards.reverse();
    cards
}

pub fn create_local_pr(
    repo: &str,
    title: &str,
    body: &str,
    branch: &str,
    is_draft: bool,
) -> Result<u64, String> {
    let mut store = load_store(repo);
    let number = store.next_pr_number;
    store.next_pr_number += 1;
    store.prs.push(LocalPr {
        number,
        title: title.to_string(),
        body: body.to_string(),
        branch: branch.to_string(),
        state: "open".to_string(),
        is_draft,
    });
    save_store(repo, &store)?;
    Ok(number)
}

pub fn mark_local_pr_ready(repo: &str, number: u64) -> Result<(), String> {
    let mut store = load_store(repo);
    if let Some(pr) = store.prs.iter_mut().find(|p| p.number == number) {
        pr.is_draft = false;
        save_store(repo, &store)?;
        Ok(())
    } else {
        Err(format!("Local PR #{} not found", number))
    }
}

pub fn merge_local_pr(repo: &str, number: u64) -> Result<String, String> {
    let mut store = load_store(repo);
    if let Some(pr) = store.prs.iter_mut().find(|p| p.number == number) {
        if pr.state == "merged" {
            return Err("PR is already merged".to_string());
        }
        pr.state = "merged".to_string();
        let branch = pr.branch.clone();
        save_store(repo, &store)?;
        Ok(branch)
    } else {
        Err(format!("Local PR #{} not found", number))
    }
}

pub fn fetch_local_merged_pr_branches(repo: &str) -> Vec<String> {
    let store = load_store(repo);
    store
        .prs
        .iter()
        .filter(|pr| pr.state == "merged")
        .map(|pr| pr.branch.clone())
        .collect()
}

/// Check if a local PR already exists for a given branch.
pub fn has_local_pr_for_branch(repo: &str, branch: &str) -> bool {
    let store = load_store(repo);
    store
        .prs
        .iter()
        .any(|pr| pr.branch == branch && pr.state == "open")
}
