# roctopai

```
                 _                    _
  _ __ ___   ___| |_ ___  _ __   __ _(_)
 | '__/ _ \ / __| __/ _ \| '_ \ / _` | |
 | | | (_) | (__| || (_) | |_) | (_| | |
 |_|  \___/ \___|\__\___/| .__/ \__,_|_|
                          |_|
```

A terminal UI for managing GitHub issues, worktrees, and AI-powered coding sessions.
Built with Rust and [Ratatui](https://github.com/ratatui/ratatui).

---

## Why use it

Turning a GitHub issue into a working pull request involves a lot of manual steps: creating a branch, setting up a worktree, launching an AI assistant with the right context, monitoring progress, and managing the resulting PR. Roctopai collapses all of that into a single kanban board in your terminal.

- **One keypress to go from issue to AI session** — press `w` on any issue and roctopai creates a worktree, opens a tmux session, and drops Claude or Cursor in with the full issue context
- **Everything in one view** — issues, worktrees, sessions, and pull requests are shown side by side so you always know what's happening
- **Real-time status** — see whether Claude is working, idle, or waiting for permission without leaving the board
- **Full lifecycle management** — create issues, review PRs, merge, revert, and clean up worktrees all from the same interface
- **Stay in the terminal** — no browser tabs, no context switching

---

## Requirements

The following tools must be installed and available on your `PATH`:

| Tool | Purpose |
|------|---------|
| [gh](https://cli.github.com/) | GitHub CLI (must be authenticated via `gh auth login`) |
| [git](https://git-scm.com/) | Version control with worktree support |
| [tmux](https://github.com/tmux/tmux) | Terminal multiplexer for AI sessions |
| [nvim](https://neovim.io/) | Editor for worktree editing |
| [python3](https://www.python.org/) | Used by the hook script for socket communication |
| [claude](https://docs.anthropic.com/en/docs/claude-code) or [cursor](https://www.cursor.com/) | AI coding assistant (at least one required) |

Roctopai checks for these on startup and will tell you what's missing.

---

## How to use it

### Install

```sh
cargo install --path .
```

### Run

```sh
roctopai
```

On first launch you'll be prompted to enter a GitHub user or organization name. Pick a repository and you're on the board.

### The board

The board has four columns:

| Column | What it shows |
|--------|---------------|
| **Issues** | Open and closed GitHub issues |
| **Worktrees** | Git worktrees created for issues |
| **Sessions** | Active tmux sessions running Claude or Cursor |
| **Pull Requests** | PRs linked to issues |

Use `Tab` / `Shift+Tab` to move between columns and `j` / `k` (or arrow keys) to navigate cards. Selecting an item highlights related cards across all columns.

### Typical workflow

1. Browse issues or press `n` to create a new one
2. Press `w` on an issue to create a worktree and launch an AI session
3. Claude (or Cursor) receives the issue context and starts working
4. Monitor session status on the board — attach with `a` if needed
5. When a PR appears, review it, mark it ready with `r`, and merge with `M`

---

## Features

- **Repository selection** — search by GitHub org or user with fuzzy filtering; last selection is saved
- **Issue management** — create, close, and browse issues with word-wrapped descriptions
- **Worktree lifecycle** — create isolated worktrees per issue; auto-cleanup when PRs are merged
- **AI sessions** — auto-launch Claude or Cursor in a tmux session with the issue context as a prompt
- **Pull request actions** — mark draft PRs as ready, merge with one key, revert merged PRs
- **Real-time session status** — Unix socket listens for Claude hook events to show working/idle/waiting state
- **Filtering** — toggle open/closed state and assigned-to-me on issues and PRs; fuzzy search across all columns with `/`
- **Related highlighting** — selecting an item highlights its related issue, worktree, session, and PR
- **Auto-refresh** — board data refreshes every 30 seconds with a countdown timer
- **Auto-assign** — issues and worktrees are automatically assigned to the current user
- **Verify and edit** — launch a terminal window running your verify command, or open an editor in the worktree directory
- **Per-repo configuration** — customize session commands, editor commands, verify commands, and PR-ready behavior per repository
- **Message center** — view hook events and system messages inline

---

## Keybindings

### Global

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit (or cancel current action) |
| `Tab` / `Shift+Tab` | Switch column |
| `j` / `k` / `Up` / `Down` | Navigate cards |
| `/` | Fuzzy filter all columns |
| `Enter` | Change repo |
| `R` | Force refresh |
| `x` | Toggle message center |
| `X` | Expand/collapse messages |
| `C` | Edit configuration for current repo |

### Issues column

| Key | Action |
|-----|--------|
| `n` | New issue (auto-creates worktree + session) |
| `w` | Create worktree + AI session for selected issue |
| `d` | Close issue |
| `s` | Toggle open/closed issues |
| `m` | Toggle assigned-to-me filter |

### Worktrees column

| Key | Action |
|-----|--------|
| `d` | Remove worktree (deletes branch and kills tmux session) |
| `v` | Verify — run the configured verify command in the worktree |
| `e` | Open editor in the worktree directory |

### Sessions column

| Key | Action |
|-----|--------|
| `a` | Attach to tmux session |
| `d` | Kill tmux session |

### Pull Requests column

| Key | Action |
|-----|--------|
| `o` | Open PR in browser |
| `r` | Mark draft PR as ready |
| `M` | Merge PR (merge commit, auto-deletes branch) |
| `V` | Revert a merged PR |
| `s` | Toggle open/closed PRs |
| `m` | Toggle assigned-to-me filter |

### Issue modal

| Key | Action |
|-----|--------|
| `Tab` | Switch between title and body fields |
| `Enter` | Move to body field (from title) / new line (in body) |
| `Ctrl+s` | Submit issue |
| `Esc` | Cancel |

---

## Worktree + AI session

Pressing `w` on an issue (or `n` to create a new one):

1. Creates a git worktree at `../<repo>-issue-<number>` on branch `issue-<number>`
2. Opens a tmux session with Claude or Cursor running in a single pane
3. The AI receives the issue title and body as a prompt and begins working
4. A hook script reports status back to the board via Unix socket

Attach to a session from the board by pressing `a` in the Sessions column, or from the terminal with `tmux attach -t issue-<number>`.

### Custom session commands

Press `C` to configure per-repo commands. Session command templates support these fields:

| Field | Value |
|-------|-------|
| `{prompt_file}` | Temp file containing the issue context |
| `{issue_number}` | Issue number |
| `{repo}` | Repository name |
| `{title}` | Issue title |
| `{body}` | Issue body |
| `{branch}` | Branch name |
| `{worktree_path}` | Path to the worktree |
| `{claude}` | Shortcut for the Claude CLI command |
| `{cursor}` | Shortcut for the Cursor CLI command |

---

## Architecture

| Module | Purpose |
|--------|---------|
| `main.rs` | Entry point, event loop, keybinding dispatch |
| `app.rs` | Application state and navigation |
| `models.rs` | Data structures, enums, and constants |
| `ui.rs` | Ratatui rendering for all screens |
| `github.rs` | GitHub CLI integration (issues, PRs, repos) |
| `git.rs` | Git operations (worktrees, branches, cleanup) |
| `session.rs` | Tmux session management and AI prompting |
| `config.rs` | Persistent configuration |
| `hooks.rs` | Unix socket event server and hook scripts |
| `deps.rs` | Dependency checking for external tools |

---

## License

MIT
