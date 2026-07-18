# Implementation Phases

## Phase 0 — Prototype spike

Goal: prove the core loop before building the full CLI.

Tasks:

1. Write a minimal Rust binary that runs:
   - `tmux list-sessions -F ...`
   - `tmux list-windows -a -F ...`
   - `tmux list-panes -a -F ...`
2. Build palette rows.
3. Pipe rows into fzf.
4. Parse selected row.
5. Switch to selected target via `tmux switch-client -t`.
6. Verify from desktop tmux and Termius/iPhone-sized terminal.

Cut line:

- no SQLite;
- no config;
- no notes;
- no project discovery;
- no layouts.

Acceptance:

- pressing the tmux binding opens fzf popup;
- selecting a pane switches exactly to that pane;
- mobile popup is legible.

## Phase 1 — MVP

Goal: replace the current practical NTM usage.

Features:

```text
tmx palette
tmx --mobile
tmx ls
tmx view
tmx new
tmx last
tmx recent
tmx note
tmx rename session/window/pane
tmx doctor
SQLite state
TOML config
basic tmux plugin/config file
```

Implementation tasks:

1. Rust CLI skeleton with `clap`.
2. tmux command wrapper with structured error reporting.
3. target discovery from tmux.
4. stable row generation and fzf selector.
5. desktop and mobile UI profiles.
6. current-directory detection from origin pane.
7. create-or-attach project session.
8. rename commands using native tmux commands.
9. session/window/pane notes.
10. SQLite migrations.
11. MRU target stack.
12. minimal `tmux.example.conf`.
13. `tmx doctor` dependency/capability checks.
14. tests for naming, parsing, state, and commands.

MVP exclusions:

```text
FFF integration
Television backend
agent state detection
macOS notifications
scheduled reminders
project-local layout scripts
declarative layouts
cross-machine sync
web/socket APIs
```

## Phase 2 — Layouts, project discovery, and reminder prompts

Goal: make the tool a better daily work launcher.

Features:

```text
tmx layout
tmx projects
project roots scanning
zoxide/fd enrichment
note prompt on switch
context reminders on return
preview pane in desktop mode
explicit tmx grep
basic attention markers from tmux activity/bell
```

Implementation tasks:

1. Global layout script discovery.
2. Trusted project-local layout discovery.
3. Layout runner with cwd/name/label variables.
4. Configurable project roots.
5. Project discovery via fd fallback to Rust directory walk.
6. zoxide import/query for frecency-ranked project rows.
7. Reminder policy engine.
8. Hook integration for MRU timestamps and switch events.
9. Desktop preview using `tmux capture-pane` only transiently.
10. Explicit pane/history grep command.

Cut line:

- no required background service;
- no persisted pane content;
- no semantic agent integration yet.

## Phase 3 — Performance accelerators and optional agent metadata

Goal: add faster repeated project/code search and lightweight “needs attention” metadata without building an agent runtime.

Features:

```text
FFF-backed project file/content search
Television backend experiment
optional Atuin context enrichment
optional macOS notifications
manual/heuristic agent status markers
Claude/Codex hooks that write metadata only
```

Implementation tasks:

1. Add `project_search_backend = "auto|fff|fd_rg|none"`.
2. If implemented in Rust, integrate `fff-search` as an in-process optional crate feature.
3. Use FFF only for repeated project file/content search, not for tmux target switching.
4. Add `selector_backend = "fzf|tv|auto"` for desktop only.
5. Add `tmx attention set/clear` commands for external hooks.
6. Add optional macOS notification command.
7. Add `tmx hooks install` for agent tools that write pane/session metadata.

Strict non-goal:

- do not build Herdr’s background server model;
- do not build cmux’s native app model;
- do not build NTM’s REST/control-plane model.

## Phase 4 — Hardening and distribution

Goal: package and stabilize.

Tasks:

1. Homebrew formula.
2. `cargo install` support.
3. binary releases for macOS ARM64, macOS x86_64, Linux x86_64, Linux ARM64.
4. shell completions for Bash, Zsh, and Fish, generated from the clap CLI definition (implemented).
5. man page.
6. upgrade/migration logic for SQLite schema.
7. comprehensive documentation.
8. compatibility tests across tmux versions.

## Suggested initial milestone order

```text
Milestone 1: tmux discovery + fzf switcher
Milestone 2: current-dir new session + exact target switching
Milestone 3: rename + notes + SQLite
Milestone 4: mobile profile + Termius testing
Milestone 5: MRU/last/recent
Milestone 6: layout shortcuts
Milestone 7: project discovery accelerators
Milestone 8: reminders and preview
Milestone 9: FFF/Television optional acceleration
```
