# Test Plan

## Unit tests

### Naming

Test:

```text
repo basename to session name
spaces/punctuation handling
empty names
collision suffixes
label suffixes
case normalization if enabled
```

### Stable keys

Test:

```text
session key from repo/cwd
window key from session + name
pane key from session + window + title/cwd/command fingerprint
project key from canonical path
```

### Config

Test:

```text
missing config uses defaults
TOML parsing
invalid config error messages
profile selection precedence
threshold-based mobile detection
```

### Notes

Test:

```text
set note by scope
update note timestamp
merge tmux option note with SQLite note
delete note
render note excerpt
strip newlines for palette rows
```

### Reminder policy

Test:

```text
prompt after active duration threshold
no prompt inside cooldown
no surprise prompt in mobile explicit mode
prompt if configured same-as-desktop
skip/dismiss cooldown
```

### Row rendering

Test:

```text
escape delimiters
hide row IDs from display
render full kind labels and searchable metadata
keep `--with-nth 3` free of an incompatible `--nth`
filter by session/window/pane names, pane title/command, paths, notes, and kinds
preserve initial grouping and use fuzzy relevance after input
mobile one-line row constraints
```

### CLI completions

Test:

```text
parse each supported completion shell
reject unsupported shell values
write nonempty scripts to stdout without loading config or contacting tmux
include representative commands, options, and scope values in every generated script
source or inspect each script in an isolated Bash, Zsh, or Fish process
```

## Integration tests with tmux

Use isolated tmux socket:

```sh
TMUX_TMPDIR=$(mktemp -d)
tmux -L tmx-test -f /dev/null new-session -d -s test -c /tmp
```

Test cases:

```text
list sessions/windows/panes
create new session with -c cwd
set/get @tmx.* options
rename session/window/pane
switch-client target command generation
capture-pane preview command does not persist data
last/recent behavior after target deletion
```

Where actual `switch-client` requires attached client, use command-generation tests plus a small attached-client smoke test.

## Manual desktop tests

Run in a normal desktop terminal:

```text
open palette from tmux keybinding
select session
select window
select pane
create session here from nested repo dir
rename current session
rename current window
rename current pane
add session note
search by note
toggle last target
view recent stack
```

## Manual mobile tests

Simulate first with small terminal dimensions, then verify from Termius on iPhone.

Small-terminal simulation:

```sh
resize terminal to ~80x24
TMX_UI=mobile tmx
```

Mobile acceptance:

```text
popup fits screen
no preview by default
rows are readable
typing filters rows
Enter selects
visible action rows work
rename prompt works
note prompt works
no required Ctrl/Alt/mouse
no accidental prompt spam
```

## Performance tests

Synthetic tmux data tests:

```text
50 targets
200 targets
1000 targets
5000 generated rows without tmux
```

Budget:

```text
<100 ms to generate rows for normal use
<250 ms for 1000 live targets
no project-root scan on palette launch
SQLite update <20 ms in normal case
```

## Dependency tests

`tmx doctor` should report:

```text
tmux present/version
fzf present/version
git present/version
fd present or absent
rg present or absent
zoxide present or absent
tv present or absent
fff feature compiled/enabled or absent
inside tmux yes/no
popup capability likely yes/no
current origin pane/cwd resolution
SQLite path writable
config path readable
```

## Regression tests for no-NTM behavior

Ensure:

```text
tmx new never scaffolds directories
tmx new never changes to home unless cwd is home
tmx does not require NTM
tmx does not read NTM state
tmx does not emulate NTM spawn/quick/agent commands
tmx does not create a daemon or server process
```

## Safety tests

Destructive action tests:

```text
kill-session requires confirmation
mobile kill-session requires typed confirmation
normal Enter never kills
missing target refreshes/fails safely
project-local layout requires trust
layout hash change requires re-trust
```

## Privacy tests

Ensure:

```text
pane preview output is not written to SQLite
explicit grep output is not persisted unless user saves note manually
state export does not include pane scrollback
logs redact command output by default
```
