# tmx machine API v1

These commands are additive machine-only surfaces. Existing human `tmx`, `palette`, `ls`, `view`, `new`, `last`, `recent`, `note`, `rename`, `doctor`, and completion behavior is unchanged.

## Compatibility rules

- Inventory schema: `dev.tmx.inventory`, major 1, minor 0.
- Route schema: `dev.tmx.route`, major 1, minor 0.
- Consumers require the exact schema name and supported major. A higher minor may add optional fields/capabilities only. Unknown optional fields are ignored.
- Missing required fields, changed types/meaning, unsupported majors, absent capabilities, duplicate object keys, excessive depth/count/bytes, and malformed UTF-8 reject tmux augmentation before rows are built.
- Unknown endpoint statuses or target kinds are skipped with one bounded diagnostic. Native WezTerm choices remain available.
- Runtime IDs retain `$`, `@`, and `%` sigils as JSON strings. Potentially wide tmux counters are strings to avoid Lua number precision loss.

Canonical fixtures live in `tests/fixtures/inventory/v1/`. Fixture normalization may replace socket paths, PIDs, timestamps, and measured duration; it must not replace endpoint IDs, generation, ordering, outcomes, or argv.

The WezTerm adapter runs inventory and mapped-client calls through the separately installed `tmx-supervisor`. It enforces a process-group deadline and byte caps outside the configured `tmx` executable, passes successful stdout through byte-for-byte, returns 124 on deadline and 125 on supervisor/output-limit failure, and never turns child stderr into routine UI logs. `cargo install --path .` installs both binaries.

## Inventory

```sh
tmx inventory --schema 1 --json \
  --request-id wezterm-opaque-id \
  --deadline-ms 400
```

`--json` is required. The deadline is clamped to 100–2000 ms. A valid envelope exits zero even when `complete=false`; endpoint failures are data, not a reason to corrupt machine stdout. CLI/config/serialization failures exit non-zero with bounded stderr and no human text on stdout.

### Envelope

```json
{
  "schema": {"name":"dev.tmx.inventory","major":1,"minor":0},
  "request_id": "wezterm-opaque-id",
  "producer_version": "0.1.0",
  "generated_at": "RFC3339",
  "applied_limits": {
    "deadline_ms": 400,
    "max_endpoints": 32,
    "max_targets": 10000,
    "max_stdout_bytes_per_endpoint": 4194304,
    "max_stderr_bytes_per_endpoint": 16384,
    "max_concurrency": 4
  },
  "complete": true,
  "capabilities": [
    "augmentation_enabled", "clients", "endpoint_generation",
    "multi_endpoint", "route_pane", "route_session", "route_window"
  ],
  "endpoints": [],
  "diagnostics": []
}
```

`augmentation_enabled` is present only when `[switcher].enabled = true`. Other capabilities advertise complete v1 behavior by kind.

Each endpoint contains:

- explicit local `host_domain`, opaque `endpoint_id`, safe `alias`, `selector_kind`, and `trust_source`;
- optional `generation` for a successfully contacted server;
- status `available`, `partial`, `unavailable_endpoint`, `untrusted_endpoint`, or `timeout`;
- arrays `sessions`, `windows`, `panes`, and `clients`; and
- structured bounded diagnostics.

Raw socket paths are private execution data and are never routine labels or logs. Registration resolves default/name/path provenance once; all subsequent tmux commands use `-S` with the canonical socket path that passed trust checks, so a retargeted original selector cannot redirect a route. Generation contains a route token plus socket device/inode/UID, server PID/start time, redacted canonical socket identity, and tmux version. The token changes when the server or socket is recreated.

Required records are:

- session: endpoint/generation, `$session_id`, name/path, creation/activity, optional last-attached, attached count, window count, optional note;
- window: endpoint/generation, parent `$session_id`, `@window_id`, index/name, active/activity, optional note;
- pane: endpoint/generation, full `$session_id`/`@window_id` parentage, `%pane_id`, index/active/activity, optional TTY/path/command/title/note; and
- client: endpoint/generation, exact client name, PID, creation time, TTY, effective UID derived from the verified user-owned endpoint, attached session, optional current window/pane/activity/flags.

A linked window identity is `(endpoint, generation, session_id, window_id)`, and pane identity additionally contains `pane_id`. The same runtime IDs on different endpoints are distinct.

### Collection safety

For each trusted healthy endpoint, inventory uses four bulk formatted commands:

