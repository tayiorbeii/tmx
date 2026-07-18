# tmux Implementation Notes

## tmux primitives to rely on

`tmx` should mostly be a friendly layer over these tmux commands:

```text
list-sessions
list-windows -a
list-panes -a
switch-client
attach-session
new-session -c
rename-session
rename-window
select-pane -T
display-popup
display-menu
command-prompt
choose-tree
set-option/show-option for @tmx.* user options
set-hook/show-hooks
capture-pane for temporary previews only
find-window for simple visible-content search fallback
```

## Discover live targets

Recommended tmux queries:

```sh
tmux list-sessions -F '<fields>'
tmux list-windows -a -F '<fields>'
tmux list-panes -a -F '<fields>'
```

Collect at least:

### Sessions

```text
session_id
session_name
session_created
session_activity
session_attached
session_windows
@tmx.cwd
@tmx.repo
@tmx.label
@tmx.note
@tmx.last_note_at
```

### Windows

```text
session_id
session_name
window_id
window_index
window_name
window_active
window_activity
window_panes
window_layout
@tmx.note
@tmx.role
```

### Panes

```text
session_id
session_name
window_id
window_index
window_name
pane_id
pane_index
pane_title
pane_current_command
pane_current_path
pane_active
pane_dead
pane_width
pane_height
pane_last
@tmx.note
@tmx.role
@tmx.status
```

## Field parsing

Avoid fragile whitespace parsing. Use a delimiter unlikely to appear in names/paths, such as ASCII unit separator `\x1f`, or use tmux quoting formats where available.

Pseudocode:

```text
format = "#{session_id}\x1f#{session_name}\x1f#{session_activity}\x1f#{@tmx.note}"
for line in output.lines():
    fields = line.split('\x1f')
```

Escape or replace newlines in notes before including them in palette rows.

## Switching

For exact pane selection:

```sh
tmux switch-client -t '%12'
```

For session selection:

```sh
tmux switch-client -t '$2'
```

For outside-tmux attach:

```sh
tmux attach-session -t '$2'
```

## Last target

There are two concepts:

1. tmux native last session: `tmux switch-client -l`
2. `tmx` exact last target: stored by `tmx` MRU stack

Implement both:

```sh
tmx last-session
tmx last
```

`tmx last` should prefer exact pane/window/session from the MRU stack. If the target no longer exists, skip it and try the next recent target.

## Creating sessions in the current directory

Inside tmux, never trust the popup process’s current directory as the source of truth. Use origin-pane metadata.

Recommended binding:

```tmux
bind-key T display-popup -w 90% -h 80% \
  -d "#{pane_current_path}" \
  -e TMX_ORIGIN_PANE="#{pane_id}" \
  -e TMX_ORIGIN_CWD="#{pane_current_path}" \
  -E 'tmx palette'
```

Creation command:

```sh
tmux new-session -d -s "$session_name" -c "$cwd"
```

Then set metadata:

```sh
tmux set-option -t "$session_id" @tmx.cwd "$cwd"
tmux set-option -t "$session_id" @tmx.repo "$repo"
tmux set-option -t "$session_id" @tmx.label "$label"
```

## Renaming

Use native tmux commands:

```sh
tmux rename-session -t '$session_id' 'new-name'
tmux rename-window  -t '@window_id'  'new-name'
tmux select-pane    -t '%pane_id' -T 'new title'
```

Note: pane title is not the same as window name. The pane title can be displayed in formats and searched by `tmx` metadata rows.

## Popup UI

Desktop:

```tmux
display-popup -w 90% -h 80% -E 'tmx palette --desktop'
```

Mobile:

```tmux
display-popup -w 100% -h 95% -E 'tmx palette --mobile'
```

Fallback if popups are unavailable or disabled:

```text
inside tmux: split-window or choose-tree fallback
outside tmux: full-screen fzf
```

## command-prompt use cases

Use `tmux command-prompt` for mobile-safe simple input:

```tmux
command-prompt -p 'session name' 'run-shell "tmx rename session -- %%"'
command-prompt -p 'window name'  'run-shell "tmx rename window -- %%"'
command-prompt -p 'pane title'   'run-shell "tmx rename pane -- %%"'
command-prompt -p 'note'         'run-shell "tmx note --set %%"'
```

When quoting is hard, prefer invoking `tmx prompt-*` subcommands that read stdin or use environment-safe arguments.

## choose-tree fallback

When fzf is unavailable:

```tmux
choose-tree -Zw
```

Use choose-tree for sessions/windows/panes only. It cannot replace the full unified palette with projects/layouts/actions, but it is a useful fallback.

## display-menu fallback

Use `display-menu` for small action menus:

```text
Rename session
Rename window
Rename pane
Note current session
New session here
Last target
```

## Hooks

Install light hooks only. Do not index pane content in hooks.

Useful hooks:

```text
client-session-changed
session-created
session-renamed
session-closed
window-renamed
pane-focus-in
pane-focus-out
alert-activity
alert-bell
```

Hook responsibilities:

```text
update MRU timestamp
record last active session/window/pane
set lightweight attention markers
maybe run note-prompt policy after switch
```

Hook non-responsibilities:

```text
no deep scanning
no FFF indexing
no content capture persistence
no blocking long-running commands
```

## Preview behavior

Desktop preview may transiently call:

```sh
tmux capture-pane -p -t '%pane_id' -S -120
```

Rules:

- preview is optional;
- preview is off in mobile mode by default;
- preview output is not written to SQLite;
- preview should truncate aggressively.

## Error handling

Common cases:

```text
No tmux server       -> offer to create new session
No sessions          -> offer new session here
Target disappeared   -> refresh palette and show message
fzf unavailable      -> choose-tree/display-menu fallback
Not inside tmux      -> attach/new behavior
Popup unsupported    -> full-screen selector or choose-tree
SQLite locked        -> short retry, then continue without metadata
```
