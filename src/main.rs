mod app;
mod config;
mod deps;
mod git;
mod github;
mod hooks;
mod models;
mod session;
mod ui;

use std::collections::HashMap;
use std::fs;
use std::io;
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use color_eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};

use app::App;
use config::{
    get_editor_command, get_pr_ready, get_session_command, get_verify_command, load_config,
    save_config, set_editor_command, set_verify_command,
};
use deps::{check_dependencies, has_missing_required};
use git::{detect_current_repo, fetch_worktrees, remove_worktree};
use github::{close_issue, create_issue, fetch_issues, fetch_prs, fetch_repos};
use hooks::start_event_socket;
use models::{
    ConfigEditState, ConfirmAction, ConfirmModal, IssueModal, IssueSubmitResult, MergeStrategy,
    MessageLog, Mode, RepoSelectPhase, Screen, SessionStates, StateFilter, TextInput,
    REFRESH_INTERVAL, SOCKET_PATH,
};
use session::{create_worktree_and_session, expand_editor_command, fetch_sessions, Multiplexer};
use ui::{ui, ui_configuration, ui_dependencies, ui_repo_select};

fn main() -> Result<()> {
    color_eyre::install()?;

    // Start the Unix socket event server for Claude hook events
    let session_states: SessionStates = Arc::new(Mutex::new(HashMap::new()));
    let message_log: MessageLog = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    start_event_socket(Arc::clone(&session_states), Arc::clone(&message_log))?;

    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;

    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(io::stdout()))?;
    // Detect available multiplexer (tmux preferred, screen as fallback)
    let multiplexer = Multiplexer::detect().unwrap_or(Multiplexer::Tmux);
    let mut app = App::new(session_states, message_log, multiplexer);

    // Check external dependencies on startup
    let initial_deps = check_dependencies();
    if has_missing_required(&initial_deps) {
        app.dependencies = initial_deps;
        app.screen = Screen::Dependencies;
    } else {
        app.dependencies = initial_deps;
        // Load saved config, falling back to detecting the current git repo
        let configured_repo = load_config().map(|c| c.repo).filter(|r| !r.is_empty());

        let repo = configured_repo.or_else(detect_current_repo);

        if let Some(repo) = repo {
            app.repo = repo.clone();
            let _ = save_config(&repo);
            app.refresh_data();
            app.selected_card = [0; 4];
            app.screen = Screen::Board;
        }
    }

    loop {
        terminal.draw(|frame| match app.screen {
            Screen::RepoSelect => ui_repo_select(frame, &app.repo_select),
            Screen::Board => ui(frame, &app),
            Screen::Dependencies => ui_dependencies(frame, &app.dependencies),
            Screen::Configuration => ui_configuration(frame, &app),
        })?;

        // Auto-refresh when interval has elapsed and on Board screen in Normal mode
        if app.screen == Screen::Board
            && app.mode == Mode::Normal
            && app.last_refresh.elapsed() >= REFRESH_INTERVAL
        {
            app.refresh_data();
        }

        // Delayed refresh after PR merge (gives GitHub API time to propagate)
        if app.screen == Screen::Board
            && app.mode == Mode::Normal
            && app
                .pending_refresh
                .is_some_and(|t| t <= std::time::Instant::now())
        {
            app.pending_refresh = None;
            app.refresh_data();
        }

        // Check for issue submission results from background thread
        if let Some(rx) = &app.issue_submit_rx {
            if let Ok(result) = rx.try_recv() {
                app.issue_submit_rx = None;
                match result {
                    IssueSubmitResult::Success {
                        number,
                        worktree_result,
                        ..
                    } => {
                        app.issues = fetch_issues(
                            &app.repo,
                            app.issue_state_filter,
                            app.issue_assignee_filter,
                        );
                        app.clamp_selected();
                        app.last_refresh = std::time::Instant::now();
                        app.issue_modal = None;
                        app.mode = Mode::Normal;
                        match worktree_result {
                            Ok(()) => {
                                app.worktrees = fetch_worktrees();
                                app.sessions = fetch_sessions(&app.session_states, app.multiplexer);
                                app.clamp_selected();
                                app.set_status(format!(
                                    "Created issue #{} with worktree and session",
                                    number
                                ));
                            }
                            Err(e) => {
                                app.set_status(format!(
                                    "Created issue #{} but failed to create worktree: {}",
                                    number, e
                                ));
                            }
                        }
                    }
                    IssueSubmitResult::Error(e) => {
                        if let Some(modal) = &mut app.issue_modal {
                            modal.submitting = false;
                            modal.error = Some(e);
                        }
                    }
                }
            }
        }

        // Advance spinner tick when submitting
        if app.issue_submit_rx.is_some() {
            app.spinner_tick = app.spinner_tick.wrapping_add(1);
        }

        // Poll for events with a short timeout so the refresh timer updates every second
        let poll_timeout = if app.issue_submit_rx.is_some() {
            // Fast polling for spinner animation
            Duration::from_millis(100)
        } else if app.screen == Screen::Board && app.mode == Mode::Normal {
            let remaining = REFRESH_INTERVAL
                .checked_sub(app.last_refresh.elapsed())
                .unwrap_or(Duration::ZERO);
            // Cap at 1 second so the countdown timer display stays current
            remaining.min(Duration::from_secs(1))
        } else {
            Duration::from_secs(60)
        };

        if !event::poll(poll_timeout)? {
            continue;
        }

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match app.screen {
                Screen::Dependencies => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        if app.repo.is_empty() {
                            break;
                        } else {
                            app.screen = Screen::Board;
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        app.dependencies = check_dependencies();
                    }
                    KeyCode::Enter => {
                        if has_missing_required(&app.dependencies) {
                            // Stay on deps screen
                        } else if app.repo.is_empty() {
                            app.screen = Screen::RepoSelect;
                        } else {
                            app.screen = Screen::Board;
                        }
                    }
                    _ => {}
                },
                Screen::Configuration => {
                    if let Some(config_edit) = &mut app.config_edit {
                        match key.code {
                            KeyCode::Esc => {
                                app.config_edit = None;
                                app.screen = Screen::Board;
                            }
                            KeyCode::Tab => {
                                config_edit.active_field = (config_edit.active_field + 1) % 4;
                            }
                            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                let verify_cmd =
                                    config_edit.verify_command.value().trim().to_string();
                                let editor_cmd =
                                    config_edit.editor_command.value().trim().to_string();
                                let claude_cmd =
                                    config_edit.session_command.value().trim().to_string();
                                let repo = app.repo.clone();

                                let pr_ready = config_edit.pr_ready;

                                if let Some(mut config) = load_config() {
                                    if verify_cmd.is_empty() {
                                        config.verify_commands.remove(&repo);
                                    } else {
                                        config
                                            .verify_commands
                                            .insert(repo.clone(), verify_cmd.clone());
                                    }
                                    if editor_cmd.is_empty() {
                                        config.editor_commands.remove(&repo);
                                    } else {
                                        config
                                            .editor_commands
                                            .insert(repo.clone(), editor_cmd.clone());
                                    }
                                    if pr_ready {
                                        config.pr_ready.insert(repo.clone(), true);
                                    } else {
                                        config.pr_ready.remove(&repo);
                                    }
                                    if claude_cmd.is_empty() {
                                        config.session_commands.remove(&repo);
                                    } else {
                                        config
                                            .session_commands
                                            .insert(repo.clone(), claude_cmd.clone());
                                    }
                                    let _ = config::save_full_config(&config);
                                }

                                app.set_status("Configuration saved".to_string());
                                app.config_edit = None;
                                app.screen = Screen::Board;
                            }
                            KeyCode::Backspace => match config_edit.active_field {
                                0 => config_edit.verify_command.delete_back(),
                                1 => config_edit.editor_command.delete_back(),
                                3 => config_edit.session_command.delete_back(),
                                _ => {}
                            },
                            KeyCode::Left => match config_edit.active_field {
                                0 => config_edit.verify_command.move_left(),
                                1 => config_edit.editor_command.move_left(),
                                3 => config_edit.session_command.move_left(),
                                _ => {}
                            },
                            KeyCode::Right => match config_edit.active_field {
                                0 => config_edit.verify_command.move_right(),
                                1 => config_edit.editor_command.move_right(),
                                3 => config_edit.session_command.move_right(),
                                _ => {}
                            },
                            KeyCode::Home => match config_edit.active_field {
                                0 => config_edit.verify_command.move_home(),
                                1 => config_edit.editor_command.move_home(),
                                3 => config_edit.session_command.move_home(),
                                _ => {}
                            },
                            KeyCode::End => match config_edit.active_field {
                                0 => config_edit.verify_command.move_end(),
                                1 => config_edit.editor_command.move_end(),
                                3 => config_edit.session_command.move_end(),
                                _ => {}
                            },
                            KeyCode::Char(' ') if config_edit.active_field == 2 => {
                                config_edit.pr_ready = !config_edit.pr_ready;
                            }
                            KeyCode::Enter if config_edit.active_field == 2 => {
                                config_edit.pr_ready = !config_edit.pr_ready;
                            }
                            KeyCode::Char(c) => match config_edit.active_field {
                                0 => config_edit.verify_command.insert(c),
                                1 => config_edit.editor_command.insert(c),
                                3 => config_edit.session_command.insert(c),
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                }
                Screen::RepoSelect => {
                    match app.repo_select.phase {
                        RepoSelectPhase::Typing => match key.code {
                            KeyCode::Esc => {
                                if app.repo.is_empty() {
                                    break; // quit if no board to return to
                                } else {
                                    app.screen = Screen::Board;
                                }
                            }
                            KeyCode::Enter => {
                                let owner = app.repo_select.input.value().trim().to_string();
                                if owner.is_empty() {
                                    app.repo_select.error =
                                        Some("Please enter an org or user name".into());
                                } else {
                                    app.repo_select.error = None;
                                    app.repo_select.phase = RepoSelectPhase::Loading;
                                    // We need to redraw to show loading state, then fetch
                                    terminal
                                        .draw(|frame| ui_repo_select(frame, &app.repo_select))?;

                                    match fetch_repos(&owner) {
                                        Ok(repos) => {
                                            app.repo_select.repos = repos;
                                            app.repo_select.filter_query.clear();
                                            app.repo_select.update_filtered();
                                            app.repo_select.selected = 0;
                                            app.repo_select.phase = RepoSelectPhase::Picking;
                                        }
                                        Err(e) => {
                                            app.repo_select.error = Some(e);
                                            app.repo_select.phase = RepoSelectPhase::Typing;
                                        }
                                    }
                                }
                            }
                            KeyCode::Backspace => {
                                app.repo_select.input.delete_back();
                            }
                            KeyCode::Left => {
                                app.repo_select.input.move_left();
                            }
                            KeyCode::Right => {
                                app.repo_select.input.move_right();
                            }
                            KeyCode::Home => {
                                app.repo_select.input.move_home();
                            }
                            KeyCode::End => {
                                app.repo_select.input.move_end();
                            }
                            KeyCode::Char(c) => {
                                app.repo_select.input.insert(c);
                            }
                            _ => {}
                        },
                        RepoSelectPhase::Loading => {
                            // No input during loading
                        }
                        RepoSelectPhase::Picking => match key.code {
                            KeyCode::Esc => {
                                app.repo_select.phase = RepoSelectPhase::Typing;
                                app.repo_select.filter_query.clear();
                            }
                            KeyCode::Enter => {
                                if let Some(repo) =
                                    app.repo_select.filtered_repos.get(app.repo_select.selected)
                                {
                                    let repo = repo.clone();
                                    let _ = save_config(&repo);
                                    app.repo = repo;
                                    app.refresh_data();
                                    app.selected_card = [0; 4];
                                    app.screen = Screen::Board;
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if app.repo_select.selected > 0 {
                                    app.repo_select.selected -= 1;
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if !app.repo_select.filtered_repos.is_empty()
                                    && app.repo_select.selected
                                        < app.repo_select.filtered_repos.len() - 1
                                {
                                    app.repo_select.selected += 1;
                                }
                            }
                            KeyCode::Char('/') => {
                                // Toggle filter — if already filtering, this adds '/' to query
                                // Start fresh filter
                                app.repo_select.filter_query.clear();
                                app.repo_select.update_filtered();
                            }
                            KeyCode::Backspace => {
                                app.repo_select.filter_query.delete_back();
                                app.repo_select.update_filtered();
                            }
                            KeyCode::Left => {
                                app.repo_select.filter_query.move_left();
                            }
                            KeyCode::Right => {
                                app.repo_select.filter_query.move_right();
                            }
                            KeyCode::Char(c) => {
                                if c != '/' {
                                    app.repo_select.filter_query.insert(c);
                                    app.repo_select.update_filtered();
                                }
                            }
                            _ => {}
                        },
                    }
                }
                Screen::Board => {
                    match &mut app.mode {
                        Mode::Filtering { query } => match key.code {
                            KeyCode::Esc => {
                                app.mode = Mode::Normal;
                            }
                            KeyCode::Backspace => {
                                query.delete_back();
                                app.clamp_selected();
                            }
                            KeyCode::Left => {
                                query.move_left();
                            }
                            KeyCode::Right => {
                                query.move_right();
                            }
                            KeyCode::Home => {
                                query.move_home();
                            }
                            KeyCode::End => {
                                query.move_end();
                            }
                            KeyCode::Up => {
                                app.move_card_up();
                            }
                            KeyCode::Down => {
                                app.move_card_down();
                            }
                            KeyCode::Char(c) => {
                                query.insert(c);
                                app.clamp_selected();
                            }
                            _ => {}
                        },
                        Mode::Normal => {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => break,
                                KeyCode::Enter => {
                                    app.enter_repo_select();
                                }
                                KeyCode::Tab | KeyCode::Char('l') => {
                                    app.active_section = (app.active_section + 1) % 4;
                                }
                                KeyCode::BackTab | KeyCode::Char('h') => {
                                    app.active_section = (app.active_section + 3) % 4;
                                }
                                KeyCode::Char('R') => {
                                    app.refresh_data();
                                    app.set_status("Refreshed".to_string());
                                }
                                KeyCode::Char('D') => {
                                    app.dependencies = check_dependencies();
                                    app.screen = Screen::Dependencies;
                                }
                                KeyCode::Char('/') => {
                                    app.mode = Mode::Filtering {
                                        query: TextInput::new(),
                                    };
                                }
                                KeyCode::Char('n') => {
                                    app.mode = Mode::CreatingIssue;
                                    app.issue_modal = Some(IssueModal::new());
                                }
                                KeyCode::Char('w') if app.active_section == 0 => {
                                    if let Some(card) = app.issues.get(app.selected_card[0]) {
                                        // Extract issue number from id "issue-N"
                                        if let Some(num_str) = card.id.strip_prefix("issue-") {
                                            if let Ok(number) = num_str.parse::<u64>() {
                                                let title = card.title.clone();
                                                let body = card
                                                    .full_description
                                                    .clone()
                                                    .unwrap_or_default();
                                                let repo = app.repo.clone();
                                                let pr_ready = get_pr_ready(&repo);
                                                let claude_cmd = get_session_command(&repo);
                                                match create_worktree_and_session(
                                                    &repo,
                                                    number,
                                                    &title,
                                                    &body,
                                                    app.hook_script_path.as_deref(),
                                                    pr_ready,
                                                    claude_cmd.as_deref(),
                                                    app.multiplexer,
                                                ) {
                                                    Ok(()) => {
                                                        app.worktrees = fetch_worktrees();
                                                        app.sessions = fetch_sessions(
                                                            &app.session_states,
                                                            app.multiplexer,
                                                        );
                                                        app.clamp_selected();
                                                        app.last_refresh =
                                                            std::time::Instant::now();
                                                        app.set_status(format!(
                                                            "Created worktree and session for issue #{}",
                                                            number
                                                        ));
                                                    }
                                                    Err(e) => {
                                                        app.set_status(format!("Error: {}", e));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('d')
                                    if app.active_section == 0
                                        && app.issue_state_filter == StateFilter::Open =>
                                {
                                    if let Some(card) = app.issues.get(app.selected_card[0]) {
                                        if let Some(num_str) = card.id.strip_prefix("issue-") {
                                            if let Ok(number) = num_str.parse::<u64>() {
                                                app.confirm_modal = Some(ConfirmModal {
                                                    message: format!(
                                                        "Close issue #{}?\n\n{}",
                                                        number, card.title
                                                    ),
                                                    on_confirm: ConfirmAction::CloseIssue {
                                                        number,
                                                    },
                                                });
                                                app.mode = Mode::Confirming;
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('d') if app.active_section == 1 => {
                                    if let Some(card) = app.worktrees.get(app.selected_card[1]) {
                                        let branch = card.title.clone();
                                        if branch == "main" || branch == "master" {
                                            app.set_status(
                                                "Cannot remove main/master worktree".to_string(),
                                            );
                                        } else {
                                            let path = card.description.clone();
                                            app.confirm_modal = Some(ConfirmModal {
                                                message: format!(
                                                    "Remove worktree '{}'?\n\nPath: {}\nThis will also delete the branch and kill any active session.",
                                                    branch, path
                                                ),
                                                on_confirm: ConfirmAction::RemoveWorktree {
                                                    path,
                                                    branch,
                                                },
                                            });
                                            app.mode = Mode::Confirming;
                                        }
                                    }
                                }
                                KeyCode::Char('v') if app.active_section == 1 => {
                                    if let Some(card) = app.worktrees.get(app.selected_card[1]) {
                                        let worktree_path = card.description.clone();
                                        if let Some(cmd) = get_verify_command(&app.repo) {
                                            let expanded =
                                                expand_editor_command(&cmd, &worktree_path);
                                            let result =
                                                Command::new("sh").args(["-c", &expanded]).spawn();
                                            match result {
                                                Ok(_) => {
                                                    app.set_status(format!(
                                                        "Launched verify for '{}'",
                                                        card.title
                                                    ));
                                                }
                                                Err(e) => {
                                                    app.set_status(format!(
                                                        "Failed to launch verify command: {}",
                                                        e
                                                    ));
                                                }
                                            }
                                        } else {
                                            // No verify command configured — prompt user
                                            app.mode = Mode::EditingVerifyCommand {
                                                input: TextInput::new(),
                                            };
                                        }
                                    }
                                }
                                KeyCode::Char('e') if app.active_section == 1 => {
                                    if let Some(card) = app.worktrees.get(app.selected_card[1]) {
                                        let worktree_path = card.description.clone();
                                        if let Some(cmd) = get_editor_command(&app.repo) {
                                            let expanded =
                                                expand_editor_command(&cmd, &worktree_path);
                                            let result =
                                                Command::new("sh").args(["-c", &expanded]).spawn();
                                            match result {
                                                Ok(_) => {
                                                    app.set_status(format!(
                                                        "Opened editor for '{}'",
                                                        card.title
                                                    ));
                                                }
                                                Err(e) => {
                                                    app.set_status(format!(
                                                        "Failed to launch editor: {}",
                                                        e
                                                    ));
                                                }
                                            }
                                        } else {
                                            app.mode = Mode::EditingEditorCommand {
                                                input: TextInput::new(),
                                            };
                                        }
                                    }
                                }
                                KeyCode::Char('C') => {
                                    let current_verify =
                                        get_verify_command(&app.repo).unwrap_or_default();
                                    let current_editor =
                                        get_editor_command(&app.repo).unwrap_or_default();
                                    let current_pr_ready = get_pr_ready(&app.repo);
                                    let current_claude =
                                        get_session_command(&app.repo).unwrap_or_default();
                                    app.config_edit = Some(ConfigEditState::new(
                                        current_verify,
                                        current_editor,
                                        current_pr_ready,
                                        current_claude,
                                    ));
                                    app.screen = Screen::Configuration;
                                }
                                // PR actions: 'o' to open in browser, 'r' to mark ready
                                KeyCode::Char('o') if app.active_section == 3 => {
                                    if let Some(card) = app.pull_requests.get(app.selected_card[3])
                                    {
                                        if let Some(url) = &card.url {
                                            let _ = Command::new("open").arg(url).output();
                                        }
                                    }
                                }
                                KeyCode::Char('r') if app.active_section == 3 => {
                                    if let Some(card) = app.pull_requests.get(app.selected_card[3])
                                    {
                                        if card.is_draft == Some(true) {
                                            if let Some(number) = card.pr_number {
                                                let repo = app.repo.clone();
                                                let output = Command::new("gh")
                                                    .args([
                                                        "pr",
                                                        "ready",
                                                        "--repo",
                                                        &repo,
                                                        &number.to_string(),
                                                    ])
                                                    .output();
                                                match output {
                                                    Ok(o) if o.status.success() => {
                                                        app.pull_requests = fetch_prs(
                                                            &repo,
                                                            app.pr_state_filter,
                                                            app.pr_assignee_filter,
                                                        );
                                                        app.clamp_selected();
                                                        app.last_refresh =
                                                            std::time::Instant::now();
                                                        app.set_status(format!(
                                                            "PR #{} marked as ready",
                                                            number
                                                        ));
                                                    }
                                                    Ok(o) => {
                                                        let stderr =
                                                            String::from_utf8_lossy(&o.stderr);
                                                        app.set_status(format!(
                                                            "Error: {}",
                                                            stderr.trim()
                                                        ));
                                                    }
                                                    Err(e) => {
                                                        app.set_status(format!("Error: {}", e));
                                                    }
                                                }
                                            }
                                        } else {
                                            app.set_status("PR is already ready".to_string());
                                        }
                                    }
                                }
                                KeyCode::Char('V') if app.active_section == 3 => {
                                    if let Some(card) = app.pull_requests.get(app.selected_card[3])
                                    {
                                        if let Some(number) = card.pr_number {
                                            if card.is_merged != Some(true) {
                                                app.set_status(
                                                    "Can only revert merged PRs".to_string(),
                                                );
                                            } else {
                                                app.confirm_modal = Some(ConfirmModal {
                                                    message: format!(
                                                        "Revert PR #{}? This will create a new PR that undoes its changes.",
                                                        number
                                                    ),
                                                    on_confirm: ConfirmAction::RevertPr { number },
                                                });
                                                app.mode = Mode::Confirming;
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('M') if app.active_section == 3 => {
                                    if let Some(card) = app.pull_requests.get(app.selected_card[3])
                                    {
                                        if let Some(number) = card.pr_number {
                                            if card.is_draft == Some(true) {
                                                app.set_status(
                                                    "Cannot merge a draft PR".to_string(),
                                                );
                                            } else {
                                                let branch = card.head_branch.clone();
                                                app.confirm_modal = Some(ConfirmModal {
                                                    message: format!(
                                                        "Merge PR #{} with merge strategy?",
                                                        number
                                                    ),
                                                    on_confirm: ConfirmAction::MergePr {
                                                        number,
                                                        strategy: MergeStrategy::Merge,
                                                        branch,
                                                    },
                                                });
                                                app.mode = Mode::Confirming;
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('d') if app.active_section == 2 => {
                                    if let Some(card) = app.sessions.get(app.selected_card[2]) {
                                        let session_name = card.title.clone();
                                        app.confirm_modal = Some(ConfirmModal {
                                            message: format!(
                                                "Kill {} session '{}'?",
                                                app.multiplexer.label(),
                                                session_name
                                            ),
                                            on_confirm: ConfirmAction::KillSession {
                                                name: session_name,
                                            },
                                        });
                                        app.mode = Mode::Confirming;
                                    }
                                }
                                KeyCode::Char('a') if app.active_section == 2 => {
                                    if let Some(card) = app.sessions.get(app.selected_card[2]) {
                                        let session_name = card.title.clone();
                                        // Suspend TUI, attach to tmux, resume on detach
                                        disable_raw_mode()?;
                                        io::stdout().execute(LeaveAlternateScreen)?;
                                        let _ = app.multiplexer.attach(&session_name);
                                        enable_raw_mode()?;
                                        io::stdout().execute(EnterAlternateScreen)?;
                                        terminal.clear()?;
                                        // Refresh all state after returning (Claude may have created PRs)
                                        app.refresh_data();
                                    }
                                }
                                // Filter toggles for Issues and Pull Requests
                                KeyCode::Char('s')
                                    if app.active_section == 0 || app.active_section == 3 =>
                                {
                                    if app.active_section == 0 {
                                        app.issue_state_filter = app.issue_state_filter.toggle();
                                        app.issues = fetch_issues(
                                            &app.repo,
                                            app.issue_state_filter,
                                            app.issue_assignee_filter,
                                        );
                                    } else {
                                        app.pr_state_filter = app.pr_state_filter.toggle();
                                        app.pull_requests = fetch_prs(
                                            &app.repo,
                                            app.pr_state_filter,
                                            app.pr_assignee_filter,
                                        );
                                    }
                                    app.clamp_selected();
                                    app.last_refresh = std::time::Instant::now();
                                }
                                KeyCode::Char('m')
                                    if app.active_section == 0 || app.active_section == 3 =>
                                {
                                    if app.active_section == 0 {
                                        app.issue_assignee_filter =
                                            app.issue_assignee_filter.toggle();
                                        app.issues = fetch_issues(
                                            &app.repo,
                                            app.issue_state_filter,
                                            app.issue_assignee_filter,
                                        );
                                    } else {
                                        app.pr_assignee_filter = app.pr_assignee_filter.toggle();
                                        app.pull_requests = fetch_prs(
                                            &app.repo,
                                            app.pr_state_filter,
                                            app.pr_assignee_filter,
                                        );
                                    }
                                    app.clamp_selected();
                                    app.last_refresh = std::time::Instant::now();
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    app.move_card_up();
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    app.move_card_down();
                                }
                                KeyCode::Char('x') => {
                                    app.show_messages = !app.show_messages;
                                    if !app.show_messages {
                                        app.messages_expanded = false;
                                    }
                                }
                                KeyCode::Char('X') if app.show_messages => {
                                    app.messages_expanded = !app.messages_expanded;
                                }
                                _ => {}
                            }
                        }
                        Mode::Confirming => match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                if let Some(modal) = app.confirm_modal.take() {
                                    match modal.on_confirm {
                                        ConfirmAction::CloseIssue { number } => {
                                            let repo = app.repo.clone();
                                            match close_issue(&repo, number) {
                                                Ok(()) => {
                                                    app.issues = fetch_issues(
                                                        &repo,
                                                        app.issue_state_filter,
                                                        app.issue_assignee_filter,
                                                    );
                                                    app.clamp_selected();
                                                    app.last_refresh = std::time::Instant::now();
                                                    app.set_status(format!(
                                                        "Closed issue #{}",
                                                        number
                                                    ));
                                                }
                                                Err(e) => {
                                                    app.set_status(format!("Error: {}", e));
                                                }
                                            }
                                        }
                                        ConfirmAction::RemoveWorktree { path, branch } => {
                                            match remove_worktree(&path, &branch, app.multiplexer) {
                                                Ok(()) => {
                                                    app.worktrees = fetch_worktrees();
                                                    app.sessions = fetch_sessions(
                                                        &app.session_states,
                                                        app.multiplexer,
                                                    );
                                                    app.clamp_selected();
                                                    app.last_refresh = std::time::Instant::now();
                                                    app.set_status(format!(
                                                        "Removed worktree '{}'",
                                                        branch
                                                    ));
                                                }
                                                Err(e) => {
                                                    app.set_status(format!("Error: {}", e));
                                                }
                                            }
                                        }
                                        ConfirmAction::KillSession { name } => {
                                            let output = app.multiplexer.kill_session(&name);
                                            if output.status.success() {
                                                app.sessions = fetch_sessions(
                                                    &app.session_states,
                                                    app.multiplexer,
                                                );
                                                app.clamp_selected();
                                                app.last_refresh = std::time::Instant::now();
                                                app.set_status(format!(
                                                    "Killed session '{}'",
                                                    name
                                                ));
                                            } else {
                                                let stderr =
                                                    String::from_utf8_lossy(&output.stderr);
                                                app.set_status(format!("Error: {}", stderr.trim()));
                                            }
                                        }
                                        ConfirmAction::RevertPr { number } => {
                                            let repo = app.repo.clone();
                                            // Get the PR's GraphQL node ID
                                            let id_output = Command::new("gh")
                                                .args([
                                                    "pr",
                                                    "view",
                                                    &number.to_string(),
                                                    "--repo",
                                                    &repo,
                                                    "--json",
                                                    "id",
                                                    "--jq",
                                                    ".id",
                                                ])
                                                .output();
                                            match id_output {
                                                Ok(o) if o.status.success() => {
                                                    let node_id =
                                                        String::from_utf8_lossy(&o.stdout)
                                                            .trim()
                                                            .to_string();
                                                    let query = format!(
                                                        r#"mutation {{ revertPullRequest(input: {{pullRequestId: "{}"}}) {{ revertPullRequest {{ number url }} }} }}"#,
                                                        node_id
                                                    );
                                                    let revert_output = Command::new("gh")
                                                        .args([
                                                            "api",
                                                            "graphql",
                                                            "-f",
                                                            &format!("query={}", query),
                                                        ])
                                                        .output();
                                                    match revert_output {
                                                        Ok(o) if o.status.success() => {
                                                            app.pull_requests = fetch_prs(
                                                                &repo,
                                                                app.pr_state_filter,
                                                                app.pr_assignee_filter,
                                                            );
                                                            app.clamp_selected();
                                                            app.last_refresh =
                                                                std::time::Instant::now();
                                                            app.set_status(format!(
                                                                "Created revert PR for #{}",
                                                                number
                                                            ));
                                                        }
                                                        Ok(o) => {
                                                            let stderr =
                                                                String::from_utf8_lossy(&o.stderr);
                                                            app.set_status(format!(
                                                                "Error: {}",
                                                                stderr.trim()
                                                            ));
                                                        }
                                                        Err(e) => {
                                                            app.set_status(format!("Error: {}", e));
                                                        }
                                                    }
                                                }
                                                Ok(o) => {
                                                    let stderr = String::from_utf8_lossy(&o.stderr);
                                                    app.set_status(format!(
                                                        "Error: {}",
                                                        stderr.trim()
                                                    ));
                                                }
                                                Err(e) => {
                                                    app.set_status(format!("Error: {}", e));
                                                }
                                            }
                                        }
                                        ConfirmAction::MergePr {
                                            number,
                                            strategy,
                                            branch,
                                        } => {
                                            let repo = app.repo.clone();
                                            let output = Command::new("gh")
                                                .args([
                                                    "pr",
                                                    "merge",
                                                    &number.to_string(),
                                                    strategy.flag(),
                                                    "--delete-branch",
                                                    "--repo",
                                                    &repo,
                                                ])
                                                .output();
                                            match output {
                                                Ok(o) if o.status.success() => {
                                                    // Immediately clean up the worktree for the
                                                    // merged branch instead of waiting for the
                                                    // next refresh cycle and GitHub API to reflect
                                                    // the merged state.
                                                    if let Some(ref branch_name) = branch {
                                                        if let Some(wt) = app
                                                            .worktrees
                                                            .iter()
                                                            .find(|w| w.title == *branch_name)
                                                        {
                                                            let wt_path = wt.description.clone();
                                                            let wt_branch = wt.title.clone();
                                                            if remove_worktree(
                                                                &wt_path,
                                                                &wt_branch,
                                                                app.multiplexer,
                                                            )
                                                            .is_ok()
                                                            {
                                                                app.set_status(format!(
                                                                    "Merged PR #{} ({}) — cleaned up worktree '{}'",
                                                                    number,
                                                                    strategy.label(),
                                                                    wt_branch
                                                                ));
                                                            }
                                                        }
                                                    }
                                                    app.pull_requests = fetch_prs(
                                                        &repo,
                                                        app.pr_state_filter,
                                                        app.pr_assignee_filter,
                                                    );
                                                    app.worktrees = fetch_worktrees();
                                                    app.sessions = fetch_sessions(
                                                        &app.session_states,
                                                        app.multiplexer,
                                                    );
                                                    app.clamp_selected();
                                                    app.last_refresh = std::time::Instant::now();
                                                    // Schedule a delayed refresh so GitHub
                                                    // API changes (e.g. linked issues closing)
                                                    // are picked up.
                                                    app.pending_refresh = Some(
                                                        std::time::Instant::now()
                                                            + std::time::Duration::from_secs(3),
                                                    );
                                                    if branch.is_none()
                                                        || app.worktrees.iter().any(|w| {
                                                            branch.as_deref() == Some(&w.title)
                                                        })
                                                    {
                                                        app.set_status(format!(
                                                            "Merged PR #{} ({})",
                                                            number,
                                                            strategy.label()
                                                        ));
                                                    }
                                                }
                                                Ok(o) => {
                                                    let stderr = String::from_utf8_lossy(&o.stderr);
                                                    app.set_status(format!(
                                                        "Error: {}",
                                                        stderr.trim()
                                                    ));
                                                }
                                                Err(e) => {
                                                    app.set_status(format!("Error: {}", e));
                                                }
                                            }
                                        }
                                    }
                                }
                                app.mode = Mode::Normal;
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                app.confirm_modal = None;
                                app.mode = Mode::Normal;
                            }
                            _ => {}
                        },
                        Mode::CreatingIssue => {
                            if let Some(modal) = &mut app.issue_modal {
                                // Block input while submitting (only allow Esc)
                                if modal.submitting && key.code != KeyCode::Esc {
                                    continue;
                                }
                                match key.code {
                                    KeyCode::Esc => {
                                        app.issue_submit_rx = None;
                                        app.issue_modal = None;
                                        app.mode = Mode::Normal;
                                    }
                                    KeyCode::Tab => {
                                        modal.active_field =
                                            if modal.active_field == 0 { 1 } else { 0 };
                                    }
                                    KeyCode::Enter if modal.active_field == 0 => {
                                        modal.active_field = 1;
                                    }
                                    KeyCode::Char('s')
                                        if key.modifiers.contains(KeyModifiers::CONTROL)
                                            && !modal.submitting =>
                                    {
                                        let title = modal.title.value().trim().to_string();
                                        if title.is_empty() {
                                            modal.error = Some("Title cannot be empty".to_string());
                                        } else {
                                            modal.submitting = true;
                                            modal.error = None;
                                            let body = modal.body.value().to_string();
                                            let repo = app.repo.clone();
                                            let hook_script = app.hook_script_path.clone();
                                            let claude_cmd = get_session_command(&repo);
                                            let mux = app.multiplexer;
                                            let (tx, rx) = mpsc::channel();
                                            app.issue_submit_rx = Some(rx);
                                            std::thread::spawn(move || {
                                                match create_issue(&repo, &title, &body) {
                                                    Ok(number) => {
                                                        let pr_ready = get_pr_ready(&repo);
                                                        let worktree_result =
                                                            create_worktree_and_session(
                                                                &repo,
                                                                number,
                                                                &title,
                                                                &body,
                                                                hook_script.as_deref(),
                                                                pr_ready,
                                                                claude_cmd.as_deref(),
                                                                mux,
                                                            );
                                                        let _ =
                                                            tx.send(IssueSubmitResult::Success {
                                                                number,
                                                                worktree_result,
                                                            });
                                                    }
                                                    Err(e) => {
                                                        let _ =
                                                            tx.send(IssueSubmitResult::Error(e));
                                                    }
                                                }
                                            });
                                        }
                                    }
                                    KeyCode::Backspace => {
                                        if modal.active_field == 0 {
                                            modal.title.delete_back();
                                        } else {
                                            modal.body.delete_back();
                                        }
                                    }
                                    KeyCode::Left => {
                                        if modal.active_field == 0 {
                                            modal.title.move_left();
                                        } else {
                                            modal.body.move_left();
                                        }
                                    }
                                    KeyCode::Right => {
                                        if modal.active_field == 0 {
                                            modal.title.move_right();
                                        } else {
                                            modal.body.move_right();
                                        }
                                    }
                                    KeyCode::Home => {
                                        if modal.active_field == 0 {
                                            modal.title.move_home();
                                        } else {
                                            modal.body.move_home();
                                        }
                                    }
                                    KeyCode::End => {
                                        if modal.active_field == 0 {
                                            modal.title.move_end();
                                        } else {
                                            modal.body.move_end();
                                        }
                                    }
                                    KeyCode::Char(c) => {
                                        if modal.active_field == 0 {
                                            modal.title.insert(c);
                                        } else {
                                            modal.body.insert(c);
                                        }
                                    }
                                    KeyCode::Enter if modal.active_field == 1 => {
                                        modal.body.insert('\n');
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Mode::EditingVerifyCommand { input } => match key.code {
                            KeyCode::Esc => {
                                app.mode = Mode::Normal;
                            }
                            KeyCode::Enter => {
                                let cmd = input.value().trim().to_string();
                                if !cmd.is_empty() {
                                    let repo = app.repo.clone();
                                    let _ = set_verify_command(&repo, &cmd);
                                    app.set_status(format!("Saved verify command: {}", cmd));

                                    // Now execute the verify command
                                    if let Some(card) = app.worktrees.get(app.selected_card[1]) {
                                        let worktree_path = card.description.clone();
                                        let expanded = expand_editor_command(&cmd, &worktree_path);
                                        let _ = Command::new("sh").args(["-c", &expanded]).spawn();
                                    }
                                }
                                app.mode = Mode::Normal;
                            }
                            KeyCode::Backspace => {
                                input.delete_back();
                            }
                            KeyCode::Left => {
                                input.move_left();
                            }
                            KeyCode::Right => {
                                input.move_right();
                            }
                            KeyCode::Home => {
                                input.move_home();
                            }
                            KeyCode::End => {
                                input.move_end();
                            }
                            KeyCode::Char(c) => {
                                input.insert(c);
                            }
                            _ => {}
                        },
                        Mode::EditingEditorCommand { input } => match key.code {
                            KeyCode::Esc => {
                                app.mode = Mode::Normal;
                            }
                            KeyCode::Enter => {
                                let cmd = input.value().trim().to_string();
                                if !cmd.is_empty() {
                                    let repo = app.repo.clone();
                                    let _ = set_editor_command(&repo, &cmd);
                                    app.set_status(format!("Saved editor command: {}", cmd));

                                    // Now launch the editor
                                    if let Some(card) = app.worktrees.get(app.selected_card[1]) {
                                        let worktree_path = card.description.clone();
                                        let expanded = expand_editor_command(&cmd, &worktree_path);
                                        let _ = Command::new("sh").args(["-c", &expanded]).spawn();
                                    }
                                }
                                app.mode = Mode::Normal;
                            }
                            KeyCode::Backspace => {
                                input.delete_back();
                            }
                            KeyCode::Left => {
                                input.move_left();
                            }
                            KeyCode::Right => {
                                input.move_right();
                            }
                            KeyCode::Home => {
                                input.move_home();
                            }
                            KeyCode::End => {
                                input.move_end();
                            }
                            KeyCode::Char(c) => {
                                input.insert(c);
                            }
                            _ => {}
                        },
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    let _ = fs::remove_file(SOCKET_PATH);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    fn tmux_available() -> bool {
        Command::new("tmux").arg("-V").output().is_ok()
    }

    #[test]
    fn test_prompt_is_clean_single_line() {
        let body = "This has\n\nmultiple\n\n  lines  \n and   spaces\n";
        let clean: String = body
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(clean, "This has multiple lines and   spaces");
        assert!(!clean.contains('\n'));
    }

    #[test]
    fn test_prompt_empty_body() {
        let body = "";
        let clean = if body.is_empty() {
            "No description provided.".to_string()
        } else {
            body.lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(clean, "No description provided.");
    }

    #[test]
    fn test_prompt_format_no_newlines() {
        let body = "Fix the bug\nwhere login fails\n\nwith special chars: \"quotes\" and $dollars";
        let body_clean: String = body
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        let prompt = format!(
            "You are working on GitHub issue #{} for the repo {}. Title: {}. {} Please investigate the codebase and implement a solution for this issue. When you are confident the problem is solved, commit your changes and open a draft pull request with a clear title and description that explains what was changed and why. Reference the issue with 'Closes #{}' in the PR body.",
            1, "owner/repo", "#1 Fix login", body_clean, 1
        );

        assert!(!prompt.contains('\n'));
        assert!(prompt.contains("\"quotes\""));
        assert!(prompt.contains("$dollars"));
    }

    #[test]
    fn test_shell_cmd_format() {
        let prompt_file = "/tmp/octopai-prompt-42.txt";
        let shell_cmd = format!(
            "claude \"$(cat '{}')\" --allowedTools Read,Edit,Bash",
            prompt_file
        );
        assert_eq!(
            shell_cmd,
            "claude \"$(cat '/tmp/octopai-prompt-42.txt')\" --allowedTools Read,Edit,Bash"
        );
    }

    #[test]
    fn test_send_keys_executes_in_tmux_pane() {
        if !tmux_available() {
            eprintln!("Skipping: tmux not available");
            return;
        }

        let session = "octopai-test-sendkeys";

        // Kill any leftover test session
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", session])
            .output();

        // Create a detached session
        let output = Command::new("tmux")
            .args(["new-session", "-d", "-s", session])
            .output()
            .expect("Failed to create tmux session");
        assert!(output.status.success(), "Failed to create tmux session");

        // Write a test prompt to a file
        let test_prompt = "Hello from octopai test";
        let prompt_file = "/tmp/octopai-test-prompt.txt";
        fs::write(prompt_file, test_prompt).expect("Failed to write prompt file");

        // Use the same send-keys approach as production code
        let shell_cmd = format!("cat '{}'", prompt_file);
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", session, "-l", &shell_cmd])
            .output();
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", session, "Enter"])
            .output();

        // Wait for command to execute
        std::thread::sleep(std::time::Duration::from_millis(1000));

        // Capture pane contents
        let output = Command::new("tmux")
            .args(["capture-pane", "-t", session, "-p"])
            .output()
            .expect("Failed to capture pane");

        let pane_contents = String::from_utf8_lossy(&output.stdout);

        // Clean up
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", session])
            .output();
        let _ = fs::remove_file(prompt_file);

        assert!(
            pane_contents.contains(test_prompt),
            "Pane should contain the prompt text. Got:\n{}",
            pane_contents
        );
    }

    #[test]
    fn test_send_keys_with_shell_expansion() {
        if !tmux_available() {
            eprintln!("Skipping: tmux not available");
            return;
        }

        let session = "octopai-test-expansion";

        let _ = Command::new("tmux")
            .args(["kill-session", "-t", session])
            .output();

        let output = Command::new("tmux")
            .args(["new-session", "-d", "-s", session])
            .output()
            .expect("Failed to create tmux session");
        assert!(output.status.success());

        // Write test content
        let prompt_file = "/tmp/octopai-test-expansion.txt";
        fs::write(prompt_file, "expanded-prompt-content").unwrap();

        // Use the exact same pattern as the claude command
        let shell_cmd = format!("echo \"$(cat '{}')\"", prompt_file);

        std::thread::sleep(std::time::Duration::from_millis(500));

        let _ = Command::new("tmux")
            .args(["send-keys", "-t", session, "-l", &shell_cmd])
            .output();
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", session, "Enter"])
            .output();

        std::thread::sleep(std::time::Duration::from_millis(1000));

        let output = Command::new("tmux")
            .args(["capture-pane", "-t", session, "-p"])
            .output()
            .expect("Failed to capture pane");

        let pane_contents = String::from_utf8_lossy(&output.stdout);

        let _ = Command::new("tmux")
            .args(["kill-session", "-t", session])
            .output();
        let _ = fs::remove_file(prompt_file);

        assert!(
            pane_contents.contains("expanded-prompt-content"),
            "Shell expansion should have produced the file contents. Got:\n{}",
            pane_contents
        );
    }
}
