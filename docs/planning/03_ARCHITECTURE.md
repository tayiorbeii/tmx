# Architecture

## Overview

`tmx` is a short-lived Rust CLI that queries tmux, builds a normalized model of live sessions/windows/panes, merges lightweight metadata, renders a selector, and executes one tmux command based on the selected row.

It is not a daemon. It does not own process lifetime. tmux owns session/window/pane persistence.

```text
┌──────────────┐
│ user / tmux  │
└──────┬───────┘
       │ keybinding / shell command
┌──────▼───────┐
│ tmx CLI      │ short-lived process
└──────┬───────┘
       │
       ├── query tmux: sessions/windows/panes/formats/options
       ├── query SQLite: notes/MRU/projects/layout trust
       ├── optionally query accelerators: git/fd/zoxide/fff
       ├── render selector: fzf by default
       └── execute tmux action: switch/new/rename/note/layout
```

## Module boundaries

Recommended Rust modules:

```text
src/main.rs
src/cli.rs              clap command definitions
src/tmux/mod.rs         tmux command runner and parsers
src/tmux/formats.rs     format strings and robust field decoding
src/model.rs            Session, Window, Pane, Target, PaletteRow
src/state/mod.rs        SQLite storage and migrations
src/state/schema.rs     schema constants and migration tests
src/config.rs           TOML config loading and defaults
src/palette/mod.rs      row generation, sorting, filtering, actions
src/selector/mod.rs     selector trait and implementations
src/selector/fzf.rs     fzf backend
src/selector/tv.rs      optional Television backend
src/selector/tmux.rs    choose-tree/display-menu fallback
src/project.rs          cwd/repo discovery, project naming, zoxide/fd
src/layout.rs           layout discovery and execution
src/note.rs             notes and prompt policy
src/reminder.rs         reminder decision engine
src/mobile.rs           UI profile selection and client sizing
src/commands/*.rs       command implementations
```

## Core data model

### Target

A target is one of:

```rust
Session { session_id, name, cwd, note }
Window  { session_id, window_id, name, index, cwd, note }
Pane    { session_id, window_id, pane_id, title, command, cwd, note }
Project { path, repo_root, name, source }
Layout  { name, path, trust_state }
Action  { id, label, mobile_safe }
```

Use tmux IDs for live targets. tmux IDs are stable for the life of the tmux server, while names can change and can be ambiguous.

### Palette row

Palette rows should be display-first but action-safe:

```text
row_id<TAB>type<TAB>display<TAB>target_kind<TAB>target_id<TAB>search_blob
```

`fzf` shows only the human-readable display field via `--with-nth 3`; the hidden row and target IDs make selection unambiguous.

Important fzf constraint: when `--with-nth` is active, `--nth` indexes the transformed presentation, not the original encoded row. fzf cannot search hidden original fields. The current renderer therefore includes every supported search value (full kind label, names, title, command, path, and note) in the display field. Do not combine `--with-nth 3` with `--nth 3,6`; that produces an empty search scope on fzf 0.70.0.

Recommended delimiter: ASCII unit separator `\x1f` internally; tab is acceptable for generated rows if fields are escaped. Notes must be stripped of newlines in palette rows.

## State model

Use SQLite at:

```text
~/.local/state/tmx/state.sqlite3
```

Use TOML config at:

```text
~/.config/tmx/config.toml
```

Use layout scripts at:

```text
~/.config/tmx/layouts/*.sh
<repo>/.tmx/layouts/*.sh       trusted only after confirmation
```

## SQLite tables

Initial schema:

