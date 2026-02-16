use std::collections::HashSet;
use std::sync::mpsc;
use std::time::Instant;

use crate::deps::Dependency;
use crate::git::{cleanup_merged_worktrees, fetch_main_behind_count, fetch_worktrees};
use crate::github::{fetch_issues, fetch_prs};
use crate::hooks::ensure_hook_script;
use crate::models::{
    AssigneeFilter, Card, ConfigEditState, ConfirmModal, IssueModal, IssueSubmitResult, MessageLog,
    Mode, RepoSelectState, Screen, SessionStates, StateFilter, MAX_MESSAGES,
};
use crate::session::fetch_sessions;

pub struct App {
    pub screen: Screen,
    pub repo_select: RepoSelectState,
    pub repo: String,
    pub issues: Vec<Card>,
    pub worktrees: Vec<Card>,
    pub pull_requests: Vec<Card>,
    pub sessions: Vec<Card>,
    pub active_section: usize,
    pub selected_card: [usize; 4],
    pub mode: Mode,
    pub issue_modal: Option<IssueModal>,
    pub confirm_modal: Option<ConfirmModal>,
    pub last_refresh: Instant,
    pub session_states: SessionStates,
    pub hook_script_path: Option<String>,
    pub issue_state_filter: StateFilter,
    pub issue_assignee_filter: AssigneeFilter,
    pub pr_state_filter: StateFilter,
    pub pr_assignee_filter: AssigneeFilter,
    pub issue_submit_rx: Option<mpsc::Receiver<IssueSubmitResult>>,
    pub spinner_tick: usize,
    pub dependencies: Vec<Dependency>,
    pub config_edit: Option<ConfigEditState>,
    pub message_log: MessageLog,
    pub show_messages: bool,
    pub messages_expanded: bool,
    pub main_behind_count: usize,
}

impl App {
    pub fn new(session_states: SessionStates, message_log: MessageLog) -> Self {
        let hook_script_path = ensure_hook_script()
            .ok()
            .map(|p| p.to_string_lossy().to_string());
        Self {
            screen: Screen::RepoSelect,
            repo_select: RepoSelectState::new(),
            repo: String::new(),
            issues: Vec::new(),
            worktrees: Vec::new(),
            pull_requests: Vec::new(),
            active_section: 0,
            selected_card: [0; 4],
            mode: Mode::Normal,
            issue_modal: None,
            confirm_modal: None,
            sessions: Vec::new(),
            last_refresh: Instant::now(),
            session_states,
            hook_script_path,
            issue_state_filter: StateFilter::Open,
            issue_assignee_filter: AssigneeFilter::All,
            pr_state_filter: StateFilter::Open,
            pr_assignee_filter: AssigneeFilter::All,
            issue_submit_rx: None,
            spinner_tick: 0,
            dependencies: Vec::new(),
            config_edit: None,
            message_log: message_log.clone(),
            show_messages: true,
            messages_expanded: false,
            main_behind_count: 0,
        }
    }

    pub fn add_message(&self, msg: &str) {
        if let Ok(mut log) = self.message_log.lock() {
            log.push_back(msg.to_string());
            while log.len() > MAX_MESSAGES {
                log.pop_front();
            }
        }
    }

    pub fn set_status(&mut self, msg: String) {
        self.add_message(&msg);
    }

    pub fn section_cards(&self, section: usize) -> &[Card] {
        match section {
            0 => &self.issues,
            1 => &self.worktrees,
            2 => &self.sessions,
            3 => &self.pull_requests,
            _ => &[],
        }
    }

    pub fn section_card_count(&self, section: usize) -> usize {
        self.section_cards(section).len()
    }

    pub fn clamp_selected(&mut self) {
        let s = self.active_section;
        let count = self.section_card_count(s);
        if count == 0 {
            self.selected_card[s] = 0;
        } else if self.selected_card[s] >= count {
            self.selected_card[s] = count - 1;
        }
    }

    pub fn move_card_up(&mut self) {
        let s = self.active_section;
        if self.selected_card[s] > 0 {
            self.selected_card[s] -= 1;
        }
    }

    pub fn move_card_down(&mut self) {
        let s = self.active_section;
        let count = self.section_card_count(s);
        if count > 0 && self.selected_card[s] < count - 1 {
            self.selected_card[s] += 1;
        }
    }

    pub fn selected_card_related_ids(&self) -> HashSet<String> {
        let cards = self.section_cards(self.active_section);
        let idx = self.selected_card[self.active_section];
        let card = match cards.get(idx) {
            Some(c) => c,
            None => return HashSet::new(),
        };

        // Find the issue key that ties all related cards together.
        // If this IS an issue card, its own id is the key.
        // Otherwise, look for an issue-N entry in the related field.
        let issue_key = if card.id.starts_with("issue-") {
            Some(card.id.clone())
        } else {
            card.related
                .iter()
                .find(|r| r.starts_with("issue-"))
                .cloned()
        };

        let Some(key) = issue_key else {
            return card.related.iter().cloned().collect();
        };

        // Collect all cards across every column whose id matches the key
        // or whose related list contains the key, excluding the selected card.
        let mut ids = HashSet::new();
        for section in 0..4 {
            for c in self.section_cards(section) {
                if c.id == card.id {
                    continue;
                }
                if c.id == key || c.related.contains(&key) {
                    ids.insert(c.id.clone());
                }
            }
        }
        ids
    }

    pub fn refresh_data(&mut self) {
        self.issues = fetch_issues(
            &self.repo,
            self.issue_state_filter,
            self.issue_assignee_filter,
        );
        self.pull_requests = fetch_prs(&self.repo, self.pr_state_filter, self.pr_assignee_filter);
        self.worktrees = fetch_worktrees();

        // Clean up worktrees and sessions for merged PRs
        let cleaned = cleanup_merged_worktrees(&self.repo, &self.worktrees);
        if !cleaned.is_empty() {
            self.set_status(format!("Cleaned up merged: {}", cleaned.join(", ")));
            // Re-fetch worktrees after cleanup
            self.worktrees = fetch_worktrees();
        }

        self.sessions = fetch_sessions(&self.session_states);
        self.main_behind_count = fetch_main_behind_count();
        self.clamp_selected();
        self.last_refresh = Instant::now();
    }

    pub fn enter_repo_select(&mut self) {
        let owner = if self.repo.contains('/') {
            self.repo.split('/').next().unwrap_or("").to_string()
        } else {
            String::new()
        };
        self.repo_select = RepoSelectState::new();
        self.repo_select.input = crate::models::TextInput::from(owner);
        self.screen = Screen::RepoSelect;
    }
}
