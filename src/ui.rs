use std::collections::HashSet;
use std::time::Duration;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
    Frame,
};

use crate::app::App;
use crate::config::config_path;
use crate::deps::Dependency;
use crate::models::{
    card_matches, Card, ConfirmModal, IssueModal, Mode, RepoSelectPhase, RepoSelectState,
    REFRESH_INTERVAL,
};

pub fn ui_repo_select(frame: &mut Frame, state: &RepoSelectState) {
    let area = frame.area();

    // Center the content vertically
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(0),
            Constraint::Percentage(30),
        ])
        .split(area);

    // Center horizontally
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(vertical[1]);

    let center = horizontal[1];

    match state.phase {
        RepoSelectPhase::Typing => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(2),
                    Constraint::Min(0),
                ])
                .split(center);

            // Title
            let title = Paragraph::new(Line::from(vec![Span::styled(
                "Enter GitHub user or org:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]))
            .block(Block::default());
            frame.render_widget(title, chunks[0]);

            // Input field
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
                .title(" Owner ");
            let input_text = Paragraph::new(Line::from(vec![
                Span::styled(
                    &state.input,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("_", Style::default().fg(Color::Cyan)),
            ]))
            .block(input_block);
            frame.render_widget(input_text, chunks[1]);

            // Error message
            if let Some(err) = &state.error {
                let err_text = Paragraph::new(Line::from(vec![Span::styled(
                    err.as_str(),
                    Style::default().fg(Color::Red),
                )]));
                frame.render_widget(err_text, chunks[2]);
            }

            // Hint
            let hint = Paragraph::new(Line::from(vec![Span::styled(
                "Press Enter to fetch repos, Esc to go back",
                Style::default().fg(Color::DarkGray),
            )]));
            frame.render_widget(hint, chunks[3]);
        }
        RepoSelectPhase::Loading => {
            let loading = Paragraph::new(Line::from(vec![Span::styled(
                "Fetching repositories...",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]))
            .block(Block::default());
            frame.render_widget(loading, center);
        }
        RepoSelectPhase::Picking => {
            let max_visible = (center.height.saturating_sub(5)) as usize; // reserve space for header + filter
            let list_height = if max_visible > 0 { max_visible } else { 1 };

            let mut constraints = vec![
                Constraint::Length(1), // title
                Constraint::Length(1), // filter line
                Constraint::Length(1), // separator
            ];
            for _ in 0..list_height.min(state.filtered_repos.len()) {
                constraints.push(Constraint::Length(1));
            }
            constraints.push(Constraint::Min(0)); // hint at bottom

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(center);

            // Title
            let title = Paragraph::new(Line::from(vec![
                Span::styled(
                    "Select a repository",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ({} repos)", state.filtered_repos.len()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            frame.render_widget(title, chunks[0]);

            // Filter line
            let filter_line = if state.filter_query.is_empty() {
                Paragraph::new(Line::from(vec![Span::styled(
                    "Type to filter...",
                    Style::default().fg(Color::DarkGray),
                )]))
            } else {
                Paragraph::new(Line::from(vec![
                    Span::styled("/ ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        &state.filter_query,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("_", Style::default().fg(Color::Cyan)),
                ]))
            };
            frame.render_widget(filter_line, chunks[1]);

            // Separator
            let sep = Paragraph::new(Line::from(vec![Span::styled(
                "─".repeat(center.width as usize),
                Style::default().fg(Color::DarkGray),
            )]));
            frame.render_widget(sep, chunks[2]);

            // Scrolled repo list
            let scroll_offset = if state.selected >= list_height {
                state.selected - list_height + 1
            } else {
                0
            };

            let visible_count = list_height.min(state.filtered_repos.len());
            for i in 0..visible_count {
                let repo_idx = scroll_offset + i;
                if repo_idx >= state.filtered_repos.len() {
                    break;
                }
                let is_selected = repo_idx == state.selected;
                let repo_name = &state.filtered_repos[repo_idx];
                let line = if is_selected {
                    Line::from(vec![
                        Span::styled(" > ", Style::default().fg(Color::Cyan)),
                        Span::styled(
                            repo_name.as_str(),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled("   ", Style::default()),
                        Span::styled(repo_name.as_str(), Style::default().fg(Color::Gray)),
                    ])
                };
                frame.render_widget(Paragraph::new(line), chunks[3 + i]);
            }

            // Hint at bottom
            let hint_idx = 3 + visible_count;
            if hint_idx < chunks.len() {
                let hint = Paragraph::new(Line::from(vec![Span::styled(
                    "j/k ↑/↓ navigate  Enter select  Esc back",
                    Style::default().fg(Color::DarkGray),
                )]));
                frame.render_widget(hint, chunks[hint_idx]);
            }
        }
    }
}

pub fn ui_dependencies(frame: &mut Frame, deps: &[Dependency]) {
    let area = frame.area();

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);

    // Title bar
    let title_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Dependencies ");
    let title_text = Paragraph::new(Line::from(vec![Span::styled(
        "  External Dependency Status",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]))
    .block(title_block);
    frame.render_widget(title_text, vertical[0]);

    // Dependency list
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .padding(Padding::new(2, 2, 1, 0));
    let inner = content_block.inner(vertical[1]);
    frame.render_widget(content_block, vertical[1]);

    let mut constraints: Vec<Constraint> = Vec::new();
    // Header row
    constraints.push(Constraint::Length(2));
    // One row per dependency
    for _ in deps {
        constraints.push(Constraint::Length(2));
    }
    constraints.push(Constraint::Min(0));

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    // Header
    let header = Paragraph::new(Line::from(vec![Span::styled(
        format!(
            "{:<14} {:<10} {:<46} {}",
            "Command", "Status", "Description", "Version"
        ),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]));
    frame.render_widget(header, rows[0]);

    // Dependency rows
    for (i, dep) in deps.iter().enumerate() {
        let (status_text, status_color) = if dep.available {
            ("OK", Color::Green)
        } else if dep.required {
            ("MISSING", Color::Red)
        } else {
            ("MISSING", Color::Yellow)
        };

        let version = dep.version.as_deref().unwrap_or("-");

        let line = Line::from(vec![
            Span::styled(
                format!("{:<14} ", dep.name),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10} ", status_text),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<46} ", dep.description),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(version, Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(Paragraph::new(line), rows[1 + i]);
    }

    // Bottom hint bar
    let has_missing = deps.iter().any(|d| d.required && !d.available);
    let bottom_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(vertical[2]);

    let key_style = Style::default()
        .fg(Color::White)
        .bg(Color::Rgb(60, 60, 60))
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);

    let mut hints: Vec<Span> = vec![
        Span::styled(" r ", key_style),
        Span::styled(" Re-check ", desc_style),
    ];
    if has_missing {
        hints.push(Span::styled(" Esc/q ", key_style));
        hints.push(Span::styled(" Quit ", desc_style));
    } else {
        hints.push(Span::styled(" Enter ", key_style));
        hints.push(Span::styled(" Continue ", desc_style));
        hints.push(Span::styled(" Esc/q ", key_style));
        hints.push(Span::styled(" Back ", desc_style));
    }

    frame.render_widget(Paragraph::new(Line::from(hints)), bottom_rows[0]);

    if has_missing {
        let warning = Paragraph::new(Line::from(vec![Span::styled(
            " Install missing required dependencies to continue",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )]));
        frame.render_widget(warning, bottom_rows[1]);
    }
}

pub fn ui(frame: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(frame.area());

    // Top bar — selected repository
    let repo_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Repository ");
    let repo_text = Paragraph::new(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            &app.repo,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  (Enter to change)", Style::default().fg(Color::DarkGray)),
    ]))
    .block(repo_block);
    frame.render_widget(repo_text, outer[0]);

    // Four columns
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(outer[1]);

    let issue_title = format!(
        " Issues [{}|{}] ",
        app.issue_state_filter.label(),
        app.issue_assignee_filter.label()
    );
    let pr_title = format!(
        " Pull Requests [{}|{}] ",
        app.pr_state_filter.label(),
        app.pr_assignee_filter.label()
    );
    let section_data: [(&str, Color, &[Card]); 4] = [
        (&issue_title, Color::Red, &app.issues),
        (" Worktrees ", Color::Yellow, &app.worktrees),
        (" Sessions ", Color::Blue, &app.sessions),
        (&pr_title, Color::Magenta, &app.pull_requests),
    ];

    let filter_query = match &app.mode {
        Mode::Filtering { query } => Some(query.as_str()),
        _ => None,
    };

    let related_ids = app.selected_card_related_ids();

    for (i, (title, color, cards)) in section_data.iter().enumerate() {
        let is_active = i == app.active_section;
        let query = if is_active { filter_query } else { None };
        let selected = if is_active {
            Some(app.selected_card[i])
        } else {
            None
        };
        render_column(
            frame,
            columns[i],
            title,
            *color,
            cards,
            is_active,
            query,
            selected,
            &related_ids,
        );
    }

    // Bottom legend bar (two lines: global on top, area-specific on bottom)
    let key_style = Style::default()
        .fg(Color::White)
        .bg(Color::Rgb(60, 60, 60))
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let key_accent = Style::default()
        .fg(Color::Black)
        .bg(Color::Green)
        .add_modifier(Modifier::BOLD);

    // Top line: global actions (or mode-specific actions for non-Normal modes)
    let mut global_spans: Vec<Span> = Vec::new();

    // Status message prefix
    if let Some(msg) = &app.status_message {
        global_spans.push(Span::styled(
            format!(" {} ", msg),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        global_spans.push(Span::styled(" | ", desc_style));
    }

    let global_mode_spans: Vec<Span> = match &app.mode {
        Mode::Normal => {
            vec![
                Span::styled(" q/Esc ", key_style),
                Span::styled(" Quit ", desc_style),
                Span::styled(" h/l Tab/S-Tab ", key_style),
                Span::styled(" Switch column ", desc_style),
                Span::styled(" j/k ↑/↓ ", key_style),
                Span::styled(" Navigate ", desc_style),
                Span::styled(" / ", key_style),
                Span::styled(" Filter ", desc_style),
                Span::styled(" Enter ", key_style),
                Span::styled(" Change repo ", desc_style),
                Span::styled(" R ", key_style),
                Span::styled(" Refresh ", desc_style),
                Span::styled(" n ", key_accent),
                Span::styled(" New issue ", desc_style),
                Span::styled(" D ", key_style),
                Span::styled(" Deps ", desc_style),
                Span::styled(" C ", key_style),
                Span::styled(" Config ", desc_style),
            ]
        }
        Mode::Filtering { .. } => vec![
            Span::styled(" Esc ", key_style),
            Span::styled(" Clear filter ", desc_style),
            Span::styled(" ↑/↓ ", key_style),
            Span::styled(" Navigate ", desc_style),
        ],
        Mode::CreatingIssue => vec![
            Span::styled(" Esc ", key_style),
            Span::styled(" Cancel ", desc_style),
            Span::styled(" Tab ", key_style),
            Span::styled(" Switch field ", desc_style),
            Span::styled(" Ctrl+S ", key_accent),
            Span::styled(" Submit ", desc_style),
        ],
        Mode::Confirming => vec![
            Span::styled(" y ", key_accent),
            Span::styled(" Confirm ", desc_style),
            Span::styled(" n/Esc ", key_style),
            Span::styled(" Cancel ", desc_style),
        ],
        Mode::EditingVerifyCommand { .. } | Mode::EditingEditorCommand { .. } => vec![
            Span::styled(" Enter ", key_accent),
            Span::styled(" Save & run ", desc_style),
            Span::styled(" Esc ", key_style),
            Span::styled(" Cancel ", desc_style),
        ],
    };
    global_spans.extend(global_mode_spans);

    // Bottom line: area-specific actions
    let section_names = ["Issues", "Worktrees", "Sessions", "Pull Requests"];
    let section_colors = [Color::Red, Color::Yellow, Color::Blue, Color::Magenta];
    let mut area_spans: Vec<Span> = Vec::new();

    if matches!(app.mode, Mode::Normal) {
        let section_label_style = Style::default()
            .fg(section_colors[app.active_section])
            .add_modifier(Modifier::BOLD);
        area_spans.push(Span::styled(
            format!(" {} ", section_names[app.active_section]),
            section_label_style,
        ));
        area_spans.push(Span::styled("│ ", desc_style));

        match app.active_section {
            0 => {
                area_spans.push(Span::styled(" w ", key_accent));
                area_spans.push(Span::styled(" Worktree+Claude ", desc_style));
                area_spans.push(Span::styled(" d ", key_style));
                area_spans.push(Span::styled(" Close issue ", desc_style));
                area_spans.push(Span::styled(" s ", key_style));
                area_spans.push(Span::styled(" Open/Closed ", desc_style));
                area_spans.push(Span::styled(" m ", key_style));
                area_spans.push(Span::styled(" Assigned to me ", desc_style));
            }
            1 => {
                area_spans.push(Span::styled(" e ", key_accent));
                area_spans.push(Span::styled(" Editor ", desc_style));
                area_spans.push(Span::styled(" v ", key_accent));
                area_spans.push(Span::styled(" Verify ", desc_style));
                area_spans.push(Span::styled(" d ", key_style));
                area_spans.push(Span::styled(" Remove worktree ", desc_style));
            }
            2 => {
                area_spans.push(Span::styled(" a ", key_accent));
                area_spans.push(Span::styled(" Attach session ", desc_style));
                area_spans.push(Span::styled(" d ", key_style));
                area_spans.push(Span::styled(" Kill session ", desc_style));
            }
            3 => {
                area_spans.push(Span::styled(" o ", key_accent));
                area_spans.push(Span::styled(" Open in browser ", desc_style));
                area_spans.push(Span::styled(" r ", key_accent));
                area_spans.push(Span::styled(" Mark ready ", desc_style));
                area_spans.push(Span::styled(" M ", key_accent));
                area_spans.push(Span::styled(" Merge ", desc_style));
                area_spans.push(Span::styled(" V ", key_accent));
                area_spans.push(Span::styled(" Revert ", desc_style));
                area_spans.push(Span::styled(" s ", key_style));
                area_spans.push(Span::styled(" Open/Closed ", desc_style));
                area_spans.push(Span::styled(" m ", key_style));
                area_spans.push(Span::styled(" Assigned to me ", desc_style));
            }
            _ => {}
        }
    }

    // Split bottom area into two rows
    let bottom_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(outer[2]);

    // Top row: global actions with timer on right
    let top_bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(14)])
        .split(bottom_rows[0]);

    let global_legend = Paragraph::new(Line::from(global_spans));
    frame.render_widget(global_legend, top_bottom[0]);

    // Refresh countdown timer
    let remaining = REFRESH_INTERVAL
        .checked_sub(app.last_refresh.elapsed())
        .unwrap_or(Duration::ZERO);
    let secs = remaining.as_secs();
    let timer_text = format!(" ⏱ {}s ", secs);
    let timer_style = if secs <= 5 {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let timer = Paragraph::new(Line::from(Span::styled(timer_text, timer_style)))
        .alignment(ratatui::layout::Alignment::Right);
    frame.render_widget(timer, top_bottom[1]);

    // Bottom row: area-specific actions
    let area_legend = Paragraph::new(Line::from(area_spans));
    frame.render_widget(area_legend, bottom_rows[1]);

    // Render issue modal overlay if open
    if let Some(modal) = &app.issue_modal {
        ui_issue_modal(frame, modal, app.spinner_tick);
    }

    // Render confirm modal overlay if open
    if let Some(modal) = &app.confirm_modal {
        ui_confirm_modal(frame, modal);
    }

    // Render verify command prompt overlay if in EditingVerifyCommand mode
    if let Mode::EditingVerifyCommand { input } = &app.mode {
        ui_verify_prompt(frame, input);
    }

    // Render editor command prompt overlay if in EditingEditorCommand mode
    if let Mode::EditingEditorCommand { input } = &app.mode {
        ui_editor_prompt(frame, input);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn ui_issue_modal(frame: &mut Frame, modal: &IssueModal, spinner_tick: usize) {
    let area = centered_rect(50, 50, frame.area());

    frame.render_widget(Clear, area);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" New Issue ")
        .title_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .padding(Padding::new(1, 1, 1, 0));
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    // Layout: title field (3), body field (remaining), error/spinner (1), hint (1)
    let has_error = modal.error.is_some();
    let has_status = has_error || modal.submitting;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                              // title input
            Constraint::Min(3),                                 // body input
            Constraint::Length(if has_status { 1 } else { 0 }), // error or spinner
            Constraint::Length(1),                              // hint
        ])
        .split(inner);

    // Title field
    let title_style = if modal.submitting {
        Style::default().fg(Color::DarkGray)
    } else if modal.active_field == 0 {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let title_block = Block::default()
        .borders(Borders::ALL)
        .border_style(title_style)
        .title(" Title ");
    let title_text = Paragraph::new(Line::from(vec![
        Span::styled(
            &modal.title,
            Style::default().fg(if modal.submitting {
                Color::DarkGray
            } else {
                Color::White
            }),
        ),
        if modal.active_field == 0 && !modal.submitting {
            Span::styled("_", Style::default().fg(Color::Cyan))
        } else {
            Span::raw("")
        },
    ]))
    .block(title_block);
    frame.render_widget(title_text, chunks[0]);

    // Body field
    let body_style = if modal.submitting {
        Style::default().fg(Color::DarkGray)
    } else if modal.active_field == 1 {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let body_block = Block::default()
        .borders(Borders::ALL)
        .border_style(body_style)
        .title(" Body ");
    let mut body_text = modal.body.clone();
    if modal.active_field == 1 && !modal.submitting {
        body_text.push('_');
    }
    let body_paragraph = Paragraph::new(body_text)
        .style(Style::default().fg(if modal.submitting {
            Color::DarkGray
        } else {
            Color::White
        }))
        .block(body_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(body_paragraph, chunks[1]);

    // Spinner or error
    if modal.submitting {
        let spinner = SPINNER_FRAMES[spinner_tick % SPINNER_FRAMES.len()];
        let spinner_text = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", spinner),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Creating issue...",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        frame.render_widget(spinner_text, chunks[2]);
    } else if let Some(err) = &modal.error {
        let err_text = Paragraph::new(Line::from(vec![Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        )]));
        frame.render_widget(err_text, chunks[2]);
    }

    // Hint
    let hint_text = if modal.submitting {
        "Esc: cancel"
    } else {
        "Tab: switch field | Ctrl+S: submit | Esc: cancel"
    };
    let hint = Paragraph::new(Line::from(vec![Span::styled(
        hint_text,
        Style::default().fg(Color::DarkGray),
    )]));
    frame.render_widget(hint, chunks[3]);
}

fn ui_confirm_modal(frame: &mut Frame, modal: &ConfirmModal) {
    let area = centered_rect(50, 20, frame.area());

    frame.render_widget(Clear, area);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(" Confirm ")
        .title_style(
            Style::default()
                .fg(Color::White)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )
        .padding(Padding::new(1, 1, 1, 0));
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let message = Paragraph::new(modal.message.as_str()).style(Style::default().fg(Color::White));
    frame.render_widget(message, chunks[0]);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled(
            "y",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" confirm  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "n",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
    ]));
    frame.render_widget(hint, chunks[1]);
}

fn render_column(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    color: Color,
    cards: &[Card],
    is_active: bool,
    filter_query: Option<&str>,
    selected: Option<usize>,
    related_ids: &HashSet<String>,
) {
    let border_style = if is_active {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color)
    };
    let col_block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title)
        .title_style(if is_active {
            Style::default()
                .fg(Color::Black)
                .bg(color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        })
        .padding(Padding::new(1, 1, 1, 0));
    let inner = col_block.inner(area);
    frame.render_widget(col_block, area);

    // Determine content area — if filtering, reserve a line for the search input
    let (cards_area, filter_area) = if let Some(_) = filter_query {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner);
        (split[1], Some(split[0]))
    } else {
        (inner, None)
    };

    // Render filter input if active
    if let (Some(area), Some(query)) = (filter_area, filter_query) {
        let input = Paragraph::new(Line::from(vec![
            Span::styled("/ ", Style::default().fg(Color::Cyan)),
            Span::styled(
                query,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", Style::default().fg(Color::Cyan)),
        ]));
        frame.render_widget(input, area);
    }

    // Filter cards
    let visible_cards: Vec<&Card> = if let Some(query) = filter_query {
        if query.is_empty() {
            cards.iter().collect()
        } else {
            cards.iter().filter(|c| card_matches(c, query)).collect()
        }
    } else {
        cards.iter().collect()
    };

    let card_height = 4u16;
    let max_visible = (cards_area.height / card_height) as usize;
    let total = visible_cards.len();

    // Calculate scroll offset to keep the selected card visible
    let scroll_offset = if let Some(sel) = selected {
        if max_visible == 0 {
            0
        } else if sel >= max_visible {
            sel - max_visible + 1
        } else {
            0
        }
    } else {
        0
    };

    let display_count = max_visible.min(total.saturating_sub(scroll_offset));
    let display_cards: Vec<&Card> = visible_cards
        .iter()
        .skip(scroll_offset)
        .take(display_count)
        .copied()
        .collect();

    let mut constraints: Vec<Constraint> = display_cards
        .iter()
        .map(|_| Constraint::Length(card_height))
        .collect();
    constraints.push(Constraint::Min(0));

    let slots = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(cards_area);

    for (i, card) in display_cards.iter().enumerate() {
        let original_idx = scroll_offset + i;
        let is_selected = selected.is_some_and(|s| s == original_idx);
        let is_related = !is_selected && related_ids.contains(&card.id);
        render_card(frame, slots[i], card, is_selected, is_related);
    }
}

fn render_card(frame: &mut Frame, area: Rect, card: &Card, is_selected: bool, is_related: bool) {
    let border_style = if is_selected {
        Style::default()
            .fg(Color::Rgb(255, 200, 50))
            .add_modifier(Modifier::BOLD)
    } else if is_related {
        Style::default().fg(Color::Rgb(180, 160, 100))
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let card_block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = card_block.inner(area);
    frame.render_widget(card_block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    // Title line with tag
    let tag = Span::styled(
        format!(" {} ", card.tag),
        Style::default().fg(Color::Black).bg(card.tag_color),
    );
    let title = Span::styled(
        format!(" {}", card.title),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(Paragraph::new(Line::from(vec![tag, title])), lines[0]);

    // Description
    let desc = Paragraph::new(Span::styled(
        &card.description,
        Style::default().fg(Color::Gray),
    ));
    frame.render_widget(desc, lines[1]);
}

fn ui_verify_prompt(frame: &mut Frame, input: &str) {
    let area = centered_rect(50, 20, frame.area());

    frame.render_widget(Clear, area);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Set Verify Command ")
        .title_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .padding(Padding::new(1, 1, 1, 0));
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // label
            Constraint::Length(3), // input
            Constraint::Min(1),    // hint
        ])
        .split(inner);

    let label = Paragraph::new(Line::from(vec![Span::styled(
        "No verify command configured for this repo. Enter a command:",
        Style::default().fg(Color::White),
    )]));
    frame.render_widget(label, chunks[0]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White))
        .title(" Command ");
    let input_text = Paragraph::new(Line::from(vec![
        Span::styled(
            input,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("_", Style::default().fg(Color::Cyan)),
    ]))
    .block(input_block);
    frame.render_widget(input_text, chunks[1]);

    let hint = Paragraph::new(Line::from(vec![Span::styled(
        "e.g. cargo run, npm start, make run  |  Enter: save & run  Esc: cancel",
        Style::default().fg(Color::DarkGray),
    )]));
    frame.render_widget(hint, chunks[2]);
}

fn ui_editor_prompt(frame: &mut Frame, input: &str) {
    let area = centered_rect(50, 20, frame.area());

    frame.render_widget(Clear, area);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(" Set Editor Command ")
        .title_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
        .padding(Padding::new(1, 1, 1, 0));
    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // label
            Constraint::Length(3), // input
            Constraint::Min(1),    // hint
        ])
        .split(inner);

    let label = Paragraph::new(Line::from(vec![Span::styled(
        "No editor configured for this repo. Enter a command:",
        Style::default().fg(Color::White),
    )]));
    frame.render_widget(label, chunks[0]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White))
        .title(" Command ");
    let input_text = Paragraph::new(Line::from(vec![
        Span::styled(
            input,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("_", Style::default().fg(Color::Cyan)),
    ]))
    .block(input_block);
    frame.render_widget(input_text, chunks[1]);

    let hint = Paragraph::new(Line::from(vec![Span::styled(
        "e.g. nvim, code, vim, hx  |  Enter: save & open  Esc: cancel",
        Style::default().fg(Color::DarkGray),
    )]));
    frame.render_widget(hint, chunks[2]);
}

pub fn ui_configuration(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);

    // Title bar
    let title_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Configuration ");
    let title_text = Paragraph::new(Line::from(vec![Span::styled(
        format!("  Configuration for {}", app.repo),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]))
    .block(title_block);
    frame.render_widget(title_text, vertical[0]);

    // Content
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .padding(Padding::new(2, 2, 1, 0));
    let inner = content_block.inner(vertical[1]);
    frame.render_widget(content_block, vertical[1]);

    if let Some(config_edit) = &app.config_edit {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // verify label
                Constraint::Length(3), // verify input
                Constraint::Length(1), // spacing
                Constraint::Length(1), // editor label
                Constraint::Length(3), // editor input
                Constraint::Length(1), // spacing
                Constraint::Length(1), // pr ready label
                Constraint::Length(1), // pr ready toggle
                Constraint::Length(1), // spacing
                Constraint::Length(1), // config file path
                Constraint::Min(0),
            ])
            .split(inner);

        let verify_active = config_edit.active_field == 0;
        let editor_active = config_edit.active_field == 1;
        let pr_ready_active = config_edit.active_field == 2;

        // Verify command field
        let verify_label = Paragraph::new(Line::from(vec![Span::styled(
            "Verify Command",
            Style::default()
                .fg(if verify_active {
                    Color::Cyan
                } else {
                    Color::Gray
                })
                .add_modifier(Modifier::BOLD),
        )]));
        frame.render_widget(verify_label, chunks[0]);

        let verify_border = if verify_active {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let verify_block = Block::default()
            .borders(Borders::ALL)
            .border_style(verify_border)
            .title(" Command ");
        let mut verify_spans = vec![Span::styled(
            &config_edit.verify_command,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )];
        if verify_active {
            verify_spans.push(Span::styled("_", Style::default().fg(Color::Cyan)));
        }
        let verify_text = Paragraph::new(Line::from(verify_spans)).block(verify_block);
        frame.render_widget(verify_text, chunks[1]);

        // Editor command field
        let editor_label = Paragraph::new(Line::from(vec![Span::styled(
            "Editor Command",
            Style::default()
                .fg(if editor_active {
                    Color::Cyan
                } else {
                    Color::Gray
                })
                .add_modifier(Modifier::BOLD),
        )]));
        frame.render_widget(editor_label, chunks[3]);

        let editor_border = if editor_active {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let editor_block = Block::default()
            .borders(Borders::ALL)
            .border_style(editor_border)
            .title(" Command ");
        let mut editor_spans = vec![Span::styled(
            &config_edit.editor_command,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )];
        if editor_active {
            editor_spans.push(Span::styled("_", Style::default().fg(Color::Cyan)));
        }
        let editor_text = Paragraph::new(Line::from(editor_spans)).block(editor_block);
        frame.render_widget(editor_text, chunks[4]);

        // PR Ready toggle field
        let pr_ready_label = Paragraph::new(Line::from(vec![Span::styled(
            "Open PRs as Ready (not draft)",
            Style::default()
                .fg(if pr_ready_active {
                    Color::Cyan
                } else {
                    Color::Gray
                })
                .add_modifier(Modifier::BOLD),
        )]));
        frame.render_widget(pr_ready_label, chunks[6]);

        let checkbox = if config_edit.pr_ready { "[x]" } else { "[ ]" };
        let toggle_color = if pr_ready_active {
            Color::White
        } else {
            Color::DarkGray
        };
        let pr_ready_text = Paragraph::new(Line::from(vec![
            Span::styled(
                checkbox,
                Style::default()
                    .fg(toggle_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if config_edit.pr_ready {
                    "  Enabled — PRs will be opened as ready"
                } else {
                    "  Disabled — PRs will be opened as draft"
                },
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        frame.render_widget(pr_ready_text, chunks[7]);

        let path_label = Paragraph::new(Line::from(vec![
            Span::styled("Config file: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                config_path().to_string_lossy().to_string(),
                Style::default().fg(Color::Gray),
            ),
        ]));
        frame.render_widget(path_label, chunks[9]);
    }

    // Bottom hint bar
    let bottom_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(vertical[2]);

    let key_style = Style::default()
        .fg(Color::White)
        .bg(Color::Rgb(60, 60, 60))
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let key_accent = Style::default()
        .fg(Color::Black)
        .bg(Color::Green)
        .add_modifier(Modifier::BOLD);

    let hints = vec![
        Span::styled(" Tab ", key_style),
        Span::styled(" Switch field ", desc_style),
        Span::styled(" Ctrl+S ", key_accent),
        Span::styled(" Save ", desc_style),
        Span::styled(" Esc ", key_style),
        Span::styled(" Cancel ", desc_style),
    ];

    frame.render_widget(Paragraph::new(Line::from(hints)), bottom_rows[0]);
}
