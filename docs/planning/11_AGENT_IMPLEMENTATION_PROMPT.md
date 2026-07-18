# Agent Implementation Prompt

Use this as the starting instruction for an implementation agent.

---

You are implementing `tmx`, a lightweight tmux-native workflow layer. It is not NTM, not Herdr, not cmux, and not a terminal multiplexer replacement.

## Hard constraints

- Do not create a daemon, background server, web UI, socket API, REST API, browser dashboard, or native app wrapper.
- tmux is the source of truth for live sessions, windows, and panes.
- The tool must work over plain SSH, including Termius on iPhone.
- The core must be terminal-native and driven by tmux commands.
- Do not persist pane scrollback or terminal output.
- New sessions must be created in the current/origin directory, never by scaffolding a new home/project directory.
- Do not emulate all NTM commands.
- Do not implement agent orchestration.

## Implementation language

Use Rust for the main CLI.

Recommended crates:

```toml
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
rusqlite = { version = "0.32", features = ["bundled"] }
anyhow = "1"
thiserror = "1"
which = "6"
shellexpand = "3"
dirs = "5"
chrono = { version = "0.4", features = ["serde"] }
tempfile = "3"
tracing = "0.1"
tracing-subscriber = "0.3"
```

Add FFF as an optional feature in a later phase, not in MVP.

## MVP command list

Implement:

```text
tmx
tmx palette [--desktop|--mobile]
tmx ls
tmx view [target]
tmx new [--name NAME] [--label LABEL]
tmx last
tmx recent
tmx note [session|window|pane] [--set TEXT]
tmx rename [session|window|pane] [NAME]
tmx doctor
```

Do not implement agent commands.

## tmux integration

Discover live targets using tmux:

```sh
tmux list-sessions -F ...
tmux list-windows -a -F ...
tmux list-panes -a -F ...
```

Use tmux IDs for live actions:

```text
session IDs: $...
window IDs:  @...
pane IDs:    %...
```

Switch with:

```sh
tmux switch-client -t <target>
```

Create with:

```sh
tmux new-session -d -s <name> -c <cwd>
```

Rename with:

```sh
tmux rename-session -t <session> <name>
tmux rename-window -t <window> <name>
tmux select-pane -t <pane> -T <title>
```

## Origin directory handling

The tmux binding must pass origin pane information:

```tmux
bind-key T display-popup -w 90% -h 80% \
  -d "#{pane_current_path}" \
  -e TMX_ORIGIN_PANE="#{pane_id}" \
  -e TMX_ORIGIN_CWD="#{pane_current_path}" \
  -E 'tmx palette'
```

`tmx new` must resolve cwd using:

1. `TMX_ORIGIN_CWD`;
2. `TMX_ORIGIN_PANE` + `tmux display-message -p -t ... '#{pane_current_path}'`;
3. `$PWD`.

## State

Use SQLite:

```text
~/.local/state/tmx/state.sqlite3
```

Use TOML config:

```text
~/.config/tmx/config.toml
```

Store notes, MRU targets, project records, and layout trust in SQLite.

Mirror live notes to tmux user options:

```text
@tmx.cwd
@tmx.repo
@tmx.label
@tmx.note
@tmx.last_note_at
@tmx.layout
```

## UI

Default selector: fzf.

Desktop:

```text
popup 90%x80%
preview enabled after MVP
multi-column rows
keyboard shortcuts allowed
```

Mobile:

```text
popup 100%x95%
no preview by default
one-line rows
visible action rows
Enter selects
no required Ctrl/Alt/mouse
```

## Row types

Palette rows should include:

```text
session
window
pane
project
action
layout
```

Use hidden row IDs to execute exact actions safely. Do not parse display text to determine targets.

## Testing

Use isolated tmux socket integration tests where possible:

```sh
tmux -L tmx-test -f /dev/null new-session -d -s test -c /tmp
```

Required tests:

```text
name generation
current cwd resolution
tmux format parsing
row rendering
fzf output parsing
SQLite migrations
notes
rename command generation
new-session command generation
MRU stack
mobile profile selection
```

## Phase discipline

MVP first. Do not add FFF, Television, agent hooks, notifications, project-local layouts, or deep grep until the MVP is complete.

When adding performance accelerators later:

- FFF is for repeated project file/content search, not tmux target switching.
- Television is an optional selector backend, not a core dependency.
- fd/zoxide improve project discovery, not live tmux target discovery.
