# Unified WezTerm and tmx destination switcher

The adapter in `wezterm/tmx_switcher/` presents native WezTerm and trusted local tmux destinations in one `InputSelector`. WezTerm owns native enumeration, labels, selector presentation, focus, and local tab spawning. `tmx` owns endpoint trust, bounded tmux inventory, stale revalidation, exact-client routing, and attachment.

## Compatibility

| Component | Supported floor | Release validation |
|---|---:|---|
| WezTerm | `20230408-112425-69ae8472` | Floor API fixtures plus installed/latest manual GUI row |
| tmux | 3.2 | CI builds 3.2 and 3.6a; local validation uses the installed version |
| Platforms | Local Unix-like macOS and Linux | macOS plus Linux CI; X11/Wayland limitations are recorded in the release checklist |

Windows/WSL, remote mux domains, SSH endpoint discovery, group-shared sockets, persistent restoration, previews, and project/session creation are not part of v1. Missing or incompatible tmux support disables only tmux augmentation; native rows remain available.

## Install

1. Install a release build of `tmx` and confirm its absolute path:

   ```sh
   cargo install --path .
   command -v tmx
   command -v tmx-supervisor
   ```

2. Copy the Lua module into the WezTerm configuration directory:

   ```sh
   mkdir -p ~/.config/wezterm/tmx_switcher
   cp wezterm/tmx_switcher/*.lua ~/.config/wezterm/tmx_switcher/
   ```

3. Add the adapter to `wezterm.lua`. `apply_to_config` preserves unrelated key bindings and replaces only `Alt+9`/`Alt+Shift+9`:

   ```lua
   local wezterm = require 'wezterm'
   local config = wezterm.config_builder()
   local tmx_switcher = require 'tmx_switcher'

   tmx_switcher.apply_to_config(config, wezterm, {
     -- Prefer the absolute output of `command -v tmx` because GUI apps may
     -- not inherit the interactive shell PATH.
     tmx_bin = '/absolute/path/to/tmx',
     -- Optional when installed beside tmx; otherwise set the absolute path.
     supervisor_bin = '/absolute/path/to/tmx-supervisor',
     enabled = true,
     allowed_local_domains = { 'local' },
     native_only_key = 'phys:0',
   })

   return config
   ```

4. Copy the `[switcher]` section from `config.example.toml` into `~/.config/tmx/config.toml`. Keep `enabled = false` for initial installation, reload WezTerm, verify native rows, then set it to `true` for the opt-in canary.

The Lua `enabled` option is a process-local emergency switch that avoids invoking `tmx`. The TOML `[switcher].enabled` flag controls whether inventory advertises tmux augmentation. Both must permit augmentation. Neither changes native collection.

## Bindings and interaction

| Binding | Mode | Behavior |
|---|---|---|
| `Alt+9` | Fuzzy | Type to filter; arrows move; Enter accepts; Escape, `Ctrl+C`, or `Ctrl+G` cancels |
| `Alt+Shift+9` | Non-fuzzy | Jump labels/arrows move; Enter accepts; the same cancel keys are no-ops |
| `Alt+0` | Fuzzy native-only | Emergency binding that never invokes `tmx` |

Repeated invocation is ignored while one selector is active, so pickers and inventory subprocesses do not stack. Choice IDs are invocation-local and opaque. Search terms are in visible labels because `InputSelector` searches the rendered label. Inventory and mapped-client requests run through the separately installed `tmx-supervisor`, which enforces an outer process-group deadline even if the configured `tmx` executable itself wedges; native collection therefore remains available.

The selector includes:

- one tab row and one row for every pane in every live WezTerm mux window/workspace;
- one row for every session, linked window identity, and pane on each configured trusted tmux endpoint; and
- text-only provider, kind, current/attached state, workspace/parent context, safe endpoint alias, title, command, path component, and note when the contract supplies one.

Equal labels remain distinct and receive deterministic workspace, parent, endpoint, or runtime-ID qualification. ANSI escapes and control characters are removed. Raw socket paths, cwd values, commands, labels, notes, PTYs, request JSON, and raw stderr are not logged by default.

## Destination semantics

- Selecting a native tab activates its current pane. Selecting a native pane activates that exact pane. Identity is re-resolved from a fresh global mux snapshot before activation. A current destination is a no-op.
- A tmux destination reuses a client only when exactly one fresh tmux client and one allowed local WezTerm pane share the same non-empty normalized TTY. Remote, nil-TTY, reused, or ambiguous PTYs never match.
- Candidate priority is invoking pane, current workspace, focus rank when supplied, then stable domain/workspace/window/pane identity.
- A matched client is revalidated by endpoint, generation, exact client name, TTY, PID, creation time, and UID. `tmx` issues one exact `switch-client -c ... -t ...` operation and WezTerm then focuses the known host pane.
- Without a safe local match, WezTerm revalidates the invoking pane's mux domain, opens exactly one tab in that same allowed local domain/window/workspace, and starts `tmx attach` with typed arguments. A configurable remote `DefaultDomain` is never used implicitly. `tmx` revalidates before selecting the target and runs `attach-session` last.
- An external-only client is never moved. No route uses `-d`, `-x`, shell evaluation, a rendered label, a session name fallback, or a request-provided socket path.

