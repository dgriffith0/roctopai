<p align="center">
  <img src="image.png" alt="octopai" width="200" />
</p>

<h1 align="center">octopai</h1>

<p align="center">
  <a href="https://github.com/dgriffith0/octopai/releases/latest"><img src="https://img.shields.io/github/v/release/dgriffith0/octopai" alt="Latest Release" /></a>
  <a href="https://github.com/dgriffith0/octopai/blob/main/LICENSE"><img src="https://img.shields.io/github/license/dgriffith0/octopai" alt="License" /></a>
</p>

<p align="center">A terminal UI for managing GitHub issues, worktrees, and AI-powered coding sessions.<br/>Built with Rust and <a href="https://github.com/ratatui/ratatui">Ratatui</a>.</p>

---

## Why use it

Turning a GitHub issue into a working pull request involves a lot of manual steps: creating a branch, setting up a worktree, launching an AI assistant with the right context, monitoring progress, and managing the resulting PR. Octopai collapses all of that into a single kanban board in your terminal.

- **One keypress to go from issue to AI session** — press `w` on any issue and octopai creates a worktree, opens a terminal session, and drops Claude or Cursor in with the full issue context
- **Everything in one view** — issues, worktrees, sessions, and pull requests shown side by side
- **Real-time status** — see whether your AI assistant is working, idle, or waiting for permission
- **Full lifecycle management** — create issues, review PRs, merge, revert, and clean up worktrees
- **Stay in the terminal** — no browser tabs, no context switching

---

## Requirements

Octopai checks for these on startup and will tell you what's missing.

| Dependency | Required | Why it's needed |
|---|---|---|
| [git](https://git-scm.com/) | Yes | Worktree creation and branch management |
| [python3](https://www.python.org/) | Yes | Runs the hook script that reports session status back to the board via Unix socket |
| [claude](https://docs.anthropic.com/en/docs/claude-code) **or** [cursor](https://www.cursor.com/) | Yes | AI coding assistant — at least one is required |
| [tmux](https://github.com/tmux/tmux) | Recommended | Preferred terminal multiplexer — faster pane capture, better scripting interface, and native working directory support. Falls back to GNU Screen if not installed |
| [gh](https://cli.github.com/) | Recommended | Fetching issues, creating PRs, merging, etc. Without it, octopai runs in local mode using a JSON-based store |

---

## Install

### Homebrew (macOS)

```sh
brew install dgriffith0/octopai/octopai
```

### Pre-built binaries

Download the latest release for your platform from [GitHub Releases](https://github.com/dgriffith0/octopai/releases/latest).

Supported targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`.

### Cargo

From GitHub:

```sh
cargo install --git https://github.com/dgriffith0/octopai
```

Or clone and build locally:

```sh
git clone https://github.com/dgriffith0/octopai.git
cd octopai
cargo install --path .
```

## Quick start

Run `octopai` inside a git repo and it will automatically detect the repository and open the board. If `gh` is installed and authenticated, octopai connects to GitHub for issues and PRs. Without `gh`, it runs in **local mode**, storing issues and PRs in a JSON file at `~/.config/octopai/local/`. You can also toggle local mode in the configuration screen (`C` → `P`).

If you run it outside a repo, you'll be prompted to enter a GitHub user or organization name and pick a repository. Press `Enter` on the board to switch repos at any time.

---

## The board

Four columns: **Issues**, **Worktrees**, **Sessions**, and **Pull Requests**. Use `Tab` / `Shift+Tab` to move between columns and `j`/`k` or arrow keys to navigate. Selecting an item highlights related cards across all columns.

### Typical workflow

1. Browse issues or press `n` to create a new one
2. Press `w` to create a worktree and launch an AI session
3. Monitor session status on the board — attach with `a` if needed
4. When a PR appears, review it, mark ready with `r`, and merge with `M`

---

## Keybindings

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit or cancel |
| `Tab` / `l` | Next column |
| `Shift+Tab` / `h` | Previous column |
| `j` / `k` / arrows | Navigate cards |
| `/` | Fuzzy filter |
| `Enter` | Change repo |
| `R` | Force refresh |
| `C` | Edit repo configuration |
| `p` | Pull main branch |
| `D` | Show dependency check |
| `x` | Toggle message log |

**Issues** — `n` new issue (with option to skip worktree), `w` create worktree + session, `d` close, `s` toggle open/closed, `m` toggle assigned-to-me

**Worktrees** — `w` create session, `d` remove, `v` verify, `e` open editor

**Sessions** — `a` attach, `d` kill

**Pull Requests** — `o` open in browser, `r` mark ready, `M` merge, `V` revert, `s` toggle open/closed, `m` toggle assigned-to-me

---

## Worktree + AI session

Pressing `w` on an issue (or `n` to create a new one) creates a git worktree at `../<repo>-issue-<number>`, opens a multiplexer session with Claude or Cursor, and feeds the issue context as a prompt. A hook script reports status back to the board via Unix socket.

Octopai supports both **tmux** and **GNU Screen** as session multiplexers. You can toggle between them by pressing `C` to open the configuration page. At least one must be installed; if both are available, octopai defaults to tmux.

### Multiplexer

A terminal multiplexer lets you run multiple terminal sessions inside a single window and detach or reattach to them at will. Octopai uses one to give each AI session its own isolated terminal that it can monitor and attach to from the board.

[GNU Screen](https://www.gnu.org/software/screen/) comes pre-installed on most Unix systems, so it works out of the box. However, [tmux](https://github.com/tmux/tmux) is the preferred multiplexer for octopai:

- **Pane capture** — tmux's `capture-pane` reads pane content directly from its internal buffer, which is faster and more reliable than Screen's `hardcopy` approach of writing to a temporary file
- **Scripting interface** — tmux commands return structured, predictable output that's easier to parse for session listing and state detection
- **Working directory support** — tmux's `-c` flag sets the starting directory natively when creating sessions, avoiding extra shell commands

Press `C` to configure per-repo session commands. Templates support: `{prompt_file}`, `{issue_number}`, `{repo}`, `{title}`, `{body}`, `{branch}`, `{worktree_path}`, `{claude}`, `{cursor}`.

---

## License

MIT
