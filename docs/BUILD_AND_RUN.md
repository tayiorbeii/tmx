# Building & Running `tmx`

`tmx` is a Rust CLI binary. This guide covers prerequisites, build options, configuration, tmux binding setup, and basic smoke testing.

## Prerequisites

```sh
rustc --version      # 1.70+ (run `rustup update` if too old)
cargo --version
tmux -V              # 3.2+ recommended (for display-popup)
fzf --version        # 0.30+; required for palette/recent selector
```

Optional tools (enrich project features):

```sh
git --version        # repo-root detection
fd --version         # faster project file search
rg --version         # faster grep integration
zoxide --version     # frecency-based directory jumping
```

---

## Build

From the repository root:

```sh
# Release build (optimised binary)
cargo build --release
```

The binary lands at `target/release/tmx`.

---

## Install

### Option A — Add to PATH manually

Add the build directory to your shell's `PATH`:

```sh
export PATH="$PWD/target/release:$PATH"
```

### Option B — Cargo install (recommended)

```sh
cargo install --path .
```

This copies the binary to `~/.cargo/bin/tmx`. Make sure `~/.cargo/bin` is on your `PATH`:

```sh
echo $PATH | grep cargo
```

If it's missing, add this line to `~/.zshrc` or `~/.bashrc`:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

---

## Verify

```sh
tmx --version   # should print "tmx 0.1.0"
tmx --help      # should show all subcommands
```

---

## Shell Completions

`tmx` generates completions directly from its command-line definition. Generation writes the script to stdout and does not edit shell configuration.

### Bash

Generate the script:

```sh
mkdir -p ~/.local/share/tmx/completions
tmx completions bash > ~/.local/share/tmx/completions/tmx.bash
```

Add this line to `~/.bashrc`:

```sh
source "$HOME/.local/share/tmx/completions/tmx.bash"
```

### Zsh

Generate the completion function:

```sh
mkdir -p ~/.zfunc
tmx completions zsh > ~/.zfunc/_tmx
```

Add these lines to `~/.zshrc` before any existing `compinit` call (do not add a second `compinit` if one is already present):

```zsh
fpath=("$HOME/.zfunc" $fpath)
autoload -Uz compinit
compinit
```

### Fish

Fish automatically loads completion files from its user completion directory:

```fish
mkdir -p ~/.config/fish/completions
tmx completions fish > ~/.config/fish/completions/tmx.fish
```

Restart the shell after setup. Regenerate the file after upgrading `tmx` so newly added commands and options appear. Bash, Zsh, and Fish are the supported shells; PowerShell and Elvish are intentionally omitted because the documented installation targets are macOS and Linux with Bash, Zsh, or Fish.

The generated scripts complete commands, flags, and fixed values such as the `session`, `window`, and `pane` scopes. They do not query live tmux sessions, windows, or panes; use `tmx`/`tmx palette` to select a live target interactively.

---

## Configuration

```sh
mkdir -p ~/.config/tmx
cp config.example.toml ~/.config/tmx/config.toml
```

The config file is optional — `tmx` runs fine with defaults — but it lets you tweak:

| Setting | Default | Notes |
|---|---|---|
| `default_ui` | `"auto"` | `"desktop"`, `"mobile"`, or `"auto"` |
| `mobile_width_threshold` | `100` | Columns below this → mobile |
| `mobile_height_threshold` | `35` | Rows below this → mobile |
| `selector_backend` | `"fzf"` | Only `"fzf"` in MVP |
| `project_roots` | `["~/src", "~/work", "~/Code"]` | Where to search for projects |

### State database

SQLite state file created automatically at:

```
~/.local/state/tmx/state.sqlite3
```

Contains notes and MRU history. Safe to delete to reset all tmx state.

---

## tmux Bindings

Many tmux configurations already bind single keys such as `C` (customize mode), `L` (previous client), `M` (pane marking), or `R` (plugins such as tmux-resurrect), so binding tmx directly to a bare key can clobber existing behavior.

Use the conflict-safe `tmx` key table from `tmux.example.conf` instead:

```tmux
# Enter the tmx key table with prefix + T (T is unbound in default tmux).
bind-key T switch-client -T tmx

# prefix + T, then p: desktop palette
bind-key -T tmx p display-popup -w 90% -h 80% \
  -d "#{pane_current_path}" \
  -e TMX_ORIGIN_PANE="#{pane_id}" \
  -e TMX_ORIGIN_CWD="#{pane_current_path}" \
  -E 'tmx palette --desktop'

# prefix + T, then m: mobile palette
bind-key -T tmx m display-popup -w 100% -h 95% \
  -d "#{pane_current_path}" \
  -e TMX_ORIGIN_PANE="#{pane_id}" \
  -e TMX_ORIGIN_CWD="#{pane_current_path}" \
  -e TMX_UI=mobile \
  -E 'tmx palette --mobile'

# prefix + T, then l: previous tmx MRU target
bind-key -T tmx l run-shell 'tmx last'

# prefix + T, then n: session note
bind-key -T tmx n command-prompt -p 'What were you working on?' \
  'run-shell "tmx note session --set %%"'

# prefix + T, then r: rename menu
bind-key -T tmx r display-menu -T 'Rename' \
  'Session' s 'command-prompt -p "session name" "rename-session %%"' \
  'Window'  w 'command-prompt -p "window name"  "rename-window %%"' \
  'Pane'    p 'command-prompt -p "pane title"   "select-pane -T %%"'

# prefix + T, then c: create-or-attach in the current pane directory
bind-key -T tmx c run-shell \
  'TMX_ORIGIN_PANE="#{pane_id}" TMX_ORIGIN_CWD="#{pane_current_path}" tmx new'
```

