# PRD: Unified WezTerm and tmx Destination Switcher

## Problem Statement

The user's terminal destinations are split across two control planes. WezTerm owns native mux windows, workspaces, tabs, and panes, while tmux owns sessions, windows, and panes on one or more socket endpoints. The existing `Alt+9` picker hides only part of that split: it starts from the invoking WezTerm mux window and adds tmux panes only when a visible WezTerm pane can be matched to a tmux client.

That inventory is incomplete. Native tabs and panes in other WezTerm mux windows or workspaces are absent. A tmux session is absent when it has no already-visible WezTerm-hosted client, and even a matched client contributes only panes in its current tmux window. Detached sessions, inactive windows, wrapped tmux processes, and non-default tmux servers therefore remain unreachable from the picker.

The current tmux path is also bound to the default server and routes with presentation-derived strings such as `session:window-index.pane-index`. tmux runtime IDs are stable only for an object's lifetime within one server, so an ID or label without endpoint and server-generation identity is not a safe cross-server routing contract. Official tmux target grammar supports raw window and pane IDs, but only in the server selected by the command ([target grammar](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L795-L828), [pane grammar](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L856-L867)).

`tmx` already has tmux parsing, raw IDs, stable keys, and an interactive palette, but its normal workflow inventories one server and opens a separate terminal fuzzy finder. It does not yet provide WezTerm with a bounded, side-effect-free, multi-endpoint inventory and typed activation contract.

The result is fragmented recall: a destination may be visible in WezTerm, visible only through `tmx` or tmux, or invisible until the user manually changes context. The completed feature must make every live native WezTerm tab and pane across mux windows and workspaces, and every live tmux session, window, and pane across trusted local endpoints, discoverable from the existing fast `Alt+9` interaction. Native WezTerm navigation must remain usable when `tmx` or any tmux endpoint is missing, slow, malformed, stale, or incompatible.

## Solution

Extend the existing WezTerm picker into one unified live-destination switcher. `Alt+9` remains the fuzzy entry point and `Alt+Shift+9` remains the non-fuzzy jump/diagnostic entry point. Both bindings use the same destination snapshot and activation behavior; only the initial selector mode differs.

### Product behavior

The selector contains:

- one native tab row for every live WezTerm tab across all mux windows and workspaces;
- one native pane row for every live WezTerm pane across all mux windows and workspaces; and
- one tmux session, window, and pane row for every live target on the default and configured trusted named or path endpoints.

Rows are searchable, textually identify their provider and kind, and include bounded useful context such as workspace, parent target, title, command, path component, note, and safe endpoint alias. Current and attached state must remain understandable without color. Equal labels remain separate and gain the shortest deterministic qualifier needed to distinguish them. Labels are presentation only and never become routing authority.

Selecting a native tab activates its current pane. Selecting a native pane activates that exact pane. Selecting a tmux target reuses and focuses a safely matched local WezTerm-hosted client when one exists; otherwise the adapter opens one new tab in the invoking workspace and local mux domain and attaches to the existing target. Selecting the current destination and cancelling are no-ops.

### Architecture and ownership

Use a thin WezTerm adapter over a versioned `tmx` machine interface:

- **WezTerm owns native state and GUI effects:** native mux enumeration, unified choice construction, `InputSelector`, native activation, GUI focus, and spawning a local attachment tab.
- **`tmx` owns tmux state:** trusted endpoint policy, bounded session/window/pane/client inventory, cross-endpoint identity, client matching inputs, stale-target revalidation, route planning, and tmux mutation.
- **The boundary is semantic JSON and typed arguments:** WezTerm retains an invocation-local map from opaque selector IDs to native identity or a typed tmux request. It never parses a rendered label back into an ID or command.
- **Failure domains remain separate:** native choices are built even if `tmx` is absent, times out, exits non-zero, returns malformed or oversized JSON, reports partial endpoint failure, or uses an incompatible schema.

This split is normative. WezTerm's child-process API is an argument-vector call that returns success, stdout, and stderr; endpoint trust, deadlines, bulk parsing, generation checks, and mutation are safer and more testable in `tmx` ([official `run_child_process` contract](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/wezterm/run_child_process.md#L8-L20)). External fuzzy finders and tmux plugins are implementation references, not dependencies or replacement presentation layers.

### Native WezTerm flow