### Shared tmux state

Exact-client routing does **not** isolate active-pane state. Selecting a pane changes the active pane of its tmux window and can affect another client viewing that same window. Window and pane attachment perform validated selection before attaching and may also change shared window state. Zoom state is not intentionally changed.

A successful tmux mutation followed by unavailable/failed GUI focus is partial success. The adapter reports it and never retries against another client.

## Trusted endpoints

Configured selectors are:

```toml
[switcher]
enabled = true
deadline_ms = 400
endpoint_soft_timeout_ms = 150
max_concurrency = 4
discover_named = false

[[switcher.endpoints]]
selector = "default"
alias = "default"

[[switcher.endpoints]]
selector = "name:work" # tmux -L work
alias = "work"

[[switcher.endpoints]]
selector = "path:/absolute/path/to/project.sock" # tmux -S path
alias = "project"
```

`name:` and `path:` are different registration kinds; absolute paths are never interpreted as names. Named/path endpoints require explicit configuration. After resolution and trust verification, every inventory/route/attachment command is pinned with `-S` to the verified canonical socket path; the original selector remains provenance metadata only. `discover_named = true` scans only the effective user's standard `tmux-$UID` runtime directory, deterministically retaining the lexicographically smallest bounded candidate set; arbitrary scanning is unavailable.

An automatic endpoint must be a non-symlink Unix socket owned by the effective UID in a user-owned parent not writable by group/others. Before mutation, `tmx` rechecks path device, inode, owner, file type, canonical identity, server PID/start time, socket path, and tmux version. Explicit group-shared sockets are unsupported. This detects normal replacement/restart races but cannot turn tmux's pathname API into a descriptor-pinned security boundary.

Aliases resolving to one verified path collapse to one endpoint identity. A socket recreated at that path keeps its endpoint ID but receives a new server-generation token. Runtime `$`, `@`, and `%` IDs are meaningful only with the explicit local host domain, endpoint, generation, kind, and full parent IDs.

## Bounds and degradation

Defaults and hard ceilings are documented in [MACHINE_API.md](MACHINE_API.md). Inventory uses one monotonic 400 ms deadline, four bulk entity queries per healthy endpoint, at most four concurrent endpoints, bounded stdout/stderr, process-group termination, a 25 ms grace period, and reap. It never starts a server or creates a session.

Native rows survive:

- missing/old `tmx` or no tmux server;
- one dead, hung, untrusted, or malformed endpoint;
- partial, oversized, duplicate-key, excessive-depth, or incompatible JSON;
- unknown additive minor fields/statuses/kinds; and
- stale native or tmux selection.

A concise sanitized status appears in the selector title; details remain in explicit diagnostics and `tmx doctor`. Machine stdout is JSON only.

## Rollout and rollback

1. Install with `[switcher].enabled = false` and verify both `Alt+9` modes plus `Alt+0` native-only.
2. Enable for an opt-in canary and complete `docs/RELEASE_CHECKLIST.md`.
3. Default-on is permitted only after all native and tmux kinds, every configured endpoint kind, floor/latest compatibility, performance, stale, multi-client, multi-socket, and GUI rows pass.
4. Keep the kill switch for at least one minor release.

Rollback is immediate: set `[switcher].enabled = false`, set the Lua `enabled` option to `false`, or remove `apply_to_config` and restore the prior binding. No database migration, tmux cleanup, or tab termination is required. Successfully spawned tabs belong to the user and are never auto-killed.

## Troubleshooting

| Symptom | Action |
|---|---|
| Native rows only | Set both rollout flags, verify `tmx_bin`, run `tmx inventory --schema 1 --json`, and inspect the first bounded status |
| `tmx` or its supervisor not found | Use the absolute `command -v tmx` and `command -v tmx-supervisor` paths in Lua; `cargo install --path .` installs both |
| Endpoint is untrusted | Check socket/parent owner, mode, file type, symlink leaf, and selector kind |
| Named server absent | Start the configured `tmux -L name` server or remove the endpoint; inventory never creates it |
| Stale target/client | Reopen the selector; server generation or runtime identity changed |
| Inactive workspace does not receive OS focus | Activation and GUI focus are separate; Wayland forbids programmatic focus and inactive GUI lookup can be unavailable |
| New tab exits with an error | Read the held diagnostic, verify endpoint/generation, minimum tmux version, and permissions, then retry from a fresh selector |
| Labels are truncated | Raise no limits until privacy/resource review; the v1 512-byte label ceiling is intentional |

For contract-level codes and exact commands, see [MACHINE_API.md](MACHINE_API.md).
