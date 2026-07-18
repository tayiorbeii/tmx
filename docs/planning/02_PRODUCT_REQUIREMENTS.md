# Product Requirements

## Product sentence

`tmx` is a fast, terminal-native command palette for tmux that lets the user search, switch, name, annotate, and resume work across sessions, windows, panes, projects, and layouts without leaving tmux or relying on a separate app.

## Primary jobs

### 1. Find what is already running

The user can open one palette and search across:

- sessions;
- windows;
- panes;
- project paths;
- notes;
- recent targets;
- layouts;
- actions.

Acceptance criteria:

- `tmx` opens a palette inside tmux.
- `tmx ls` lists live tmux sessions/windows/panes.
- Selecting a pane switches to the exact session, window, and pane.
- Rows remain useful when there are many open sessions.
- Results include enough context to identify tasks quickly.

### 2. Switch quickly and safely

The user can jump to any session/window/pane and toggle back to the previous working context.

Acceptance criteria:

- `tmx last` toggles to the previous exact target selected through `tmx`.
- `tmx last-session` maps to tmux’s native last-session behavior when possible.
- A recent-target stack is maintained.
- Switching by pane uses tmux’s stable pane target, not fuzzy names.

### 3. Create a session here

The user can create or attach to a project session from the current directory.

Acceptance criteria:

- `tmx new` from a terminal in `/path/to/repo` creates a tmux session rooted at `/path/to/repo` or its git root.
- It does not create a scaffold directory.
- It does not cd to `$HOME`.
- It does not clone NTM’s project/session creation behavior.
- If a session already exists for the same project path, selection switches to the existing session unless a new label is explicitly provided.

### 4. Rename what is ambiguous

The user can rename sessions, windows, and panes.

Acceptance criteria:

- `tmx rename session <name>` calls native tmux session rename.
- `tmx rename window <name>` calls native tmux window rename.
- `tmx rename pane <title>` calls native tmux pane title setting.
- Palette actions can rename the selected object.
- Mobile mode supports simple prompt-based rename.

### 5. Capture lightweight context

The user can add notes to a session/window/pane and later search those notes.

Acceptance criteria:

- `tmx note` updates the current session note by default.
- `tmx note pane` updates current pane note.
- `tmx note window` updates current window note.
- Notes appear in palette rows.
- Notes are searchable.
- Notes persist across tmux server restart via SQLite.
- Live notes are mirrored into tmux user options where possible.

### 6. Prompt occasionally

The tool can ask “What were you working on?” only at useful moments.

Acceptance criteria:

- Note prompts are rate-limited.
- Prompts occur on context switches, not on an always-running timer.
- Mobile mode suppresses surprise prompts unless explicitly configured.
- The prompt can be skipped quickly.
- The reminder system still works without a daemon.

### 7. Work well on iPhone over SSH

The mobile experience is deliberately designed for Termius/iPhone.

Acceptance criteria:

- `tmx --mobile` or `TMX_UI=mobile tmx` uses the mobile UI.
- Auto mode selects mobile UI when the client is small.
- No required Ctrl/Alt/mouse shortcuts.
- Rows are short and one-line.
- Preview is off by default.
- Enter selects.
- Actions are visible as rows.
- It works in a normal SSH session attached to tmux.

### 8. Layout shortcuts

The user can run named layouts.

Acceptance criteria:

- `tmx layout` lists layouts.
- `tmx layout <name>` opens a named tmux layout in the current directory.
- Layout recipes can be global.
- Project-local layout scripts require trust confirmation before first execution.
- Layouts can be attached to project/session metadata later.

## Non-goals

- no agent orchestration runtime;
- no dashboard server;
- no browser UI;
- no REST/SSE/WebSocket API;
- no persistent background process required for tmux switching;
- no full NTM compatibility layer;
- no automatic destructive actions;
- no persisted pane scrollback;
- no mandatory macOS-only dependency;
- no assumption that the client terminal is desktop-sized.

## MVP acceptance checklist

MVP is complete when the following works reliably:

```text
tmx                      opens palette
tmx --mobile             opens mobile palette
tmx ls                   lists sessions/windows/panes
tmx view <target>         switches/attaches to target
tmx new                  create-or-attach session for current directory
tmx last                 toggle previous exact target
tmx note                 set current session note
tmx rename session       rename current session
tmx rename window        rename current window
tmx rename pane          rename current pane title
```

MVP does not need deep project discovery, FFF integration, agent state detection, scheduled reminders, or project-local layout scripts.
