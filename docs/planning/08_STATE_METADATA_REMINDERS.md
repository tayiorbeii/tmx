# State, Metadata, Notes, and Reminders

## State principles

1. tmux is the source of truth for live sessions/windows/panes.
2. SQLite stores durable metadata and recent history.
3. tmux user options mirror live notes and labels.
4. Do not persist pane output by default.
5. Everything must survive a tmux server restart except ephemeral exact pane IDs.

## Note scopes

Support three live scopes plus project scope:

```text
session note: default; describes the overall task
window note:  describes a window role
a pane note:  describes a pane/agent/process
project note: durable note attached to a path/repo
```

Default command behavior:

```text
tmx note              current session
tmx note session      current session
tmx note window       current window
tmx note pane         current pane
```

## Note UX

Desktop palette:

```text
Ctrl-n note selected row
ACTION note-current
ACTION note-selected
```

Mobile palette:

```text
A note current
A note session
A note window
A note pane
```

No required Ctrl key in mobile mode.

## Note data flow

When setting a note:

```text
1. resolve target scope and live tmux ID
2. compute stable key
3. update SQLite notes table
4. set @tmx.note and @tmx.last_note_at on the live tmux object if possible
5. display confirmation
```

When building palette:

```text
1. query tmux @tmx.note options
2. query SQLite notes by stable key
3. choose newest note by timestamp
4. show compact note excerpt in row
5. include full note in search_blob
```

## Reminder model

No daemon means reminders should be context-triggered, not time-triggered.

Trigger opportunities:

```text
opening palette
switching away from a session
switching back to a session
attaching to tmux
running tmx recent/resume
```

Default reminder on return:

```text
Returning to work-api
Last note, 2h ago: fixing auth redirect; check failing callback test
```

## Note prompt policy

Default policy:

```text
prompt_notes = true
note_prompt_min_active_minutes = 20
note_prompt_cooldown_hours = 4
mobile_note_prompt_mode = "explicit"   # explicit | daily | same-as-desktop
```

Decision function:

```text
ShouldPrompt(session):
  if prompt_notes is false: no
  if UI profile is mobile and mobile mode is explicit: no
  if active duration < min_active: no
  if note updated within cooldown: no
  if prompt dismissed recently: no
  yes
```

## Mobile prompting

Mobile mode must never surprise the user with a large form.

Allowed:

```text
explicit ACTION note current
explicit tmx note
single-line prompt after selecting note action
optional once-per-day reminder message
```

Avoid:

```text
prompt on every switch
multi-field prompt
mandatory note before switching
```

## MRU target stack

Maintain a recent target stack with at least:

```text
target kind
tmux target id if still live
stable key
session/window/pane names
cwd
client tty
visited_at
```

Commands:

```sh
tmx last
tmx recent
tmx recent --mobile
```

Behavior:

- `tmx last` jumps to the most recent target that is not the current target and still exists.
- If exact pane is gone, try containing window/session if still meaningful.
- If no live target remains, show recent projects instead.

## Attention markers

Lightweight attention metadata can be set by:

```text
tmux activity/bell hooks
manual command: tmx attention set/clear
external agent hooks that only write @tmx.status/@tmx.note
```

Do not build agent orchestration. Treat agent state as optional metadata.

Possible values:

```text
idle
working
blocked
done
needs-review
unknown
```

Palette row example:

```text
P work-api:claude  needs-review  tests passed; wants review
```

## Privacy boundaries

Do not store:

```text
pane scrollback
captured command output
terminal transcripts
API keys or tokens
prompt contents unless user manually writes them as notes
```

Allowed to store:

```text
names
titles
commands
paths
repo/branch
manual notes
timestamps
layout labels
attention markers
```

Temporary preview may display recent pane output but must not write it to disk.

## Pruning

Default retention:

```text
mru targets: keep last 1000 visits, prune older than 180 days
closed live target records: prune after 30 days
project records: keep indefinitely unless path no longer exists for 180 days
notes: keep indefinitely unless explicitly deleted
attention markers: clear on session/window/pane close or after 7 days
```

Commands:

```sh
tmx state prune
tmx notes list
tmx notes delete
tmx state export
tmx state import
```

State export/import can be JSON even if the live store is SQLite.
