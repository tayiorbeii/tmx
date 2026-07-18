# Layouts and Projects

## Project model

A project is a directory that can become a tmux session.

Project sources:

```text
active tmux sessions with @tmx.cwd/@tmx.repo
recent directories from tmx SQLite
configured project roots scanned by fd/Rust walker
zoxide frecency list
manual project entries
```

## Current directory behavior

Inside tmux:

1. use `TMX_ORIGIN_CWD` from popup binding;
2. fallback to `tmux display-message -p -t "$TMX_ORIGIN_PANE" '#{pane_current_path}'`;
3. fallback to process `$PWD`.

Outside tmux:

1. use `$PWD`.

Then:

```text
repo_root = git rev-parse --show-toplevel if available
base_dir = repo_root else cwd
session_name = sanitized basename(base_dir)
```

## New session behavior

`tmx new` means create-or-attach for the current directory.

Algorithm:

```text
cwd = origin/current directory
repo = git root if available
key = canonical(repo or cwd)

if an active session has @tmx.repo == repo or @tmx.cwd == cwd:
    switch to it
else:
    name = derive_name(repo or cwd)
    name = avoid_collision(name)
    tmux new-session -d -s name -c cwd
    set @tmx.cwd cwd
    set @tmx.repo repo
    switch/attach
```

This intentionally behaves more like raw tmux than NTM.

## Duplicate sessions for same project

Allow duplicates only when explicitly labeled:

```sh
tmx new --label auth
tmx new --label frontend
tmx new --label review
```

Naming:

```text
work-api
work-api--auth
work-api--frontend
work-api--review
```

If a duplicate label already exists, switch to it unless `--force-new` is provided.

## Name generation

Rules:

```text
use git repo basename if in repo
else use cwd basename
replace spaces with '-'
strip/replace unsafe punctuation
collapse repeated '-'
trim leading/trailing '-'
avoid empty names
handle collisions with suffixes
```

Examples:

```text
~/src/Work API              -> work-api
~/src/work-api + label auth -> work-api--auth
~/Desktop/tmp               -> tmp
/                           -> root
empty/unsafe                -> session
collision                   -> work-api-2
```

Name override:

```sh
tmx new --name payments-backend
```

## Project rows

Selecting a project row means “go there,” not “always create another session.”

Behavior:

```text
if one session exists for project: switch to it
if multiple labeled sessions exist: show subchoice
if no session exists: create session in that directory
```

## Layout principles

Start simple: named shell recipes.

Global layout directory:

```text
~/.config/tmx/layouts/
```

Project-local layout directory:

```text
<repo>/.tmx/layouts/
```

Project-local scripts require trust confirmation before first run.

## Layout script contract

Layout scripts receive environment variables:

```text
TMX_SESSION_NAME
TMX_CWD
TMX_REPO
TMX_LABEL
TMX_LAYOUT_NAME
```

Layout scripts should print useful errors and should be idempotent when practical.

Example global layout:

```sh
#!/usr/bin/env bash
set -euo pipefail

name="${TMX_SESSION_NAME:?}"
cwd="${TMX_CWD:?}"

tmux new-session -d -s "$name" -c "$cwd"
tmux rename-window -t "$name:1" editor
tmux split-window -h -t "$name:1" -c "$cwd"
tmux split-window -v -t "$name:1.2" -c "$cwd"
tmux select-layout -t "$name:1" tiled
tmux select-pane -t "$name:1.1"
```

## Layout commands

```sh
tmx layout                  choose layout
tmx layout dev-server       run named layout in current cwd
tmx new --layout dev-server create new session using layout
tmx layout trust ./...       trust project-local layout after review
```

## Layout trust model

Global scripts are trusted because the user installed them.

Project-local scripts are untrusted until approved. Store approval by path and content hash:

```text
layout name
absolute path
source repo path
sha256 content hash
trusted_at
```

If content hash changes, require confirmation again.

## Declarative layouts later

Post-MVP, add TOML layouts for common cases:

```toml
name = "dev-server"

[[windows]]
name = "dev"
layout = "main-vertical"

[[windows.panes]]
name = "editor"
command = "$SHELL"

[[windows.panes]]
name = "server"
command = "npm run dev"

[[windows.panes]]
name = "tests"
command = "npm test -- --watch"
```

Do not start here. Shell recipes are more flexible and faster to implement.

## Project discovery refresh

Avoid scanning every palette launch.

Commands:

```sh
tmx projects refresh
tmx projects refresh --root ~/src
tmx projects add ~/src/work-api
tmx projects remove ~/src/old
```

Palette launch should merge cached project rows with live tmux rows.
