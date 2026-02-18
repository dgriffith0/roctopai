use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ratatui::style::Color;

pub const SOCKET_PATH: &str = "/tmp/octopai-events.sock";
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(30);
pub const MAX_MESSAGES: usize = 100;

pub type SessionStates = Arc<Mutex<HashMap<String, String>>>;
pub type MessageLog = Arc<Mutex<VecDeque<String>>>;

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
    pub head_branch: Option<String>,
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
        branch: Option<String>,
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
    Filtering { query: TextInput },
    CreatingIssue,
    Confirming,
    EditingVerifyCommand { input: TextInput },
    EditingEditorCommand { input: TextInput },
}

#[derive(PartialEq)]
pub enum Screen {
    RepoSelect,
    Board,
    Dependencies,
    Configuration,
}

pub struct ConfigEditState {
    pub verify_command: TextInput,
    pub editor_command: TextInput,
    pub pr_ready: bool,
    pub session_command: TextInput,
    pub multiplexer: crate::session::Multiplexer,
    pub active_field: usize, // 0 = verify, 1 = editor, 2 = pr_ready, 3 = session_command, 4 = multiplexer
}

impl ConfigEditState {
    pub fn new(
        verify_command: String,
        editor_command: String,
        pr_ready: bool,
        session_command: String,
        multiplexer: crate::session::Multiplexer,
    ) -> Self {
        Self {
            verify_command: TextInput::from(verify_command),
            editor_command: TextInput::from(editor_command),
            pr_ready,
            session_command: TextInput::from(session_command),
            multiplexer,
            active_field: 0,
        }
    }
}

#[derive(PartialEq)]
pub enum RepoSelectPhase {
    Typing,
    Loading,
    Picking,
}

pub struct RepoSelectState {
    pub input: TextInput,
    pub repos: Vec<String>,
    pub filtered_repos: Vec<String>,
    pub selected: usize,
    pub phase: RepoSelectPhase,
    pub error: Option<String>,
    pub filter_query: TextInput,
}

impl RepoSelectState {
    pub fn new() -> Self {
        Self {
            input: TextInput::new(),
            repos: Vec::new(),
            filtered_repos: Vec::new(),
            selected: 0,
            phase: RepoSelectPhase::Typing,
            error: None,
            filter_query: TextInput::new(),
        }
    }

    pub fn update_filtered(&mut self) {
        if self.filter_query.is_empty() {
            self.filtered_repos = self.repos.clone();
        } else {
            self.filtered_repos = self
                .repos
                .iter()
                .filter(|r| fuzzy_match(self.filter_query.value(), r))
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
    pub title: TextInput,
    pub body: TextInput,
    pub active_field: usize, // 0 = title, 1 = body, 2 = create_worktree toggle
    pub error: Option<String>,
    pub submitting: bool,
    pub create_worktree: bool,
}

impl IssueModal {
    pub fn new() -> Self {
        Self {
            title: TextInput::new(),
            body: TextInput::new(),
            active_field: 0,
            error: None,
            submitting: false,
            create_worktree: true,
        }
    }
}

pub enum IssueSubmitResult {
    Success {
        number: u64,
        worktree_result: Option<std::result::Result<(), String>>,
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

#[derive(PartialEq)]
pub struct TextInput {
    pub text: String,
    pub cursor: usize, // character index (not byte index)
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }

    pub fn from(s: String) -> Self {
        let cursor = s.chars().count();
        Self { text: s, cursor }
    }

    fn byte_index(&self) -> usize {
        self.text
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    pub fn insert(&mut self, c: char) {
        let idx = self.byte_index();
        self.text.insert(idx, c);
        self.cursor += 1;
    }

    pub fn delete_back(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let idx = self.byte_index();
            self.text.remove(idx);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.text.chars().count() {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.chars().count();
    }

    pub fn value(&self) -> &str {
        &self.text
    }

    /// Text before the cursor position
    pub fn before_cursor(&self) -> &str {
        let idx = self.byte_index();
        &self.text[..idx]
    }

    /// Text from cursor position onward
    pub fn after_cursor(&self) -> &str {
        let idx = self.byte_index();
        &self.text[idx..]
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
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
