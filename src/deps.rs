use std::process::Command;

pub struct Dependency {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
    pub available: bool,
    pub version: Option<String>,
}

pub fn check_dependencies() -> Vec<Dependency> {
    let mut deps = vec![
        check_dep(
            "gh",
            "gh",
            "GitHub CLI for issue/PR management (optional for local mode)",
            false,
        ),
        check_dep("git", "git", "Version control with worktree support", true),
    ];

    // Terminal multiplexers are optional; tmux is preferred
    deps.push(check_dep(
        "tmux",
        "tmux",
        "Preferred terminal multiplexer for sessions",
        false,
    ));
    deps.push(check_dep(
        "screen",
        "screen",
        "Alternative terminal multiplexer (GNU Screen)",
        false,
    ));

    // Require at least one AI coding assistant (claude or cursor)
    let claude = check_dep(
        "claude",
        "claude",
        "Claude Code CLI for autonomous work",
        false,
    );
    let cursor = check_dep("cursor", "cursor", "Cursor CLI for autonomous work", false);
    let either_available = claude.available || cursor.available;
    deps.push(Dependency {
        name: "claude/cursor",
        description: "AI coding assistant (Claude Code or Cursor)",
        required: true,
        available: either_available,
        version: if claude.available {
            claude.version
        } else {
            cursor.version
        },
    });

    deps.push(check_dep(
        "python3",
        "python3",
        "Used by hook script for socket communication",
        true,
    ));
    deps
}

fn check_dep(
    name: &'static str,
    command: &'static str,
    description: &'static str,
    required: bool,
) -> Dependency {
    // tmux and screen use -V instead of --version
    let version_flag = if command == "tmux" || command == "screen" {
        "-V"
    } else {
        "--version"
    };

    let (available, version) = match Command::new(command).arg(version_flag).output() {
        Ok(output) => {
            let version_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let version_str = if version_str.is_empty() {
                String::from_utf8_lossy(&output.stderr).trim().to_string()
            } else {
                version_str
            };
            // Take just the first line
            let first_line = version_str.lines().next().unwrap_or("").to_string();
            (
                output.status.success(),
                if first_line.is_empty() {
                    None
                } else {
                    Some(first_line)
                },
            )
        }
        Err(_) => (false, None),
    };

    Dependency {
        name,
        description,
        required,
        available,
        version,
    }
}

pub fn has_missing_required(deps: &[Dependency]) -> bool {
    deps.iter().any(|d| d.required && !d.available)
}

/// Check if the GitHub CLI (`gh`) is available.
pub fn gh_available() -> bool {
    Command::new("which")
        .arg("gh")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Detect which AI coding assistants are available on the system.
/// Returns `(claude_available, cursor_available)`.
pub fn detect_ai_tools() -> (bool, bool) {
    let claude = Command::new("which")
        .arg("claude")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let cursor = Command::new("which")
        .arg("cursor-agent")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    (claude, cursor)
}
