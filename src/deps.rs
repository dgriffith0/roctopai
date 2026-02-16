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
        check_dep("gh", "gh", "GitHub CLI for issue/PR management", true),
        check_dep("git", "git", "Version control with worktree support", true),
    ];

    // Require at least one terminal multiplexer (tmux or screen)
    let tmux = check_dep("tmux", "tmux", "Terminal multiplexer for sessions", false);
    let screen = check_dep(
        "screen",
        "screen",
        "Terminal multiplexer for sessions",
        false,
    );
    let mux_available = tmux.available || screen.available;
    deps.push(Dependency {
        name: "tmux/screen",
        description: "Terminal multiplexer (tmux or GNU Screen)",
        required: true,
        available: mux_available,
        version: if tmux.available {
            tmux.version
        } else {
            screen.version
        },
    });

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
