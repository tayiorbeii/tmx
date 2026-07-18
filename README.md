# tmx

A lightweight tmux-native workflow layer: fast fuzzy switching across sessions, windows, and panes, current-directory session creation, scoped notes, MRU history, and renaming — all through a single fzf-powered palette that works on desktop and over SSH from a phone.

`tmx` is a single Rust binary. No daemon, no ports, no background server.

## Features

- **Palette** — `tmx` opens an fzf palette over every live session, window, and pane, plus quick actions.
- **Create-or-attach** — `tmx new` creates (or attaches to) a session named after the current directory.
- **Exact target switching** — `tmx view <target>` accepts session names or tmux IDs (`$session`, `@window`, `%pane`).
- **MRU history** — `tmx last` toggles to the previous target; `tmx recent` picks from the recent stack.
- **Scoped notes** — `tmx note [session|window|pane] --set "..."` records what you were doing.
- **Rename** — `tmx rename [session|window|pane] [NAME]`.
- **Mobile profile** — compact UI for small terminals (Termius, Blink, any SSH client on a phone), selected automatically or forced with `--mobile`.
- **Doctor** — `tmx doctor` checks dependencies, paths, and tmux capabilities.
- **Shell completions** — `tmx completions bash|zsh|fish`, generated from the CLI definition.

## Requirements

- Rust 1.70+ (build)
- tmux 3.2+ recommended (for `display-popup`)
- fzf 0.30+ (palette/recent selector)
- Optional: git, fd, rg, zoxide

## Install

```sh
cargo install --path .
```

The binary lands in `~/.cargo/bin/tmx`. See [docs/BUILD_AND_RUN.md](docs/BUILD_AND_RUN.md) for full build, configuration, tmux binding, and troubleshooting details.

## Quick start

```sh
tmx doctor      # verify dependencies
tmx             # open the palette
tmx new         # create-or-attach a session for the current directory
tmx ls          # list live targets
```

Copy the conflict-safe key bindings from [`tmux.example.conf`](tmux.example.conf) into `~/.tmux.conf` to open the palette with `prefix` + `T` + `p` (desktop) or `prefix` + `T` + `m` (mobile).

## Working with tmx

### Switching between sessions, windows, and panes

The fastest way to move around is the palette — it lists every live session, window, and pane in one fuzzy-searchable list:

```sh
tmx             # or: tmx palette
```

Type to filter by name, pane title/command, path, or note text, then `Enter` to switch/attach. `Esc` or `Ctrl-c` cancels without changing anything.

Other ways to switch:

```sh
tmx view mysession   # switch to a session by name (substring match if no exact match)
tmx view $0           # switch to an exact tmux target ID: $session, @window, or %pane
tmx view              # no argument -> opens the palette

tmx last              # toggle back to the previous exact target you were on
tmx recent            # fzf picker over your recent target history (MRU)
```

`tmx last` and `tmx recent` are backed by tmx's own MRU history (in SQLite), separate from tmux's built-in last-session tracking, so they follow session/window/pane switches made through tmx.

If you copied the example tmux bindings, `prefix` + `T` then `p` (desktop) or `m` (mobile) opens the palette in a popup from inside any pane, and `prefix` + `T` then `l` jumps to the last target — no need to leave your current pane first.

### Leaving notes on sessions, windows, and panes

Notes are short scoped strings — handy for reminding yourself (or a teammate) what a session, window, or pane is for. They show up in the palette rows and persist across restarts.

```sh
tmx note session --set "fixing auth redirect"   # note on the current session
tmx note window  --set "running dev server"      # note on the current window
tmx note pane    --set "tailing prod logs"        # note on the current pane

tmx note session     # print the current session's note (omit --set to read)
tmx note window
tmx note pane
```

Notes are keyed to the *current* target based on where you run the command from — there's no separate "target" argument, so run `tmx note ...` from inside the pane/window/session you want to annotate (or use the `prefix T n` binding below, which prompts inline without leaving the pane). Notes are stored in tmx's SQLite state and mirrored to a tmux pane/window/session option (`@tmx.note`) so they survive even if the state file is reset.

### Renaming sessions, windows, and panes

```sh
tmx rename session "production"   # rename the current session
tmx rename window "monitor"        # rename the current window
tmx rename pane "build-logs"       # set the current pane's title

tmx rename session                # omit the name to be prompted on stdin
```

### Creating sessions for a project

```sh
tmx new                           # create-or-attach, named after the current directory
tmx new --name api                # explicit session name
tmx new --name api --label v2     # a second, labeled session for the same project
```

### Recommended tmux key bindings

Copying [`tmux.example.conf`](tmux.example.conf) into `~/.tmux.conf` gives you a dedicated `tmx` key table (entered with `prefix` + `T`) so none of these actions require leaving your current pane or memorizing the full command:

| Keys (after `prefix` + `T`) | Action |
|---|---|
| `p` | Open the desktop palette in a popup |
| `m` | Open the mobile-friendly palette in a popup |
| `l` | Jump to the previous (`tmx last`) target |
| `n` | Prompt for a note on the current session |
| `r` | Menu to rename the session, window, or pane |
| `c` | Create-or-attach a session for the current pane's directory |

See [docs/BUILD_AND_RUN.md](docs/BUILD_AND_RUN.md#tmux-bindings) for the full walkthrough, including how the key sequence works and how to verify it loaded.

## Shell completions

Completions are generated from the CLI definition and written to stdout — `tmx` never edits your shell configuration. Bash, Zsh, and Fish are supported.

```sh
# Bash
mkdir -p ~/.local/share/tmx/completions
tmx completions bash > ~/.local/share/tmx/completions/tmx.bash
# then add to ~/.bashrc:  source "$HOME/.local/share/tmx/completions/tmx.bash"

# Zsh
mkdir -p ~/.zfunc
tmx completions zsh > ~/.zfunc/_tmx
# then ensure in ~/.zshrc (before compinit):  fpath=("$HOME/.zfunc" $fpath)

# Fish
mkdir -p ~/.config/fish/completions
tmx completions fish > ~/.config/fish/completions/tmx.fish
```

See [docs/BUILD_AND_RUN.md](docs/BUILD_AND_RUN.md#shell-completions) for details.

## Configuration

Optional TOML config at `~/.config/tmx/config.toml` (see [`config.example.toml`](config.example.toml)). State (notes, MRU) lives in SQLite at `~/.local/state/tmx/state.sqlite3` and is safe to delete.

## Documentation

- [docs/BUILD_AND_RUN.md](docs/BUILD_AND_RUN.md) — build, install, configure, tmux bindings, smoke tests, troubleshooting.
- [docs/planning/](docs/planning/00_INDEX.md) — the original design and planning pack (architecture, UX, phases, test plan).

## Development

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features
```

Integration tests use isolated tmux sockets (`tmux -L tmx-test-…`) and never touch your live tmux sessions.

## License

MIT — see [LICENSE](LICENSE).
