# Performance and Accelerators

## Core conclusion

Use `fzf` as the default selector UI, but do not confuse selector performance with project/code search performance.

For tmux target switching, the candidate set is usually small: tens or hundreds of sessions/windows/panes. `fzf` is more than fast enough, mature, scriptable, and proven for terminal UI workflows.

For repeated file/content search inside large repositories, FFF is the more relevant performance tool. FFF is a file search library/SDK, not a drop-in replacement for the fzf interactive selector.

## Tool roles

| Tool | Role in `tmx` | Core? | Notes |
|---|---:|---:|---|
| tmux | source of truth and execution layer | yes | owns sessions/windows/panes |
| fzf | default selector UI | yes | best starting UI for popup/mobile workflows |
| git | repo-root and branch detection | strongly preferred | use CLI initially |
| fd | fast project/directory discovery | optional Phase 2 | installed in full performance profile |
| ripgrep | explicit content/history grep fallback | optional Phase 2 | do not use for every palette launch |
| zoxide | frecency-ranked directories/projects | optional Phase 2 | useful for project rows |
| FFF | high-performance repeated file/content search | optional Phase 3 | use in-process Rust crate if possible |
| fff-mcp | agent-facing search server | optional agent workflow only | not core because no-daemon root constraint |
| Television (`tv`) | alternative selector backend | optional experiment | desktop-first, not mobile-first until tested |
| Atuin | optional shell-history context | future | useful context, not required |
| skim | possible fzf alternative | not planned | keep as fallback research, not initial dependency |

## Why fzf stays in the core

`fzf` is a general-purpose command-line fuzzy finder and terminal toolkit. It is distributed as a single binary, supports custom menus/workflows, shell integrations, and is fast enough to process very large item lists. It also has mature behavior inside tmux popups and ordinary SSH terminals.

For `tmx`, fzf is the right default because:

- the UI protocol is simple: stdin rows, stdout selected row;
- it works in a tmux popup;
- it works over SSH;
- it supports preview for desktop mode;
- it can be configured down for mobile mode;
- it does not require a daemon;
- it can be replaced behind a selector interface later.

## What FFF changes

FFF should influence the implementation language and later project search design, but it should not replace fzf for target selection.

FFF is relevant because it provides:

- typo-resistant path and content search;
- frecency-ranked file access;
- an in-memory content index;
- background watching;
- Rust/C/Node/Bun/Python bindings;
- much faster repeated search in a long-running process or library-embedded process.

Important distinction:

```text
fzf: interactive selector for arbitrary rows
FFF: file/content search SDK with ranking and indexing
```

The core `tmx` palette searches live tmux metadata. It does not need an indexed file-search engine to list 100 tmux rows.

Use FFF later for:

- “open project file” rows;
- explicit project content search;
- “find task/source file in this repo” workflows;
- agent-facing search hooks;
- optional per-project file metadata enrichment.

Do not use FFF for:

- switching to tmux sessions/windows/panes;
- basic `tmx ls`;
- mobile mode core behavior;
- any feature that would require a mandatory background server.

## Rust recommendation due to FFF

If FFF becomes a serious Phase 3 accelerator, Rust is the best implementation language for `tmx` because FFF exposes native Rust crates. Using the Rust crate avoids shelling out to a separate CLI and aligns with the no-daemon preference better than depending on an MCP server.

The Rust CLI can compile with optional features:

```toml
[features]
default = []
fff = ["fff-search"]
tv = []
mac-notify = []
```

When compiled without `fff`, the tool still works with fzf/fd/rg.

## fd and ripgrep strategy

Use `fd` for project discovery:

```sh
fd -t d -H -E .git -E node_modules -E target . ~/src ~/work
```

Use `ripgrep` for explicit content search:

```sh
rg --hidden --glob '!.git' --glob '!node_modules' 'auth redirect'
```

Do not run `rg` or deep `fd` scans on every palette launch. Cache project candidates in SQLite and refresh opportunistically or explicitly:

```sh
tmx projects refresh
tmx projects refresh --root ~/src
```

## zoxide strategy

zoxide is useful for project rows because it already tracks directories used frequently and recently.

Use it as an optional source:

```sh
zoxide query -l
```

Project row sources should be merged:

```text
active tmux sessions
recent tmx projects
zoxide directories
configured roots scanned with fd
manual projects
```

## Television strategy

Television (`tv`) is promising as a fast, hackable fuzzy finder with custom channels. It should be implemented as an optional selector backend after MVP.

Recommendation:

- keep fzf default;
- add `selector_backend = "tv"` only after testing inside tmux popups and Termius-sized clients;
- do not make TV required for mobile mode until the key behavior is verified.

## Atuin strategy

Atuin stores shell history in SQLite with contextual metadata such as cwd, hostname, session, duration, and exit code. This could enrich a later “resume what I was doing” command, but it should not be a dependency.

Possible future use:

```text
tmx resume project    show recent commands for current project
tmx context           summarize recent cwd/session commands locally
tmx command-history   filter shell history by tmux session/project
```

Do not use Atuin in MVP.

## macOS accelerators

Optional macOS-only features:

```text
terminal-notifier or osascript for notifications
reattach-to-user-namespace only if needed for clipboard workflows
Homebrew dependency installer
```

Rules:

- macOS accelerators must be optional;
- the tool must still work on Linux remote hosts;
- mobile SSH uses tools installed on the remote Mac/Linux host, not on the phone.

## Performance budget

Target budgets:

```text
Palette launch with <200 tmux targets: <100 ms before fzf opens
Palette launch with <1000 targets: <250 ms before fzf opens
Switch selected target: <50 ms after fzf exits
SQLite note/MRU update: <20 ms
Project scan refresh: async/manual or clearly indicated, not hidden on every launch
```

No hidden expensive scan should run on every palette invocation.