These are three sequential keystrokes, not one chord. For example, with a `C-a` prefix:

1. Hold `Ctrl` and press `a`, then release both.
2. Press uppercase `T` (`Shift-t`), then release it. This enters the one-key `tmx` table.
3. Press lowercase `p` for desktop or lowercase `m` for mobile.

The sequence works only after the block above (or `tmux.example.conf`) has been copied into `~/.tmux.conf` and sourced. Verify installation with `tmux list-keys -T tmx`; `table tmx doesn't exist` means the bindings have not been loaded.

Reload after editing:

```sh
tmux source-file ~/.tmux.conf
```

Or from inside tmux: `prefix` + `:` then `source-file ~/.tmux.conf`.

### Environment variables explained

| Variable | Purpose |
|---|---|
| `TMX_ORIGIN_PANE` | Originating pane ID (set by `display-popup`) |
| `TMX_ORIGIN_CWD` | Originating pane working directory |
| `TMX_UI=mobile` | Force mobile UI profile |

---

## Smoke Tests

Run these inside a tmux session for best results.

### `tmx doctor` — health check

```sh
tmx doctor
```

Reports binary availability, tmux version, config/state paths, and origin cwd. Fix any `[FAIL]` before proceeding.

### `tmx ls` — list live targets

```sh
tmx ls
```

Shows sessions, windows, panes from the running tmux server.

### `tmx` / `tmx palette` — navigation palette

```sh
tmx
tmx palette
tmx palette --desktop
tmx palette --mobile
```

Opens fzf with live tmux targets. The prompt reads `Filter>` and rows use explicit `SESSION`, `WINDOW`, `PANE`, and `ACTION` labels. Type to fuzzy-filter and relevance-rank by kind, names, pane titles/commands, paths, or notes; `Enter` selects (switches/attaches), and `Esc`/`Ctrl-c` cancels.

### `tmx new` — create-or-attach session

```sh
tmx new                        # named after current directory
tmx new --name my-project      # explicit name
tmx new --name api --label v2  # labeled duplicate
```

Creates a new tmux session in the origin/current directory, or attaches if a session with that name already exists.

### `tmx view` — switch to a target

```sh
tmx view $0                    # session by tmux ID
tmx view @3                    # window by tmux ID
tmx view mysession             # session by name
tmx view                       # opens palette (no argument)
```

### `tmx last` — jump to previous target

```sh
tmx last
```

Switches to the most recent non-current target from MRU history.

### `tmx recent` — pick from MRU

```sh
tmx recent
```

Opens fzf selector over recent targets.

### `tmx note` — read/write notes

```sh
tmx note session --set "debugging auth redirect"
tmx note window  --set "running dev server"
tmx note pane    --set "tailing logs"
tmx note session              # reads and prints note
tmx note window
tmx note pane
```

Notes stored in SQLite + backed up to scoped tmux options (`@tmx.note`).

### `tmx rename` — rename current scope

```sh
tmx rename session "production"
tmx rename window "monitor"
tmx rename pane "build-logs"
tmx rename session             # prompts for name on stdin
```

---

## Inside vs. Outside tmux

| Command | Inside tmux | Outside tmux |
|---|---|---|
| `tmx` / `tmx palette` | Opens fzf in popup/full-screen | Full-screen fzf; selection attaches |
| `tmx ls` | Lists all live targets | Lists sessions from default server |
| `tmx new` | Creates/attaches in origin cwd | Creates/attaches in $PWD |
| `tmx last` | Switches to previous MRU entry | Attaches if valid target |
| `tmx note` | Reads/writes for current scope | Error — no active tmux target |
| `tmx rename` | Renames current scope | Error — no active tmux target |
| `tmx view` | Switches to target | Attaches to target session |
| `tmx doctor` | Full report | Works; warns "inside tmux: false" |
| `tmx recent` | MRU fzf selector | MRU fzf selector (limited) |

---

## Developer Validation

```sh
cargo fmt                           # formatting check
cargo test                          # unit + isolated tmux integration tests
cargo clippy --all-targets -- -D warnings   # lint (must be clean)
```

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| `tmx: command not found` | Add `~/.cargo/bin` to `PATH`, or use `./target/release/tmx` |
| Palette fails / fzf not found | Install fzf: `brew install fzf` (macOS) or `apt install fzf` |
| Typing does not filter | Check `fzf --version` and temporarily test with `env -u FZF_DEFAULT_OPTS -u FZF_DEFAULT_OPTS_FILE tmx palette --desktop`; custom fzf defaults such as disabled search can change interaction |
| `prefix T p` does nothing | Load the example bindings, run `tmux source-file ~/.tmux.conf`, then confirm `tmux list-keys -T tmx` lists `p` |
| `tmx` commands fail outside tmux | Some commands require an active tmux session |
| `tmx new` uses wrong directory | Ensure `TMX_ORIGIN_CWD` is set in the tmux binding, or run from the right directory |
| `tmx last` says "no previous target" | MRU is empty — switch between a few targets first |
| Corrupted state | Delete `~/.local/state/tmx/state.sqlite3` and `tmx doctor` to confirm |
| `display-popup` not available | Requires tmux 3.2+; run `tmx` directly at `prefix`+`:` prompt instead |

---

## SSH / Mobile

`tmx` works over plain SSH — no daemon, no ports, no web server.

```sh
# Force mobile UI profile
tmx palette --mobile
TMX_UI=mobile tmx palette
```

The mobile binding from `tmux.example.conf` (`prefix`, then uppercase `T`, then lowercase `m`) wraps this automatically with a near-full-screen popup and the `TMX_UI=mobile` environment variable. This is the recommended way to test from Termius, Blink, or any SSH client on iPhone.