1. `list-sessions -F ...`
2. `list-windows -a -F ...`
3. `list-panes -a -F ...`
4. `list-clients -F ...`

A generation probe precedes collection. Records use the printable versioned `|:tmx:v1:|` framing token because tmux 3.2–3.3 rewrite literal ASCII control separators on some platforms. Parsing requires exact field counts, sigils, booleans, numeric ranges, parent references, valid UTF-8, and field lengths. A framing-token collision rejects the record and marks the endpoint partial; it never shifts fields. Inventory uses no creation-capable tmux command.

Hard limits are 32 endpoints, 10,000 destination targets, 4 MiB retained stdout and 16 KiB retained stderr per endpoint, 512-byte labels, 4 KiB metadata fields, JSON depth 16, and duplicate-key rejection in the Lua consumer. Limits yield typed partial diagnostics.

## Mapped-client route

The WezTerm adapter invokes the argument-vector equivalent of:

```sh
tmx route --schema 1 --json --request-id opaque \
  --host-domain local --endpoint-id ep_HASH --generation gen_HASH \
  --target-kind pane --session-id '$1' --window-id '@2' --pane-id '%3' \
  --mode prefer-client \
  --client-name /dev/ttys001 --client-tty /dev/ttys001 \
  --client-pid 123 --client-created 456 --client-uid 501 \
  --deadline-ms 250
```

The command accepts no socket path, rendered label, shell fragment, session-name fallback, or cwd. It resolves `endpoint_id` only through the fresh trusted registry. Immediately before mutation it rechecks socket identity/ownership, generation, exact target kind and full parent IDs, and every client fingerprint field. A test-only barrier (`TMX_TEST_MODE=1` plus `TMX_TEST_BARRIER_FILE`) exists for deterministic race tests and is inert otherwise.

Mapped-client plans use one endpoint-qualified command:

- session: `switch-client -c CLIENT -t $SESSION`
- window: `switch-client -c CLIENT -t $SESSION:@WINDOW`
- pane: `switch-client -c CLIENT -t %PANE` after parent verification

No plan uses `-d` or `-x`. Current target selection returns success without mutation. Command success is followed by an exact-client postcondition check. If tmux routed but the postcondition cannot be proved, outcome is `partial_success`; no second client is tried.

The response is JSON-only and exits zero when a typed response was produced:

```json
{
  "schema":{"name":"dev.tmx.route","major":1,"minor":0},
  "request_id":"opaque",
  "plan_kind":"mapped_client",
  "outcome":"success",
  "elapsed_ms":12,
  "diagnostics":[]
}
```

Outcomes are `success`, `success_new_attachment`, `stale_target`, `stale_client`, `unavailable_endpoint`, `untrusted_endpoint`, `incompatible_schema`, `timeout`, `command_failure`, and `partial_success`. WezTerm GUI focus refines successful routing into focused/not-focused UI status; focus failure never retries tmux mutation.

## New attachment

The adapter spawns one local/default-domain tab with typed arguments equivalent to:

```sh
tmx attach --schema 1 --request-id opaque \
  --host-domain local --endpoint-id ep_HASH --generation gen_HASH \
  --target-kind window --session-id '$1' --window-id '@2' \
  --deadline-ms 250 --hold-on-error-ms 3000
```

`attach` has inherited terminal I/O. It revalidates endpoint/generation/parents immediately before launching tmux. Sessions attach directly. Window and pane requests run an argument-vector command sequence with validated `select-window`/`select-pane` followed by `attach-session` last. A failure prints a bounded actionable diagnostic, remains visible for at most the configured 0–10,000 ms hold, and exits non-zero. It never creates a server/session, executes a shell, or steals another client.

## Diagnostics and privacy

Diagnostics contain code, sanitized message, and optional endpoint ID. Routine output/logging may include request ID, timings, configured/success/failed counts, target counts, schema/producer versions, truncation, safe alias or salted endpoint hash, route outcome, and fallback reason.

Routine logs never include socket paths, labels, notes, titles, commands, cwd, PTYs, request JSON, or raw stderr. `no_server`/`not_configured` are expected conditions; stale, timeout, malformed, incompatible, untrusted, and command failures remain distinct.

## Validation

```sh
./scripts/validate.sh
./scripts/benchmark-switcher.sh
```

CI runs Rust contract/property tests, strict Lua decoder/model/adapter tests, isolated single/two-socket and PTY-client integration, tmux 3.2/3.6a compatibility, macOS/Linux jobs, release budgets, and nightly fuzz targets. Manual GUI evidence remains in [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md).
