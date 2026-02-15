use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ratatui::style::Color;

pub const SOCKET_PATH: &str = "/tmp/roctopai-events.sock";
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(30);

pub type SessionStates = Arc<Mutex<HashMap<String, String>>>;

pub struct Card {
    pub id: String,
    pub title: String,
    pub description: String,
    pub full_description: Option<String>,
    pub tag: String,
    pub tag_color: Color,
    pub related: Vec<String>,
    pub url: Option<String>,
    pub pr_number: Option<u64>,
    pub is_draft: Option<bool>,
    pub is_merged: Option<bool>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum StateFilter {
    Open,
    Closed,
}

impl StateFilter {
    pub fn toggle(self) -> Self {
        match self {
            StateFilter::Open => StateFilter::Closed,
            StateFilter::Closed => StateFilter::Open,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            StateFilter::Open => "open",
            StateFilter::Closed => "closed",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum AssigneeFilter {
    All,
    Mine,
}

impl AssigneeFilter {
    pub fn toggle(self) -> Self {
        match self {
            AssigneeFilter::All => AssigneeFilter::Mine,
            AssigneeFilter::Mine => AssigneeFilter::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AssigneeFilter::All => "all",
            AssigneeFilter::Mine => "mine",
        }
    }
}

pub enum MergeStrategy {
    Merge,
}

impl MergeStrategy {
    pub fn flag(&self) -> &str {
        match self {
            MergeStrategy::Merge => "--merge",
        }
    }

    pub fn label(&self) -> &str {
        match self {
            MergeStrategy::Merge => "merge",
        }
    }
}

pub enum ConfirmAction {
    CloseIssue {
        number: u64,
    },
    RemoveWorktree {
        path: String,
        branch: String,
    },
    KillSession {
        name: String,
    },
    MergePr {
        number: u64,
        strategy: MergeStrategy,
    },
    RevertPr {
        number: u64,
    },
}

pub struct ConfirmModal {
    pub message: String,
    pub on_confirm: ConfirmAction,
}

#[derive(PartialEq)]
pub enum Mode {
    Normal,
    Filtering { query: String },
    CreatingIssue,
    Confirming,
}

#[derive(PartialEq)]
pub enum Screen {
    RepoSelect,
    Board,
}

#[derive(PartialEq)]
pub enum RepoSelectPhase {
    Typing,
    Loading,
    Picking,
}

pub struct RepoSelectState {
    pub input: String,
    pub repos: Vec<String>,
    pub filtered_repos: Vec<String>,
    pub selected: usize,
    pub phase: RepoSelectPhase,
    pub error: Option<String>,
    pub filter_query: String,
}

impl RepoSelectState {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            repos: Vec::new(),
            filtered_repos: Vec::new(),
            selected: 0,
            phase: RepoSelectPhase::Typing,
            error: None,
            filter_query: String::new(),
        }
    }

    pub fn update_filtered(&mut self) {
        if self.filter_query.is_empty() {
            self.filtered_repos = self.repos.clone();
        } else {
            self.filtered_repos = self
                .repos
                .iter()
                .filter(|r| fuzzy_match(&self.filter_query, r))
                .cloned()
                .collect();
        }
        if self.selected >= self.filtered_repos.len() {
            self.selected = if self.filtered_repos.is_empty() {
                0
            } else {
                self.filtered_repos.len() - 1
            };
        }
    }
}

pub struct IssueModal {
    pub title: String,
    pub body: String,
    pub active_field: usize, // 0 = title, 1 = body
    pub error: Option<String>,
    pub submitting: bool,
}

impl IssueModal {
    pub fn new() -> Self {
        Self {
            title: String::new(),
            body: String::new(),
            active_field: 0,
            error: None,
            submitting: false,
        }
    }
}

pub enum IssueSubmitResult {
    Success {
        number: u64,
        worktree_result: std::result::Result<(), String>,
    },
    Error(String),
}

pub fn fuzzy_match(query: &str, target: &str) -> bool {
    let target_lower = target.to_lowercase();
    let mut target_chars = target_lower.chars();
    for qc in query.to_lowercase().chars() {
        loop {
            match target_chars.next() {
                Some(tc) if tc == qc => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

pub fn card_matches(card: &Card, query: &str) -> bool {
    fuzzy_match(query, &card.title) || fuzzy_match(query, &card.description)
}

pub fn label_color(name: &str) -> Color {
    match name.to_lowercase().as_str() {
        s if s.contains("bug") => Color::Red,
        s if s.contains("feature") || s.contains("enhancement") => Color::Green,
        s if s.contains("documentation") || s.contains("docs") => Color::Blue,
        s if s.contains("good first issue") || s.contains("help wanted") => Color::Cyan,
        s if s.contains("duplicate") || s.contains("wontfix") || s.contains("invalid") => {
            Color::Gray
        }
        s if s.contains("priority") || s.contains("critical") || s.contains("urgent") => {
            Color::LightRed
        }
        _ => Color::Yellow,
    }
}
