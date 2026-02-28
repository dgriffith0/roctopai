use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::Instant;

use crate::deps::Dependency;
use crate::git::{cleanup_merged_worktrees, fetch_main_behind_count, fetch_worktrees};
use crate::github::{assign_pr, fetch_prs};

use crate::hooks::ensure_hook_script;
use crate::models::{
    AiSetupState, AssigneeFilter, Card, ConfigEditState, ConfirmModal, DepInstallConfirm,
    EditIssueModal, IssueEditResult, IssueModal, IssueSubmitResult, MessageLog, Mode,
    RepoSelectState, Screen, SectionData, SessionStates, StateFilter, WorktreeCreateResult,
    MAX_MESSAGES,
};
use crate::session::{fetch_sessions, Multiplexer};

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
    pub issue_edit_rx: Option<mpsc::Receiver<IssueEditResult>>,
    pub edit_issue_modal: Option<EditIssueModal>,
    pub worktree_create_rx: Option<mpsc::Receiver<WorktreeCreateResult>>,
    pub loading_message: Option<String>,
    pub spinner_tick: usize,
    pub dependencies: Vec<Dependency>,
    pub config_edit: Option<ConfigEditState>,
    pub message_log: MessageLog,
    pub show_messages: bool,
    pub messages_expanded: bool,
    pub pending_refresh: Option<Instant>,
    pub main_behind_count: usize,
    pub multiplexer: Multiplexer,
    /// Tracks sessions that have been nudged to continue (to avoid repeated nudges).
    /// Maps branch name to the number of nudges sent.
    pub nudged_sessions: HashMap<String, usize>,
    pub ai_setup: Option<AiSetupState>,
    pub local_mode: bool,
    pub dep_selected: usize,
    pub dep_install_confirm: Option<DepInstallConfirm>,
    /// Server-side search query for GitHub issues.
    pub issue_search_query: Option<String>,
    /// Per-section loading state: [issues, worktrees, sessions, pull_requests].
    pub section_loading: [bool; 4],
    /// Receiver for per-section async refresh results.
    pub section_rx: Option<mpsc::Receiver<SectionData>>,
}

