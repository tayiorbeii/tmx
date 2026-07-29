# Shared Understanding

## Agreed root constraint

The tool is a tmux-native workflow layer with no required daemon, no GUI, no web dashboard, no browser surface, no Electron/native wrapper, and no always-running controller.

It should run inside a normal terminal over normal SSH, including Termius on iPhone.

## Explicitly not a full NTM clone

Do not emulate all NTM commands. The useful slice is:

- list and switch between tmux sessions/windows/panes;
- create sessions in the current directory;
- rename sessions/windows/panes;
- remember what each session/window/pane is for;
- toggle back to the last active context;
- offer named layouts;
- occasionally prompt for a note/reminder.

NTM’s broader agent orchestration, work graph, safety policy, mail, durable checkpoints, REST/SSE/WebSocket surfaces, and dashboard ideas are out of scope.

## User-specific constraints captured

- There is no strong NTM muscle memory to preserve.
- The prior habit was essentially “list sessions, then view/attach/switch.”
- New sessions must not scaffold a new project directory, create a new home directory, or behave like `ntm quick/spawn`.
- A new session should behave like running raw tmux from the current terminal directory.
- Renaming sessions/windows/panes is important.
- Prompting for notes “once in a while” is important, but it must not become nagging.
- Mobile friendliness in Termius on iPhone is a hard requirement, not a fallback.
- Performance accelerators are welcome, especially on macOS, as long as the core remains terminal-native and SSH-friendly.

## Working command namespace

Use `tmx` as the working CLI name in the spec.

Because there is no NTM muscle memory, do not install NTM-compatible aliases by default. Optional aliases can be provided for convenience, but they should be deliberately small:

```sh
alias tl='tmx ls'
alias tvw='tmx view'
alias tn='tmx new'
alias tt='tmx last'
alias tnote='tmx note'
```

Do not alias `ls` or `view` globally.

## Implementation language recommendation

Use Rust for the main CLI.

Original recommendation was Go because a small Go CLI is simple and NTM is Go. After adding the performance requirement and FFF research, Rust is the better recommendation because:

- the fast file/content search option FFF exposes a native Rust crate;
- Rust produces self-contained static-ish executables suitable for macOS/Linux servers;
- the tool can still remain a command-line wrapper around tmux rather than a long-running TUI;
- Rust has strong libraries for CLI parsing, TOML config, SQLite, serialization, process execution, and testing.

The Rust CLI must still call tmux as the source of truth; it must not become its own multiplexer.

## Source of truth

Live tmux state is the source of truth.

The tool may keep metadata, but it must discover sessions, windows, and panes from tmux each time via `tmux list-sessions`, `tmux list-windows -a`, and `tmux list-panes -a`.

## Metadata strategy

Use both:

1. tmux user options for live per-session/window/pane metadata;
2. SQLite for durable local metadata, MRU history, notes, and project records.

Rationale: tmux options keep data close to live objects; SQLite prevents JSON corruption under concurrent hook invocations and makes MRU/note queries easy.

## Dependency policy

Core profile:

- `tmux`
- `fzf`
- `tmx` binary
- `git` strongly preferred for repo-root detection

Full performance profile:

- `fd` for project discovery
- `ripgrep` for explicit pane/content/history search fallback
- `zoxide` for directory frecency
- `atuin` only as optional future context enrichment
- `television` as an experimental selector backend
- `fff-search` crate for high-performance repeated project file/content search inside the Rust process
- `fff-mcp` only for agent-facing search workflows, not for the core switcher

## UI strategy

Primary interface: one unified command palette.

Desktop mode:

- tmux popup;
- fzf selector;
- richer columns;
- preview pane;
- extra keyboard shortcuts allowed.

Mobile mode:

- near-fullscreen popup or full-screen selector;
- no preview by default;
- one-line rows;
- Enter to select;
- visible action rows instead of hidden Ctrl/Alt bindings;
- no required mouse, Alt, or complex key chords.

## Session creation behavior

`tmx new` and `tmx new-here` must use the originating pane’s current directory, not `$HOME`, unless the user explicitly asks for another directory.

Inside tmux, use the originating pane’s `#{pane_current_path}`. Outside tmux, use the shell’s `$PWD`.

New session behavior:

1. determine current directory;
2. find git root if available;
3. derive a safe session name from repo basename or directory basename;
4. if an existing session is already associated with that directory/repo, switch to it;
5. otherwise run `tmux new-session -d -s <name> -c <cwd>`;
6. set `@tmx.cwd`, `@tmx.repo`, and related metadata;
7. switch/attach to the session.

## Notes and reminders

Support notes on sessions, windows, and panes. Make session notes the default UX.

Prompt rule:

- prompt when leaving a session after meaningful activity;
- rate-limit aggressively;
- suppress surprise prompts in mobile mode unless explicitly enabled.

Default recommendation:

- ask after leaving a session that was active for at least 20 minutes;
- do not ask if the session note was updated within the last 4 hours;
- in mobile mode, only ask after explicit `tmx note` or at most once per day/session.

## Privacy boundary

Do not persist pane contents.

The default index/search corpus is metadata only:

- session name;
- window name;
- pane title;
- pane command;
- pane current path;
- git repo and branch;
- notes;
- layout label;
- last activity timestamp.

Pane scrollback can be captured for a temporary preview or explicit search command, but must not be stored in SQLite by default.