```sql
CREATE TABLE meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE notes (
  scope TEXT NOT NULL,              -- session | window | pane | project
  stable_key TEXT NOT NULL,
  note TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (scope, stable_key)
);

CREATE TABLE live_targets (
  target_kind TEXT NOT NULL,         -- session | window | pane
  tmux_id TEXT NOT NULL,
  stable_key TEXT NOT NULL,
  name TEXT,
  cwd TEXT,
  session_id TEXT,
  window_id TEXT,
  pane_id TEXT,
  last_seen_at INTEGER NOT NULL,
  PRIMARY KEY (target_kind, tmux_id)
);

CREATE TABLE mru_targets (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  target_kind TEXT NOT NULL,
  stable_key TEXT NOT NULL,
  tmux_target TEXT,
  client_tty TEXT,
  visited_at INTEGER NOT NULL
);

CREATE TABLE projects (
  path TEXT PRIMARY KEY,
  repo_root TEXT,
  name TEXT NOT NULL,
  source TEXT NOT NULL,              -- tmux | recent | fd | zoxide | manual
  last_used_at INTEGER,
  frecency_score REAL DEFAULT 0
);

CREATE TABLE layouts (
  name TEXT PRIMARY KEY,
  path TEXT NOT NULL,
  source TEXT NOT NULL,              -- global | project
  trusted INTEGER NOT NULL DEFAULT 0,
  content_hash TEXT,
  last_used_at INTEGER
);
```

## Stable keys

Do not use tmux IDs as the only durable identity, because tmux IDs are server-lifetime identifiers.

Recommended stable keys:

```text
session: repo_root or cwd if available; else session name
window:  session stable key + window name
pane:    session stable key + window name + pane title + cwd + command fingerprint
project: canonical path
```

For live actions, always execute against tmux IDs.

For durable notes, reattach notes by stable key when possible.

## tmux user options

Mirror useful live metadata into tmux user options:

Session-scope:

```text
@tmx.cwd
@tmx.repo
@tmx.label
@tmx.note
@tmx.last_note_at
@tmx.layout
```

Window-scope:

```text
@tmx.note
@tmx.role
```

Pane-scope:

```text
@tmx.note
@tmx.role
@tmx.status
```

Rules:

- SQLite is the durable store.
- tmux user options are the live store.
- On each palette launch, reconcile tmux options and SQLite notes.
- If both changed, prefer the most recent timestamp.

## Selector abstraction

Define a `Selector` trait:

```rust
trait Selector {
    fn select(&self, rows: &[PaletteRow], profile: UiProfile) -> Result<Option<Selection>>;
}
```

Backends:

1. `fzf` — default, required in core profile.
2. `tmux_choose_tree` — no-fzf fallback for session/window/pane only.
3. `television` — optional desktop experiment.
4. `skim` — optional alternative if useful, not required.

## Command execution model

All user-visible actions should resolve to tmux commands or state updates.

Examples:

```text
switch pane      -> tmux switch-client -t %pane_id
switch session   -> tmux switch-client -t $session_id
rename session   -> tmux rename-session -t $session_id <new_name>
rename window    -> tmux rename-window -t @window_id <new_name>
rename pane      -> tmux select-pane -t %pane_id -T <title>
new here         -> tmux new-session -d -s <name> -c <cwd>
```

## Inside-tmux versus outside-tmux

Inside tmux:

- use `display-popup` for palette;
- switch with `switch-client`;
- use the origin pane env var to identify the real current pane, not the popup pane.

Outside tmux:

- run `fzf` full-screen;
- list existing sessions from default tmux server;
- `tmux attach-session -t <target>` after selection;
- `tmux new-session -s <name> -c "$PWD"` for new session.

## Origin pane handling

When launching a popup, pass the origin pane and current path explicitly:

```tmux
bind-key T display-popup -w 90% -h 80% \
  -d "#{pane_current_path}" \
  -e TMX_ORIGIN_PANE="#{pane_id}" \
  -e TMX_ORIGIN_CWD="#{pane_current_path}" \
  -E 'tmx palette'
```

Then `tmx` can determine “current directory” using:

1. `TMX_ORIGIN_CWD` if set and valid;
2. `tmux display-message -p -t "$TMX_ORIGIN_PANE" '#{pane_current_path}'`;
3. process `$PWD` as fallback.

This prevents the popup process or shell integration from accidentally defaulting to `$HOME`.
