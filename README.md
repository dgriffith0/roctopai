# roctopai

A terminal UI for managing GitHub issues, worktrees, and AI-powered coding sessions.

Built with Rust and [Ratatui](https://github.com/ratatui/ratatui).

## What it does

Roctopai gives you a kanban-style board in your terminal with four columns: **Issues**, **Worktrees**, **Sessions**, and **Pull Requests**. Select a GitHub repo, browse its issues, and spin up a git worktree with a tmux session where Claude works on the issue autonomously.

### Features

- **Repository selection** — search by GitHub org or user with fuzzy filtering; last selection is saved
- **Issue management** — create, close, and browse issues with word-wrapping in the issue body
- **Worktree lifecycle** — create isolated worktrees per issue, auto-cleanup when PRs are merged
- **Claude AI sessions** — auto-launch Claude in a tmux session with the issue context as a prompt
- **Pull request actions** — mark draft PRs as ready, merge with one key, revert merged PRs
- **Real-time session status** — Unix socket listens for Claude hook events to show working/idle/waiting state
- **Filtering** — toggle open/closed state and assigned-to-me on issues and pull requests; fuzzy search across all columns
- **Related highlighting** — selecting an item highlights its related issue, worktree, session, and PR across columns
- **Auto-refresh** — board data refreshes every 30 seconds with a countdown timer displayed in the corner
- **Auto-assign** — issues and worktrees are automatically assigned to the current user
- **Verify worktree** — launch an Alacritty window running `cargo run` in a worktree directory

## Prerequisites

- [gh](https://cli.github.com/) (GitHub CLI, authenticated)
- [git](https://git-scm.com/)
- [tmux](https://github.com/tmux/tmux)
- [claude](https://claude.ai/claude-code) (Claude Code CLI)

## Install

```sh
cargo install --path .
```

## Usage

```sh
roctopai
```

On first launch you'll be prompted to enter a GitHub user or org. Pick a repo and you're on the board.

## Keybindings

### Global

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit (or cancel current action) |
| `Tab` / `Shift+Tab` | Switch column |
| `j` / `k` / `Up` / `Down` | Navigate cards |
| `/` | Fuzzy filter |
| `Enter` | Change repo |
| `R` | Force refresh |

### Issues column

| Key | Action |
|-----|--------|
| `n` | New issue (auto-creates worktree + session) |
| `w` | Create worktree + Claude session for selected issue |
| `d` | Close issue |
| `s` | Toggle open/closed issues |
| `m` | Toggle assigned-to-me filter |

### Worktrees column

| Key | Action |
|-----|--------|
| `d` | Remove worktree (deletes branch and kills tmux session) |
| `v` | Verify — launch Alacritty with `cargo run` in the worktree |

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
| `Enter` | Move to body field (from title) / New line (in body) |
| `Ctrl+s` | Submit issue |
| `Esc` | Cancel |

## Worktree + Claude session

Pressing `w` on an issue (or `n` to create a new one):

1. Creates a git worktree at `../<repo>-issue-<number>` on branch `issue-<number>`
2. Opens a tmux session with Claude running in a single pane
3. Claude receives the issue title and body as a prompt and begins working
4. A hook script reports Claude's status back via Unix socket

```
┌─────────────────────────────┐
│                             │
│          claude -p          │
│                             │
└─────────────────────────────┘
```

Attach to a session from the board by pressing `a` in the Sessions column, or from the terminal with `tmux attach -t issue-<number>`.

## Architecture

The codebase is organized into modules:

| Module | Purpose |
|--------|---------|
| `main.rs` | Entry point, event loop, keybinding dispatch |
| `app.rs` | Application state and navigation |
| `models.rs` | Data structures, enums, and constants |
| `ui.rs` | Ratatui rendering for all screens |
| `github.rs` | GitHub CLI integration (issues, PRs, repos) |
| `git.rs` | Git operations (worktrees, branches, cleanup) |
| `session.rs` | Tmux session management and Claude prompting |
| `config.rs` | Persistent configuration |
| `hooks.rs` | Unix socket event server and hook scripts |

## License

MIT