impl App {
    pub fn new(
        session_states: SessionStates,
        message_log: MessageLog,
        multiplexer: Multiplexer,
    ) -> Self {
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
            pr_assignee_filter: AssigneeFilter::Mine,
            issue_submit_rx: None,
            issue_edit_rx: None,
            edit_issue_modal: None,
            worktree_create_rx: None,
            loading_message: None,
            spinner_tick: 0,
            dependencies: Vec::new(),
            config_edit: None,
            message_log: message_log.clone(),
            show_messages: true,
            messages_expanded: false,
            pending_refresh: None,
            main_behind_count: 0,
            multiplexer,
            nudged_sessions: HashMap::new(),
            ai_setup: None,
            local_mode: false,
            dep_selected: 0,
            dep_install_confirm: None,
            issue_search_query: None,
            section_loading: [false; 4],
            section_rx: None,
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
        // Otherwise, look for an issue-N or local-issue-N entry in the related field.
        let issue_key = if card.id.starts_with("local-issue-") || card.id.starts_with("issue-") {
            Some(card.id.clone())
        } else {
            card.related
                .iter()
                .find(|r| r.starts_with("local-issue-") || r.starts_with("issue-"))
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
        if self.local_mode {
            self.issues = crate::local::fetch_local_issues(
                &self.repo,
                self.issue_state_filter,
                self.issue_assignee_filter,
            );
            self.pull_requests = crate::local::fetch_local_prs(
                &self.repo,
                self.pr_state_filter,
                self.pr_assignee_filter,
            );
        } else {
            self.issues = crate::github::fetch_issues(
                &self.repo,
                self.issue_state_filter,
                self.issue_assignee_filter,
                self.issue_search_query.as_deref(),
            );
            self.pull_requests =
                fetch_prs(&self.repo, self.pr_state_filter, self.pr_assignee_filter);

            // Auto-assign unassigned PRs to the current user
            for card in &self.pull_requests {
                if card.is_assigned == Some(false) {
                    if let Some(number) = card.pr_number {
                        assign_pr(&self.repo, number);
                    }
                }
            }
        }
        self.worktrees = fetch_worktrees();

        if self.local_mode {
            // Clean up worktrees for locally merged PRs
            let merged = crate::local::fetch_local_merged_pr_branches(&self.repo);
            let cleaned = crate::git::cleanup_local_merged_worktrees(
                &merged,
                &self.worktrees,
                self.multiplexer,
            );
            if !cleaned.is_empty() {
                self.set_status(format!("Cleaned up merged: {}", cleaned.join(", ")));
                self.worktrees = fetch_worktrees();
            }
        } else {
            // Clean up worktrees and sessions for merged PRs
            let cleaned = cleanup_merged_worktrees(&self.repo, &self.worktrees, self.multiplexer);
            if !cleaned.is_empty() {
                self.set_status(format!("Cleaned up merged: {}", cleaned.join(", ")));
                // Re-fetch worktrees after cleanup
                self.worktrees = fetch_worktrees();
            }
        }

        self.sessions = fetch_sessions(&self.session_states, self.multiplexer);
        self.main_behind_count = fetch_main_behind_count();

        // Auto-nudge idle sessions that have no associated PR.
        // Only nudge once per session to avoid spamming.
        // In local mode, auto-create a local PR instead of nudging when
        // the branch has commits.
        let max_nudges = 1;

        // Collect actions first to avoid borrow conflicts with self.
        enum SessionAction {
            ClearNudge(String),
            AutoCreateLocalPr(String),
            Nudge(String),
        }
        let mut actions = Vec::new();
        for session in &self.sessions {
            if session.tag != "idle" {
                continue;
            }
            let branch = &session.title; // e.g. "issue-42"
            let has_pr = self
                .pull_requests
                .iter()
                .any(|pr| pr.head_branch.as_deref() == Some(branch));
            if has_pr {
                actions.push(SessionAction::ClearNudge(branch.clone()));
                continue;
            }

            if self.local_mode {
                // In local mode, auto-create a local PR when the session is
                // idle and the branch has commits ahead of main.
                if !crate::local::has_local_pr_for_branch(&self.repo, branch)
                    && crate::git::branch_has_commits(branch)
                {
                    actions.push(SessionAction::AutoCreateLocalPr(branch.clone()));
                }
                continue;
            }

            let nudge_count = self.nudged_sessions.entry(branch.clone()).or_insert(0);
            if *nudge_count >= max_nudges {
                continue;
            }
            *nudge_count += 1;
            actions.push(SessionAction::Nudge(branch.clone()));
        }

        for action in actions {
            match action {
                SessionAction::ClearNudge(branch) => {
                    self.nudged_sessions.remove(&branch);
                }
                SessionAction::AutoCreateLocalPr(branch) => {
                    let pr_ready = crate::config::get_pr_ready(&self.repo);
                    let title = crate::git::first_commit_summary(&branch)
                        .unwrap_or_else(|| format!("PR for {}", branch));
                    match crate::local::create_local_pr(&self.repo, &title, "", &branch, !pr_ready)
                    {
                        Ok(number) => {
                            self.set_status(format!(
                                "Auto-created local PR #{} for {}",
                                number, branch
                            ));
                            self.add_message(&format!(
                                "[monitor] Auto-created local PR #{} for {}",
                                number, branch
                            ));
                            self.pull_requests = crate::local::fetch_local_prs(
                                &self.repo,
                                self.pr_state_filter,
                                self.pr_assignee_filter,
                            );
                        }
                        Err(e) => {
                            self.add_message(&format!(
                                "[monitor] Failed to auto-create local PR for {}: {}",
                                branch, e
                            ));
                        }
                    }
                }
                SessionAction::Nudge(branch) => {
                    self.multiplexer.send_keys(&branch, "continue");
                    self.add_message(&format!(
                        "[monitor] Nudged {} to continue (no PR found)",
                        branch
                    ));
                }
            }
        }

        // Clean up nudge tracking for sessions that no longer exist
        let active_branches: HashSet<String> =
            self.sessions.iter().map(|s| s.title.clone()).collect();
        self.nudged_sessions
            .retain(|k, _| active_branches.contains(k));

        self.clamp_selected();
        self.last_refresh = Instant::now();
    }

    /// Launch per-section background threads to fetch data without blocking the UI.
    /// Each section sends its results via a shared channel as soon as it's ready.
    pub fn start_async_refresh(&mut self) {
        self.section_loading = [true; 4];
        let (tx, rx) = mpsc::channel();
        self.section_rx = Some(rx);

        let repo = self.repo.clone();
        let local_mode = self.local_mode;

        // Issues thread
        let tx_issues = tx.clone();
        let repo_i = repo.clone();
        let isf = self.issue_state_filter;
        let iaf = self.issue_assignee_filter;
        let search_q = self.issue_search_query.clone();
        std::thread::spawn(move || {
            let issues = if local_mode {
                crate::local::fetch_local_issues(&repo_i, isf, iaf)
            } else {
                crate::github::fetch_issues(&repo_i, isf, iaf, search_q.as_deref())
            };
            let _ = tx_issues.send(SectionData::Issues(issues));
        });

        // Worktrees thread
        let tx_wt = tx.clone();
        std::thread::spawn(move || {
            let worktrees = fetch_worktrees();
            let _ = tx_wt.send(SectionData::Worktrees(worktrees));
        });

        // Sessions thread
        let tx_sess = tx.clone();
        let session_states = std::sync::Arc::clone(&self.session_states);
        let multiplexer = self.multiplexer;
        std::thread::spawn(move || {
            let sessions = fetch_sessions(&session_states, multiplexer);
            let _ = tx_sess.send(SectionData::Sessions(sessions));
        });

        // Pull Requests thread
        let tx_pr = tx.clone();
        let repo_pr = repo.clone();
        let psf = self.pr_state_filter;
        let paf = self.pr_assignee_filter;
        std::thread::spawn(move || {
            let prs = if local_mode {
                crate::local::fetch_local_prs(&repo_pr, psf, paf)
            } else {
                fetch_prs(&repo_pr, psf, paf)
            };
            let _ = tx_pr.send(SectionData::PullRequests(prs));
        });

        // Main behind count thread
        std::thread::spawn(move || {
            let count = fetch_main_behind_count();
            let _ = tx.send(SectionData::MainBehindCount(count));
        });
    }

    /// Run cleanup and auto-nudge logic after all sections have loaded.
    pub fn post_refresh_cleanup(&mut self) {
        if self.local_mode {
            let merged = crate::local::fetch_local_merged_pr_branches(&self.repo);
            let cleaned = crate::git::cleanup_local_merged_worktrees(
                &merged,
                &self.worktrees,
                self.multiplexer,
            );
            if !cleaned.is_empty() {
                self.set_status(format!("Cleaned up merged: {}", cleaned.join(", ")));
                self.worktrees = fetch_worktrees();
            }
        } else {
            let cleaned = cleanup_merged_worktrees(&self.repo, &self.worktrees, self.multiplexer);
            if !cleaned.is_empty() {
                self.set_status(format!("Cleaned up merged: {}", cleaned.join(", ")));
                self.worktrees = fetch_worktrees();
            }
        }

        self.sessions = fetch_sessions(&self.session_states, self.multiplexer);

        // Auto-nudge idle sessions
        let max_nudges = 1;
        enum SessionAction {
            ClearNudge(String),
            AutoCreateLocalPr(String),
            Nudge(String),
        }
        let mut actions = Vec::new();
        for session in &self.sessions {
            if session.tag != "idle" {
                continue;
            }
            let branch = &session.title;
            let has_pr = self
                .pull_requests
                .iter()
                .any(|pr| pr.head_branch.as_deref() == Some(branch));
            if has_pr {
                actions.push(SessionAction::ClearNudge(branch.clone()));
                continue;
            }

            if self.local_mode {
                if !crate::local::has_local_pr_for_branch(&self.repo, branch)
                    && crate::git::branch_has_commits(branch)
                {
                    actions.push(SessionAction::AutoCreateLocalPr(branch.clone()));
                }
                continue;
            }

            let nudge_count = self.nudged_sessions.entry(branch.clone()).or_insert(0);
            if *nudge_count >= max_nudges {
                continue;
            }
            *nudge_count += 1;
            actions.push(SessionAction::Nudge(branch.clone()));
        }

        for action in actions {
            match action {
                SessionAction::ClearNudge(branch) => {
                    self.nudged_sessions.remove(&branch);
                }
                SessionAction::AutoCreateLocalPr(branch) => {
                    let pr_ready = crate::config::get_pr_ready(&self.repo);
                    let title = crate::git::first_commit_summary(&branch)
                        .unwrap_or_else(|| format!("PR for {}", branch));
                    match crate::local::create_local_pr(&self.repo, &title, "", &branch, !pr_ready)
                    {
                        Ok(number) => {
                            self.set_status(format!(
                                "Auto-created local PR #{} for {}",
                                number, branch
                            ));
                            self.add_message(&format!(
                                "[monitor] Auto-created local PR #{} for {}",
                                number, branch
                            ));
                            self.pull_requests = crate::local::fetch_local_prs(
                                &self.repo,
                                self.pr_state_filter,
                                self.pr_assignee_filter,
                            );
                        }
                        Err(e) => {
                            self.add_message(&format!(
                                "[monitor] Failed to auto-create local PR for {}: {}",
                                branch, e
                            ));
                        }
                    }
                }
                SessionAction::Nudge(branch) => {
                    self.multiplexer.send_keys(&branch, "continue");
                    self.add_message(&format!(
                        "[monitor] Nudged {} to continue (no PR found)",
                        branch
                    ));
                }
            }
        }

        let active_branches: HashSet<String> =
            self.sessions.iter().map(|s| s.title.clone()).collect();
        self.nudged_sessions
            .retain(|k, _| active_branches.contains(k));

        self.clamp_selected();
        self.last_refresh = Instant::now();
    }

    /// Returns true if any section is currently loading asynchronously.
    pub fn is_section_loading(&self) -> bool {
        self.section_loading.iter().any(|&x| x)
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
