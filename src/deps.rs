use std::process::Command;

pub struct Dependency {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
    pub available: bool,
    pub version: Option<String>,
}

pub fn check_dependencies() -> Vec<Dependency> {
    vec![
        check_dep("gh", "gh", "GitHub CLI for issue/PR management", true),
        check_dep("git", "git", "Version control with worktree support", true),
        check_dep("tmux", "tmux", "Terminal multiplexer for sessions", true),
        check_dep("nvim", "nvim", "Editor launched in worktree sessions", true),
        check_dep(
            "claude",
            "claude",
            "Claude Code CLI for autonomous work",
            true,
        ),
        check_dep(
            "python3",
            "python3",
            "Used by hook script for socket communication",
            true,
        ),
        check_dep(
            "alacritty",
            "alacritty",
            "Terminal emulator for cargo run (optional)",
            false,
        ),
    ]
}

fn check_dep(
    name: &'static str,
    command: &'static str,
    description: &'static str,
    required: bool,
) -> Dependency {
    // tmux uses -V instead of --version
    let version_flag = if command == "tmux" { "-V" } else { "--version" };

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
