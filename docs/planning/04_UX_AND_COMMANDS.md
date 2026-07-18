# UX and Commands

## Primary UX

One command opens the palette:

```sh
tmx
```

The palette contains rows for sessions, windows, panes, projects, layouts, and actions.

Example rows:

```text
SESSION  work-api                 ~/src/work-api        fixing auth redirect
WINDOW   work-api:server          npm run dev           backend logs
PANE     work-api:server.2        claude                checking failing tests
PROJECT  ~/src/new-tool           create or attach
LAYOUT   agent-trio               open layout here
ACTION   new-session-here         create named session from current directory
ACTION   rename-session           rename current session
ACTION   rename-window            rename current window
ACTION   rename-pane              rename current pane
ACTION   note-current             add/update current session note
ACTION   last-target              toggle previous target
```

The fzf prompt is labeled `Filter>`, and the header shows `Type to filter | Enter open | Esc close`. Typing searches the visible full kind label and row metadata: session/window/pane names, pane titles and commands, paths, and notes. With an empty query, rows keep their generated live-target grouping; once a query is entered, fzf ranks the remaining matches by fuzzy relevance.

Printable quick-jump keys are intentionally not reserved: letters and numbers must remain available for ordinary fuzzy queries, especially over SSH and mobile keyboards. Use the arrow keys or `Ctrl-j`/`Ctrl-k` to move and `Enter` to open the selected row.

## Command namespace

Recommended commands:

```text
tmx                         open palette
tmx palette                 open palette explicitly
tmx palette --mobile        open mobile palette
tmx ls                      list/search live tmux targets
tmx view [target]           switch/attach selected target
tmx new                     create-or-attach session for current directory
tmx new --name NAME         create-or-attach with explicit name
tmx new --label LABEL       create duplicate labeled session for same project
tmx last                    toggle previous exact target
tmx last-session            tmux native last session behavior
tmx recent                  choose from recent target stack
tmx note                    note current session
tmx note session            note current session
tmx note window             note current window
tmx note pane               note current pane
tmx rename                  choose what to rename
tmx rename session [NAME]   rename current/selected session
tmx rename window [NAME]    rename current/selected window
tmx rename pane [TITLE]     set current/selected pane title
tmx layout                  choose layout
tmx layout NAME             run layout
tmx projects                choose project
tmx grep QUERY              explicit pane/history/project grep
tmx doctor                  dependency and tmux capability check
tmx completions SHELL       print Bash, Zsh, or Fish completions to stdout
```

Do not emulate NTM beyond tiny optional aliases.

## Shell aliases

Optional aliases:

```sh
alias t='tmx'
alias tl='tmx ls'
alias tvw='tmx view'
alias tn='tmx new'
alias tt='tmx last'
alias tnote='tmx note'
```

Do not alias `ls` or `view` globally because that will be surprising.

## Desktop mode

Invocation:

```sh
tmx palette --desktop
```

Conflict-safe tmux binding:

```tmux
bind-key T switch-client -T tmx
bind-key -T tmx p display-popup -w 90% -h 80% \
  -d "#{pane_current_path}" \
  -e TMX_ORIGIN_PANE="#{pane_id}" \
  -e TMX_ORIGIN_CWD="#{pane_current_path}" \
  -E 'tmx palette --desktop'
```

Desktop fzf behavior in the current MVP:

```text
explicit full kind labels and rich one-line metadata
Filter> prompt and visible control header
fuzzy relevance ranking after query input
mouse uses fzf's desktop default (no explicit --mouse flag)
hidden machine target IDs
no pane preview yet
```

Current controls:

```text
Enter       switch/open selected row
Arrow keys  move selection
Ctrl-j/k    move selection
Esc/Ctrl-c  exit
```

Rename, note, new-session, and last-target operations are available as visible `ACTION` rows. Printable row shortcuts are not reserved because doing so would interfere with fuzzy query entry.

## Mobile mode

Invocation:

```sh
tmx palette --mobile
TMX_UI=mobile tmx
```

Conflict-safe mobile tmux binding (after the same `bind-key T switch-client -T tmx` entry binding shown above):

```tmux
bind-key -T tmx m display-popup -w 100% -h 95% \
  -d "#{pane_current_path}" \
  -e TMX_ORIGIN_PANE="#{pane_id}" \
  -e TMX_ORIGIN_CWD="#{pane_current_path}" \
  -e TMX_UI=mobile \
  -E 'tmx palette --mobile'
```

Mobile rules:

```text
no preview by default
one-line rows
explicit SESSION/WINDOW/PANE/ACTION labels
plain Enter selects
visible action rows instead of hidden keybindings
minimal columns
minimal punctuation
no required Ctrl/Alt/mouse
```

Mobile example rows:

```text
SESSION work-api ~/src/work-api auth redirect note
WINDOW work-api/server ~/src/work-api npm run dev
PANE work-api/logs.2 checking-tests claude ~/src/work-api failing tests
ACTION new here
ACTION note current
ACTION rename session
ACTION last
```

Mobile uses the same explicit kind labels as desktop so their meaning is never implicit. Rows remain one line and may be clipped visually by a narrow terminal, but all rendered metadata remains fuzzy-searchable. Mobile interaction should be “type a few letters, tap Enter.”

## Auto mode

`tmx` should choose UI profile as follows:

```text
1. --desktop / --mobile explicit flag
2. TMX_UI=desktop/mobile env var
3. config default_ui
4. tmux client dimensions
5. fallback to desktop
```

Recommended thresholds:

```text
mobile if client_width < 100
mobile if client_height < 35
```

Do not rely only on terminal brand or `$TERM_PROGRAM`.

## Prompt UX

Rename prompts:

```text
Rename session: <input>
Rename window:  <input>
Rename pane:    <input>
```

Note prompt:

```text
What were you working on? <input>
```

Mobile prompt behavior:

- only after explicit `tmx note`, `tmx rename`, or visible action row;
- no surprise prompt when the palette opens;
- no multi-field forms.

## Destructive actions

Destructive actions are allowed but not prominent.

Rules:

```text
Never make kill/detach the Enter action for normal rows.
Always require confirmation.
Mobile requires typed confirmation for kill-session.
Default palette may omit destructive rows unless config enables them.
```

Commands:

```sh
tmx kill pane --target %12
tmx kill window --target @4
tmx kill session --target $2
```

## Out-of-tmux behavior

Running `tmx` outside tmux should still be useful.

Expected behavior:

```text
tmx             full-screen fzf selector over sessions/projects
tmx ls          list default tmux server sessions
tmx new         tmux new-session -s <name> -c "$PWD"
tmx view        tmux attach-session -t <selected>
```

Outside tmux, `display-popup` is unavailable, so use full-screen fzf.
