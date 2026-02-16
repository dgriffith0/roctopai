<p align="center">
  <img src="image.png" alt="octopai" width="200" />
</p>

<h1 align="center">octopai</h1>

<p align="center">A terminal UI for managing GitHub issues, worktrees, and AI-powered coding sessions.<br/>Built with Rust and <a href="https://github.com/ratatui/ratatui">Ratatui</a>.</p>

---

## Why use it

Turning a GitHub issue into a working pull request involves a lot of manual steps: creating a branch, setting up a worktree, launching an AI assistant with the right context, monitoring progress, and managing the resulting PR. Octopai collapses all of that into a single kanban board in your terminal.

- **One keypress to go from issue to AI session** — press `w` on any issue and octopai creates a worktree, opens a tmux session, and drops Claude or Cursor in with the full issue context
- **Everything in one view** — issues, worktrees, sessions, and pull requests shown side by side
- **Real-time status** — see whether your AI assistant is working, idle, or waiting for permission
- **Full lifecycle management** — create issues, review PRs, merge, revert, and clean up worktrees
- **Stay in the terminal** — no browser tabs, no context switching

---

## Requirements

Octopai checks for these on startup and will tell you what's missing.

| Dependency | Why it's needed |
|---|---|
| [gh](https://cli.github.com/) (authenticated) | All GitHub operations — fetching issues, creating PRs, merging, etc. |
| [git](https://git-scm.com/) | Worktree creation and branch management |
| [tmux](https://github.com/tmux/tmux) | Each AI session runs in its own tmux window so octopai can monitor and attach to it |
| [nvim](https://neovim.io/) | Default editor opened in worktree sessions |
| [python3](https://www.python.org/) | Runs the hook script that reports session status back to the board via Unix socket |
| [claude](https://docs.anthropic.com/en/docs/claude-code) **or** [cursor](https://www.cursor.com/) | AI coding assistant — at least one is required |

---

## Quick start

From GitHub:

```sh
cargo install --git https://github.com/dgriffith0/octopai
```

Or clone and install locally:

```sh
git clone https://github.com/dgriffith0/octopai.git
cd octopai
cargo install --path .
octopai
```

On first launch you'll be prompted to enter a GitHub user or organization name. Pick a repository and you're on the board.

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
| `Tab` / `Shift+Tab` | Switch column |
| `j` / `k` / arrows | Navigate cards |
| `/` | Fuzzy filter all columns |
| `Enter` | Change repo |
| `R` | Force refresh |
| `C` | Edit repo configuration |

**Issues** — `n` new issue, `w` create worktree + session, `d` close, `s` toggle open/closed, `m` toggle assigned-to-me

**Worktrees** — `d` remove, `v` verify, `e` open editor

**Sessions** — `a` attach, `d` kill

**Pull Requests** — `o` open in browser, `r` mark ready, `M` merge, `V` revert, `s` toggle open/closed, `m` toggle assigned-to-me

---

## Worktree + AI session

Pressing `w` on an issue (or `n` to create a new one) creates a git worktree at `../<repo>-issue-<number>`, opens a tmux session with Claude or Cursor, and feeds the issue context as a prompt. A hook script reports status back to the board via Unix socket.

Press `C` to configure per-repo session commands. Templates support: `{prompt_file}`, `{issue_number}`, `{repo}`, `{title}`, `{body}`, `{branch}`, `{worktree_path}`, `{claude}`, `{cursor}`.

---

## License

MIT