At invocation, traverse [`wezterm.mux.all_windows()`](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/wezterm.mux/all_windows.md#L1-L6), not only the callback's current window. Retain mux-domain, workspace, mux-window, tab, and pane IDs as strings, then traverse every [`window:tabs()`](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/mux-window/tabs.md#L1-L6) and [`tab:panes()`](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/MuxTab/panes.md#L1-L6). Live handles may be retained as a fast path, but selection must re-resolve IDs from a fresh mux snapshot before activation.

`InputSelector` choices use opaque `{id, label}` values, and cancellation yields no usable ID ([official callback contract](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/keyassignment/InputSelector.md#L16-L31), [cancellation example](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/keyassignment/InputSelector.md#L142-L166)). Native activation must use a tested sequence that proves the workspace still exists, activates the pane, resolves its GUI window when available, and requests focus. [`pane:activate()`](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/pane/activate.md#L1-L7) also activates its containing tab/window. `SwitchToWorkspace` may be used only after existence is proved because it creates an absent workspace ([official behavior](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/keyassignment/SwitchToWorkspace.md#L1-L10)). GUI lookup may be unavailable for an inactive workspace, and OS focus is unsupported on Wayland; activation success and GUI-focus success are therefore separate outcomes ([GUI lookup limitation](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/wezterm.gui/gui_window_for_mux_window.md#L1-L10), [`window:focus()` limitation](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/window/focus.md#L1-L12)).

### tmux flow

WezTerm invokes one bounded machine inventory operation per picker invocation. The exact CLI spelling is an implementation detail, but it must be a machine-only argument-vector command equivalent to `tmx inventory --schema 1 --json`; existing human list and interactive output remain unchanged. `tmx` collects each trusted endpoint with a fixed number of formatted bulk commands for sessions, all windows, all panes, and clients. tmux supports formatted `list-sessions`, all-window, and all-pane inventory ([sessions](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L1272-L1287), [windows](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L3422-L3442), [panes](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L3379-L3407)). Inventory is read-only, concurrent within a configured limit, and returns successful endpoint records even when another endpoint fails.

On selection, WezTerm sends typed identity, never label text. `tmx` reopens the registered endpoint and revalidates socket identity and ownership, server generation, target kind and full parentage, and any selected client fingerprint immediately before mutation. A stale selection fails without creating a server, session, window, pane, or attachment.

Two route plans are supported:

1. **Matched local client:** join only when a local allowed WezTerm pane and exactly one fresh tmux client have the same normalized TTY/device identity and host/mux domain. Revalidate the full client fingerprint, execute one endpoint-qualified `switch-client -c <client> -t <target>` operation, then activate and focus the WezTerm host pane. tmux documents exact client selection and pane targets for `switch-client` ([official behavior](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L1713-L1735)). A successful tmux route followed by focus failure is partial success and must not retry against another client.
2. **No safe local match:** spawn exactly one new tab in the invoking workspace with explicit `DefaultDomain` and an argument vector carrying the typed endpoint/generation/target request. WezTerm otherwise defaults spawning to `CurrentPaneDomain`, which may be remote ([official `SpawnCommand` behavior](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/SpawnCommand.md#L40-L50)). The child revalidates and replaces itself with `tmux`, attaching to the existing session. Child window/pane selection occurs in a validated server command sequence with attach last; `attach-session -t` itself accepts a session target, not an arbitrary child target ([official `attach-session`](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L1082-L1104)). Immediate exit leaves a bounded actionable diagnostic.

The UI and documentation must disclose tmux shared state: selecting a pane may change that window's active pane for another client viewing the same window. Exact-client routing does not imply pane isolation; upstream `select-pane` and `switch-client` both set window active-pane state ([`select-pane` source](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/cmd-select-pane.c#L269-L276), [`switch-client` source](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/cmd-switch-client.c#L140-L159)).

### Choice construction and degradation

Normalize native and tmux records into one pure Lua choice model with provider, kind, opaque route key, primary label, secondary context, searched text, current/attached indicators, and deterministic sort keys. The canonical order is: current native destination; other current-workspace native panes/tabs; safely matched tmux routes; other native destinations; other tmux routes grouped by endpoint; diagnostics. Within a group, sort by descending activity when known, sanitized label, then canonical identity.

Deduplicate only identities proven equal. A native outer pane hosting a tmux client is related to, not equivalent to, the inner tmux targets; it keeps its native row. Cap rows and metadata before constructing selector choices. A partial or omitted tmux inventory may add one concise non-selectable status notice, but must not add focusable separator rows or remove native destinations. Repeated invocation creates a fresh snapshot and opaque route map and must not stack selectors or leak subprocesses.

## User Stories

### Unified invocation and keyboard interaction

1. As a WezTerm user, I want `Alt+9` to open one destination selector, so that I do not need separate navigation commands for WezTerm and tmux.
2. As an existing user, I want the current fuzzy `Alt+9` interaction preserved, so that the feature does not disrupt my muscle memory.
3. As an existing user, I want the non-fuzzy `Alt+Shift+9` interaction preserved, so that I retain its diagnostic and jump-label behavior.
4. As a keyboard-only user, I want every real destination reachable with the documented movement, filter, accept, and cancel keys, so that I never need a mouse.
5. As a user pressing Escape, `Ctrl+C`, or `Ctrl+G`, I want cancellation to make no state change and show no error, so that dismissing the picker is safe.
6. As a user pressing `Alt+9` repeatedly, I want at most one coherent selector operation, so that subprocesses and pickers do not stack or race.
7. As a user, I want the selector's visible help to match its actual mode and available keys, so that fuzzy mode, jump mode, acceptance, and cancellation are discoverable.

### Native WezTerm destinations across the mux

8. As a user with multiple WezTerm workspaces, I want panes outside the current workspace to appear, so that the selector is genuinely global.
9. As a user with multiple WezTerm GUI windows, I want destinations in another window to appear, so that I can navigate without first focusing that window.
10. As a user with many tabs, I want existing native tab destinations preserved, so that the unified picker remains at least as capable as the current picker.
11. As a user with split panes, I want every native pane to remain directly selectable, so that I can jump to the exact shell or application I need.
12. As a user selecting an inactive native workspace, I want WezTerm to activate the correct workspace, tab, pane, and GUI window, so that the destination becomes visible and focused where the platform permits.
13. As a user selecting my current destination, I want a defined no-op, so that the picker does not perform unnecessary focus or routing operations.

### Searchable, distinguishable, and accessible rows

14. As a user, I want session, window, pane, tab, and native-pane rows to be textually distinguishable, so that I understand each action even without color.
15. As a user, I want labels and searched text to include useful names, paths, commands, titles, notes, workspace context, parent context, and endpoint aliases when available, so that fuzzy search uses terms I remember.
16. As a user, I want the current destination clearly identified in text, so that I can orient myself before switching.
17. As a user, I want attached tmux targets identified in text, so that I know whether selection will focus an existing local client or create a new attachment.
18. As a user with duplicate human-readable names, I want the shortest useful workspace, parent, or endpoint qualifier shown, so that equal-looking rows are unambiguous without exposing raw socket paths.
19. As a user with unusual Unicode, control characters, ANSI escapes, tabs, or newlines in tmux metadata, I want labels sanitized without changing target identity, so that display data cannot corrupt the UI.
20. As a security-conscious user, I want raw socket paths and untrusted command output excluded from routine labels and logs, so that the picker does not expose unnecessary host details.

### Complete tmux inventory and multi-socket identity

21. As a tmux user, I want every live tmux session to be searchable, so that I can jump to work outside the currently attached session.
22. As a tmux user, I want every live tmux window to be searchable, so that I can jump directly to a window rather than navigating after attachment.
23. As a tmux user, I want every live tmux pane to be searchable, so that I can jump directly to the exact task.
24. As a user with multiple tmux servers, I want targets from the default and configured trusted named or path endpoints to appear together, so that socket choice does not require a separate workflow.
25. As a user with colliding tmux runtime IDs on different servers, I want those targets treated as distinct, so that selection never reaches the wrong server.
26. As a user looking at a child target, I want its parent session/window context visible and searchable, so that linked or similarly named tmux objects are understandable.

### Focus an existing client or attach locally

27. As a user with multiple local clients attached to a target, I want a deterministic client-selection policy, so that repeated selection behaves consistently.
28. As a user with an existing local WezTerm-hosted tmux client, I want the switcher to reuse, route, and focus that exact client, so that it does not create duplicate tabs unnecessarily.
29. As a user whose target is attached only in another terminal, I want a new local attachment rather than having the other terminal's client moved, so that the picker does not disrupt unrelated work.
30. As a user selecting an unattached target, I want it opened in a new tab in the current workspace and local mux domain, so that the result appears predictably.
31. As a user selecting a tmux session, I want routing to use that session's current window and active pane according to documented tmux behavior, so that the result is predictable.
32. As a user selecting a tmux window, I want the exact session and window activated, so that similarly named or linked windows cannot cause misrouting.
33. As a user selecting a tmux pane, I want the exact server, session, window, and pane selected, so that the picker reaches the intended destination.
34. As a user with multiple tmux clients, I want the documented shared-state effects of changing a window's active pane to be accurate, so that the product does not promise isolation tmux cannot provide.

### Stale targets, degradation, and partial failure

35. As a user whose tmux server restarted while the picker was open, I want selection to fail safely, so that reused runtime IDs cannot route me to an unrelated target.
36. As a user whose target or client disappeared while the picker was open, I want it revalidated before mutation, so that stale choices do not affect another destination.
37. As a user selecting a stale target, I want a clear error without implicitly creating a server, session, window, pane, or attachment, so that navigation never becomes accidental mutation.
38. As a user with one unavailable, malformed, slow, or hung tmux endpoint, I want healthy tmux and native destinations to remain available with a bounded degraded-state notice, so that one failure does not disable the picker.
39. As a user without `tmx` installed or running, I want the complete native WezTerm picker to continue working, so that tmux integration is an enhancement rather than a dependency.
40. As a user, I want the picker to appear within a bounded time despite endpoint failures, so that `Alt+9` remains interactive.
41. As a user whose tmux route succeeds but GUI focus fails, I want partial success reported accurately, so that the switcher does not retry routing or claim nothing happened.
42. As a user whose new attachment exits immediately, I want an actionable error that remains visible, so that I can understand why the destination did not open.

### Contract compatibility and version skew

43. As a user upgrading WezTerm and `tmx` independently, I want compatible additive schema changes accepted and incompatible versions to fall back to native switching with a bounded diagnostic, so that version skew does not break navigation.
44. As a user running an older supported WezTerm build, I want capability checks and a documented minimum-version policy, so that unavailable APIs degrade predictably rather than failing during selector construction or focus.

## Implementation Decisions

The requirements in this section are normative. CLI snippets are examples of a machine-only surface; implementations may change spelling without changing the schemas, identity, safety, compatibility, or observable route semantics.

### Approved module boundaries

The design uses six deep modules:

1. **Endpoint registry:** trusted endpoint discovery, normalization, aliases, ownership, endpoint identity, and server-generation checks.
2. **Inventory service:** bounded bulk collection, hierarchy normalization, client relationships, partial failures, and deadlines.
3. **Inventory contract:** versioned data-transfer objects, validation, compatibility rules, diagnostics, and resource limits.
4. **Route planner and executor:** explicit endpoint/target/client revalidation, action planning, mutation, postcondition checks, and typed outcomes.
5. **WezTerm choice model:** pure native/tmux merge, domain-aware TTY joining, ordering, sanitization, and opaque choice mapping.
6. **WezTerm adapter:** mux collection, inventory invocation, selector presentation, activation, spawning, focus, and diagnostics.

Tests may use these seams, but acceptance remains based on public outputs rather than private layouts.

### Authority and command safety

- WezTerm is the only user-facing selector; `tmx` is the only authority for tmux endpoints and mutations.
- Each row has an opaque invocation-scoped choice ID mapped to a typed route. Display labels, titles, paths, aliases, notes, commands, and stderr are never parsed as identity or code.
- All child and tmux invocations use argument vectors. No route path may use shell evaluation. This intentionally rejects decorated-line parsing and `eval` patterns used by some external pickers ([tmux-fzf session example](https://github.com/sainnhe/tmux-fzf/blob/05af76daa2487575b93a4f604693b00969f19c2f/scripts/session.sh#L6-L15), [window example](https://github.com/sainnhe/tmux-fzf/blob/05af76daa2487575b93a4f604693b00969f19c2f/scripts/window.sh#L15-L38)).
- Searchable terms must be included in fields the selector actually searches; hidden metadata is not considered a search alias.

### Endpoint registry and trust

Supported endpoint selectors are `default`, `name:<socket-name>`, and `path:<absolute-socket-path>`, mapped respectively to the default socket, `tmux -L <name>`, and `tmux -S <path>`. tmux documents that `-L` selects a socket name under the user runtime directory while `-S` selects a full path and overrides `-L` ([official socket behavior](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L161-L209)). The implementation must model named and path endpoints separately; it must not pass an absolute path to `-L`.

`endpoint_id` is a deterministic digest of the canonical socket path. Aliases that resolve to one verified path collapse to one endpoint while aliases remain display metadata. Socket recreation retains endpoint ID but changes server generation. Default discovery includes only the current environment's default socket. Named and path endpoints require configuration; opt-in named-socket discovery may scan only the current user's documented `tmux-$UID` directory. Arbitrary directory scanning is off by default.

Automatic endpoints must be Unix sockets owned by the effective UID, with a user-owned parent that is not writable by others. Resolve the platform temporary-directory alias, reject a symlink leaf, and reject invalid file types or ownership. Explicit group-shared sockets are unsupported in v1. Pin and recheck `{device, inode, uid, file_type}` before mutation. This is best-effort replacement detection, not an absolute security boundary, because tmux reopens a pathname rather than accepting an already-verified file descriptor.

Server generation includes socket device/inode, server PID and start time, canonical socket identity, and tmux version. tmux exposes `pid`, `start_time`, `socket_path`, and `version` through formats ([official format table](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L7355-L7407)). PID alone is insufficient because it can be reused. Connection failure is `unavailable_endpoint`; inventory never starts a server or creates a session.

### Versioned inventory contract

The canonical inventory schema is `dev.tmx.inventory` major 1, minor 0. Its envelope contains request ID, producer version, generated time, applied deadline/limits, completeness, capabilities, endpoint results, and structured bounded diagnostics. Every endpoint contains opaque endpoint ID, safe alias, selector kind, trust source, generation, status, and arrays of sessions, windows, panes, and clients. Raw socket paths are private execution data and are never routine labels or logs.

Required entity data is:

- **session:** endpoint/generation, `$session_id`, name, created/activity data, optional last-attached time, and attached count;
- **window:** parent session identity, `@window_id`, index, name, active state, and activity;
- **pane:** parent session/window identity, `%pane_id`, index, active state, and optional TTY, path, command, and title; and
- **client:** endpoint/generation, exact client name, PID, creation time, TTY, UID, attached session ID, and optional activity/flags.

Runtime IDs retain their `$`, `@`, and `%` sigils as strings. tmux guarantees an ID only for an object's lifetime within one server ([official ID behavior](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L921-L947)). Canonical target identity is `(host-or-mux-domain, endpoint_id, generation, target_kind, parent IDs, raw runtime ID)`. A linked window is identified by `(session_id, window_id)` and a pane by `(session_id, window_id, pane_id)`, because one window may be linked into multiple sessions.

Collection uses four bounded formatted calls per endpoint: `list-sessions -F`, `list-windows -a -F`, `list-panes -a -F`, and `list-clients -F`. tmux has no JSON output, so the parser must use collision-safe framing or strict escaping and validate field count, sigils, numeric ranges, parent references, UTF-8 policy, and lengths. A delimiter collision makes a record malformed and the endpoint partial; it must never silently shift fields. Collision-prone `::` splitting in external prior art is not adopted ([sesh parser](https://github.com/joshmedeski/sesh/blob/e09b6007784678ffeec741cb5e5b895bde8f47ad/tmux/list.go#L24-L59)).

Hard limits are 32 endpoints, 10,000 targets, 4 MiB stdout per endpoint, 16 KiB retained stderr per endpoint, 512-byte labels, 4 KiB paths/commands, JSON depth 16, and duplicate-key rejection. Limit or parse failures produce typed partial errors. One endpoint failure never erases successful results; `complete=false` when any configured endpoint is incomplete.

Consumers require the exact schema name and a supported major. Minor versions may add optional fields and capabilities only; required fields cannot change type or meaning. Unknown fields are ignored. Unknown target kinds/statuses are skipped with a bounded diagnostic. Missing required fields reject tmux augmentation before any tmux rows are built. Potentially wide counters are strings when Lua/JSON precision is uncertain.

### Client fingerprint and deterministic join

A client fingerprint is `(endpoint_id, generation, client_name, client_tty, client_pid, client_created, client_uid)`. `client_name` is the exact `switch-client -c` target; every field is refreshed before mutation. tmux exposes these client fields separately ([official formats](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L7201-L7223)). TTY alone is unsafe because PTYs are reused.

Join a WezTerm pane to a client only when host and allowed local mux domain agree, both TTYs are non-empty, normalized device paths and device identity where available match, and exactly one fresh client matches. `pane:get_tty_name()` may return `nil` on unsupported domains or Windows ([official API](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/pane/get_tty_name.md#L1-L24)); ambiguous, remote, and nil-TTY cases create a new local attachment instead.

The deterministic candidate order is: invoking pane's unique match; unique match in the current workspace; most recently focused local GUI window; stable `(domain, workspace, mux_window_id, pane_id)` tie-break. External-only clients are never moved in v1.

### Route contract and semantics

The canonical route schema is `dev.tmx.route` major 1, minor 0. A request contains request ID, endpoint ID, expected generation, target kind with full parent/raw IDs, optional client fingerprint, mode (`prefer_client` or `new_attachment`), and deadline. Immediately before mutation, recheck endpoint identity, generation, all parent/child IDs, and the full optional client fingerprint.

Typed outcomes are `success`, `success_new_attachment`, `stale_target`, `stale_client`, `unavailable_endpoint`, `untrusted_endpoint`, `incompatible_schema`, `timeout`, `command_failure`, and `partial_success`. Every response includes request ID, plan kind, elapsed time, and sanitized bounded diagnostics. GUI focus refines a successful mapped-client result into routed-and-focused or routed-but-not-focused; it does not change tmux success into a retry.

Mapped-client routing uses one command, not a racy sequence:

- session: `switch-client -c <client_name> -t <$session_id>`;
- window: `switch-client -c <client_name> -t <$session_id:@window_id>` after proving this syntax on every supported tmux version, otherwise a tested compatibility plan that reports shared-state effects; and
- pane: `switch-client -c <client_name> -t <%pane_id>` after parent verification.

New attachments use `attach-session -t <$session_id>` for sessions. For windows and panes, a validated child-selection sequence runs against the endpoint with attach last. `select-pane` and `select-window` mutate shared active state ([official commands](https://github.com/tmux/tmux/blob/7abb9af06236eb9def862bb88a82792f6c846bef/tmux.1#L3986-L4052)); postcondition mismatch after command success is `partial_success`. Never use detach/steal options such as `-d` or `-x`.

### Native identity, deduplication, and compatibility

Native identity is `(domain, mux_window_id, tab_id, pane_id)` and is re-resolved before activation. Equal labels, paths, titles, and workspace names never deduplicate targets. A native tab and its active pane remain distinct actions, and a native outer pane remains distinct from tmux rows it hosts.

The feature probes required WezTerm and tmux capabilities rather than relying only on version strings. Probe formatted fields, `switch-client -c`, pane-target switching, `InputSelector`, mux enumeration/IDs, pane activation, TTY, domain, and GUI mapping. Disable only unsupported tmux augmentation or route kinds and preserve the native picker. Existing human CLI output, interactive commands, and completions remain backward compatible.

## Testing Decisions

Tests assert externally observable behavior rather than private helper calls or data-structure layout. Relevant outputs include inventory JSON, validation errors, ordered selector choices, labels and searched text, opaque route requests, exact spawned argv, focused mux identity, typed diagnostics, exit status, mutation postconditions, elapsed-time bounds, and child-process cleanup.

### Harness and fixture policy

- Keep canonical fixtures in `tests/fixtures/inventory/v1/`. Normalize only socket temporary paths, PIDs, timestamps, and measured duration; never normalize endpoint identity, generation, ordering, outcome, or argv.
- Each real-tmux test uses a unique `TMUX_TMPDIR`, `tmux -L tmx-test-<pid>-<nonce> -f /dev/null`, explicit endpoint arguments, and unconditional cleanup. Tests must never infer, address, or kill the ambient server. Missing tmux may skip only an optional developer lane; the required integration job fails when its prerequisite is absent.
- Multi-client tests allocate real PTYs. Use bounded state polling or `tmux wait-for`, not fixed sleeps alone. A second isolated tmux server may host inner attachment clients, following tmux's own integration-test pattern ([tmux integration helpers](https://github.com/tmux/tmux/tree/7abb9af06236eb9def862bb88a82792f6c846bef/regress)).
- Stale/race tests expose a test-only barrier between revalidation and mutation, then deterministically delete or replace a target, client, PTY, or socket.
- Load the pure Lua choice model outside live WezTerm configuration with fake native snapshots and shared inventory fixtures. Test the thin adapter with spies for `InputSelector`, subprocess execution, mux lookup/activation/focus, and spawn.
- Golden changes require review and a short statement of changed behavior; never auto-accept snapshots.

### Required test matrix

| Area | Required cases and observable assertions | Gate |
|---|---|---|
| Contract and goldens | No server; one endpoint; complete session/window/pane/client hierarchy; colliding `$`/`@`/`%` IDs on two endpoints; two clients; partial failure; unknown additive fields; incompatible major; stale generation; hostile/max-length metadata. Assert required fields, parent links, endpoint-qualified keys, stable order, typed diagnostics, limits, and canonical reserialization. | Every PR |
| Endpoint registry | Default, named `-L`, and path `-S`; aliases, duplicates, ownership/type rejection, prohibited symlinks, equivalent aliases, unlink/replacement, deterministic identity. Rejected paths are not queried and discovery creates no socket/server. | PR and Unix integration |
| Malformed input | Missing/extra fields, delimiter bytes, empty/truncated records, invalid numbers/booleans/UTF-8/JSON, duplicate keys, excessive depth/count/bytes, non-zero exit, stderr flood, and timeout. Assert no panic, bounded diagnostics, typed partial/error, and preservation of healthy endpoints. | Every PR |
| Property and fuzz | DTO round-trip; additive-field tolerance; target-key uniqueness; total ordering independent of input permutation; sanitizer idempotence/bounds/control removal; label changes cannot alter routes; dedupe never merges unproved identities. Fuzz record parsing, JSON/schema validation, sanitization, and choice fixtures with golden and hostile seeds. | PR property/smoke; nightly fuzz |
| Multi-socket | Two isolated servers with overlapping names and IDs. Inventory keeps both distinct; routing B leaves all state/generation on A unchanged. Cover hung/dead B, restart at one path, aliases, ambient `TMUX`/`TMUX_PANE`, and absence of default-server creation. | Required Linux; macOS release |
| Multi-client | Two PTY clients on different sessions plus an unrelated client. For each session/window/pane route, record every client's session, session active window, window active pane/zoom, count/TTY/fingerprint, and unrelated state. Cover invoking-client priority, mapped local client, external-only client, deleted client, and PTY reuse. | Required Linux integration |
| Stale and races | At the barrier delete a pane/window/session, kill/recreate the endpoint, replace the socket inode, detach/delete a client, reuse a PTY, or change generation. Assert zero unintended mutation, no name fallback, no new server, and exact stale/unavailable outcome. | PR fake executor and integration |
| Route and argv | Table-test session/window/pane plans, current no-op, zoom, local/external/absent clients, restart, and deletion. Assert exact endpoint/target argv. Whitespace, quotes, `$()`, backticks, globs, leading `-`, newlines, and escapes remain one argument and never reach a shell. | Every PR |
| Choice model and adapter | Native/tmux rows, dedupe, order, endpoint disambiguation, text-only kind/current/attached state, sanitization, searched terms, truncation, opaque mappings, inactive workspaces, multiple GUI windows, non-local domains, stale mux IDs, cancellation, repeat invocation, focus failure after route, and local-domain/current-workspace spawn. Native rows survive every tmux failure. | Every PR with pure tests/spies |
| Compatibility | Preserve human stdout/stderr/status, interactive navigation, bash/zsh/fish completion generation, fuzzy `Alt+9`, and non-fuzzy `Alt+Shift+9`. Machine APIs have separate goldens. Test minimum and newest supported versions. | Every PR and version matrix |
| Performance/resources | Verify the canonical budgets in Further Notes. After 100 invocations child count returns to baseline, no zombies remain, peak concurrency stays bounded, and one fake-clock wall deadline propagates through all stages. | Deterministic PR tests and release benchmark |
| Manual GUI | Record OS/display server, WezTerm/tmux versions, result, and evidence for macOS focus, inactive workspace, multiple GUI windows, local-domain/new-tab placement, cancel/no-op, stale mux target, both bindings, readable labels, and fallback diagnostics. Repeat X11/Wayland where supported and record Wayland limits. | Release checklist |

Existing parser tests, unique-socket tmux integration tests, completion tests, stable-key tests, and the manually validated picker are useful foundations, but they do not yet prove multi-client routing, server generation, stale barriers, hostile metadata, or WezTerm model/adapter degradation. Malformed identity/state must never be coerced into a plausible record. Property and fuzz tooling should be added for exposed parser, schema, identity, ordering, and sanitizer surfaces.

### Acceptance gates

1. **PR gate:** Rust unit/contract/property tests, pure Lua tests, compatibility tests, hostile corpus, and deterministic stale tests pass with no unexplained golden drift.
2. **Integration gate:** required single-socket, two-socket, and two-client suites pass on a runner that guarantees tmux; a silent early return is not a pass.
3. **Safety gate:** no ambient socket access, inventory mutation, stale unintended mutation, shell evaluation, unbounded output/depth/count, or leaked child.
4. **Behavior gate:** every typed outcome and documented session/window/pane effect has an external assertion. Exact-client isolation is documented only where the multi-client matrix proves it.
5. **Degradation gate:** native choices remain usable for missing binary/server, partial failure, malformed/oversized JSON, timeout, stale selection, and incompatible schema.
6. **Performance gate:** all external timing targets pass on a pinned warmed runner with bounded concurrency and no repeated-invocation growth.
7. **Release gate:** supported-platform manual rows are attached; an untested GUI/display row is declared as a gap, never inferred as a pass.

## Out of Scope

The first release is a live destination switcher: it collects bounded local WezTerm/tmux inventory, selects an existing destination, and routes or attaches the invoking WezTerm client. It is not a general session manager. The following exclusions prevent restoration, destructive mutation, shell execution, and transport policy from becoming implicit picker requirements.

### Deliberate exclusions

- **Alternative picker stacks and session-manager parity.** Do not replace WezTerm's selector with fzf, sesh, sessionx, tmux-fzf, or another plugin, and do not reproduce their broad command surfaces. tmux-fzf and sessionx include creation, rename, deletion, previews, detach, movement, and other management actions ([tmux-fzf scope](https://github.com/sainnhe/tmux-fzf/blob/05af76daa2487575b93a4f604693b00969f19c2f/README.md#L3-L13), [sessionx scope](https://github.com/omerxx/tmux-sessionx/blob/c9aaa1d309791871b5e8c1f9bfb91ecc5fa7da3a/README.md#L1-L3)); they are comparison products, not acceptance targets.
- **Preview and terminal-content features.** No scrollback, command-output, file/tree, or screenshot preview; clipboard extraction/insertion; or persistence/indexing of pane text. Tools such as extrakto deliberately capture pane/window content ([behavior](https://github.com/laktak/extrakto/blob/d1af77988081dae496fa4a1f5e5e6bc9ef66767f/README.md#L5-L16), [capture scope](https://github.com/laktak/extrakto/blob/d1af77988081dae496fa4a1f5e5e6bc9ef66767f/README.md#L103-L117)); that needs separate privacy, redaction, rendering, and latency requirements.
- **Mutation from the picker.** No create, kill, rename, detach, link, move, swap, join, break, resize, layout, promote, or process-signal actions. Do not move or steal clients attached in other terminals. Selection may activate an existing target for a matched local client or attach a new invoking-client tab only.
- **Dormant projects and create-or-attach.** No zoxide/project inventory, repository cloning, directory creation, startup commands, or session creation for a non-live candidate. sesh demonstrates that this is a separate project/filesystem workflow ([scope](https://github.com/joshmedeski/sesh/blob/e09b6007784678ffeec741cb5e5b895bde8f47ad/README.md#L25-L34), [commands](https://github.com/joshmedeski/sesh/blob/e09b6007784678ffeec741cb5e5b895bde8f47ad/README.md#L698-L716)).
- **Remote and arbitrary endpoint discovery.** No SSH, cloud, cross-machine, remote-mux, or arbitrary-path probing. Remote support needs separate host identity, authentication, trust, cancellation, path translation, duplicate identity, and failure UX.
- **Durable restoration and persistence.** No reboot/crash restoration of workspaces, windows, tabs, panes, layouts, processes, output, contents, or environment; no autosave hooks. tmux-resurrect and WezTerm Session Manager center a separately persistent lifecycle ([tmux-resurrect scope](https://github.com/tmux-plugins/tmux-resurrect/blob/cff343cf9e81983d3da0c8562b01616f12e8d548/README.md#L38-L52), [WezTerm Session Manager](https://github.com/danielcopper/wezterm-session-manager/blob/751f4ae23fc3511e9e4d316d15404d543a257676/README.md#L1-L14)).
- **Display text as protocol or code.** Do not parse human list output or labels as a machine protocol and do not execute shell fragments derived from display fields. This is a security boundary, not a temporary deferral.
- **Additional initial commitments.** Persistent cross-restart MRU, Windows/WSL, a daemon or long-lived inventory service, explicit group-shared sockets, and guaranteed OS focus where forbidden by the compositor are excluded. A daemon requires measured need; Windows/WSL requires separately specified transport, path, TTY, quoting, and mux-domain semantics.

### Future-extension seams

Future work may add a capability/action registry with validation and confirmation, separate live/project/remote providers, a lazy bounded preview provider, a separately versioned restoration subsystem, typed non-shell commands, cross-restart identity, and optional success observers. None may weaken v1 endpoint trust or make labels authoritative. An excluded feature enters scope only with its own user story, threat/failure model, budget or durability policy, contract change, and acceptance tests.

## Further Notes

### Delivery phases

Deliver reversible vertical slices. Each phase closes only when its observable contract and relevant timing gate pass.

1. **Behavioral proof:** freeze existing `Alt+9`, `Alt+Shift+9`, human CLI, completions, and interactive behavior. Add isolated two-client/two-socket fixtures that record session, window, pane, zoom, and unrelated-client effects. No user behavior changes.
2. **Internal pane-first tracer:** exercise the full path through the v1 envelope, endpoint-qualified opaque identity, bounded runner, one-endpoint pane inventory, and typed pane route. Reserve `session|window|pane` in schema v1, advertise only the pane capability, and keep this build unreachable from the normal picker.
3. **Kind-complete single endpoint:** inventory and route sessions, windows, and panes with parent IDs and generation. No user-visible beta until all three kinds pass stale-target and two-client tests.
4. **Multi-endpoint correctness:** support default, named `-L`, and explicit path `-S` endpoints and prove colliding runtime IDs cannot cross servers. This is a release blocker, not polish.
5. **Opt-in WezTerm canary:** merge native and kind-complete tmux choices behind one flag. Retain native collection as fallback/rollback. Reuse only a safely matched local client; otherwise spawn locally.
6. **Hardening and default-on:** pass hostile-output, 1,000-target, hung-endpoint, independent-version-skew, inactive-workspace, repeated-invocation, documentation, and rollback gates. Keep the kill switch for at least one minor release.

“Feature complete,” “beta,” and user-facing demonstrations always mean native WezTerm tabs and panes across mux windows/workspaces plus tmux sessions, windows, and panes across every configured trusted endpoint. Pane-only is an internal tracer bullet, never the final product scope.

### Version floors and compatibility

- **WezTerm:** minimum `20230408-112425-69ae8472`, the first documented `InputSelector` build. Older builds keep the native binding and receive at most one bounded diagnostic. The floor is supported by the official API history in [`InputSelector`](https://github.com/wez/wezterm/blob/76b606ec597a3c0263fa60321548637451c0a547/docs/config/lua/keyassignment/InputSelector.md#L6-L32).
- **tmux:** minimum 3.2. Bounded tag verification found all required session/window/pane/client/socket formats in 3.2 and later checked tags; CI tests 3.2 and the newest supported stable. The 3.2 man page is pinned at [`bc4681c`](https://github.com/tmux/tmux/blob/bc4681c83d612a3d9609dd609e7d89b035b25dd5/tmux.1).
- Machine-only commands and schemas are additive to existing human interfaces. Unknown optional minor fields are ignored; missing required fields, unsupported majors or kinds, and missing capabilities reject only tmux augmentation.
- WezTerm and `tmx` remain independently upgradeable and downgradeable. Missing/old binaries, version skew, malformed/oversized output, and timeouts cannot prevent native selector opening.
- No database migration is required. Endpoint configuration is additive; omission means the default server. Downgrade/removal leaves existing recency and notes untouched.

### Performance and resource budget

Measure from key action to `InputSelector` on a pinned warmed runner:

- normal local inventory p95 is at most **100 ms**;
- 1,000-target construction p95 is at most **250 ms**; and
- native fallback with one hung endpoint is at most **500 ms**.

Use one monotonic invocation-to-choice deadline, not stacked timeouts. The default internal budget is 400 ms and is configurable from 100 to 2,000 ms. Suggested allocations are discovery at most 10 ms; endpoint processes concurrent up to four, with 150 ms soft budget each and 350 ms aggregate inventory cutoff; validation/merge at most 25 ms normally or 100 ms for 1,000 targets; and a 50 ms presentation reserve. Internal allocations may be tuned from canary data but do not extend the external 500 ms fallback gate.

At deadline, terminate children, allow a 25 ms grace period, kill, close pipes, and reap. Stop launching endpoint work when the remaining budget cannot preserve rendering time. Use bulk entity-class queries, never one process per target. Repeated invocation must not grow child count or retained memory. Route execution p95 is at most 250 ms excluding terminal startup.

### Observability and privacy

Observability is local, opt-in, structured, and non-networked. An ephemeral invocation ID may record stage timings, configured/success/failed endpoint counts, counts by target kind, producer/schema versions, truncation flags, route outcome, and fallback reason. Record safe aliases or salted endpoint hashes, never socket paths. Do not log labels, notes, titles, commands, cwd, PTYs, request JSON, or raw stderr by default.

`Alt+Shift+9` remains non-fuzzy and may show a short sanitized status. Detailed diagnosis belongs in `tmx doctor` or explicit debug logging. Machine stdout is JSON only; bounded sanitized diagnostics use stderr. `no_server` and `not_configured` are expected states, while stale, timeout, malformed, incompatible, and command failure remain distinct codes.

### Rollout, rollback, and documentation

Roll out disabled, then opt-in canary, then default-on. The flag controls only tmux augmentation, never native collection. Keep a native-only emergency binding. On runtime failure, show native choices in the same invocation when budget remains; otherwise apply only a short process-local backoff to the next invocation.

Rollback disables the adapter and restores the old binding. It requires no database rollback or tmux cleanup. Successfully spawned tabs are user-owned and are not auto-killed.

Before default-on, publish:

1. a compatibility matrix with exact floors, floor/latest combinations, supported local Unix-like platforms, and unsupported remote/non-local domains;
2. examples for default, named, and path sockets; trust rules; aliases; feature/rollback flags; limits; and redacted debug mode;
3. target semantics for sessions, windows, and panes, including endpoint-qualified identity, stale behavior, client reuse, new-tab placement, zoom, and shared-pane effects;
4. a machine-contract reference with v1 fixtures for success, partial failure, ID collision, additive fields, incompatible schema, truncation, all route outcomes, stdout/stderr, and exit codes;
5. troubleshooting for missing/old binaries, no server, permissions, stale/hung/malformed endpoints, inactive workspaces, Wayland, privacy, and rollback; and
6. a release checklist recording benchmark host, p50/p95/max, target/endpoint counts, version matrix, GUI checks, canary duration, rollback rehearsal, and known limitations.

### Decision and evidence gaps

| ID | Decision or residual gap | Severity | Resolution gate |
|---|---|---:|---|
| D1 | Pane-first is internal; schema v1 and final scope include all three tmux target kinds. | High | A pane-only build cannot enable the public picker. |
| D2 | Multi-socket identity precedes public rollout. | Blocker | Endpoint, generation, parent IDs, and raw target ID are mandatory; collision fixtures pass. |
| D3 | Floors are WezTerm `20230408-112425-69ae8472` and tmux 3.2. | Medium | Test floor/latest and fail only tmux augmentation on skew. |
| R1 | Existing `tmx` code treats socket strings as `-L`; absolute paths require `-S`. | High | Separate endpoint kinds and assert exact argv. |
| R2 | Existing sequential mutations do not prove exact-client semantics, and active pane is shared window state. | High | Typed one-command plans where supported, two-client state matrix, and partial-success outcomes. |
| R3 | Existing command execution lacks complete timeout/output/redaction guarantees. | High | Bounded kill/reap runner, byte caps, typed codes, and sanitized diagnostics. |
| R4 | Existing formatted parsing permits delimiter and malformed-number hazards. | High | Collision-safe framing/escaping, strict validation, hostile fixtures, and fuzzing. |
| R5 | Existing inventory lacks clients, generation, and endpoint-qualified identity. | Blocker | Add all three before client reuse or routing claims. |
| R6 | Inactive-workspace GUI lookup and OS focus vary by platform. | Medium | Separate activation from focus outcome and complete manual platform acceptance. |
| R7 | Fully qualified mapped-window syntax and 150 ms endpoint soft budget require oldest/latest runtime proof. | Medium | Compatibility adapter and canary tuning without weakening external safety/timing gates. |

External research used pinned WezTerm, tmux, sesh, tmux-fzf, sessionx, extrakto, tmux-resurrect, and WezTerm Session Manager sources. GitHub search/content tooling was unreliable for some queries, so the reports used a bounded GitHub API fallback against named repositories and immutable SHAs. Those failures do not establish absence. No live GUI, isolated multi-socket, or two-client experiment was run during research; the required automated and manual gates above must close those evidence gaps before rollout.
