# Unified switcher PRD traceability

This is the requirement-by-requirement completion audit for
[`plans/unified-wezterm-tmx-switcher-prd.md`](../plans/unified-wezterm-tmx-switcher-prd.md).
It maps every normative PRD section to implementation and fresh evidence. Detailed commands,
version matrices, benchmark values, screenshots, and rollout state remain in
[`RELEASE_CHECKLIST.md`](RELEASE_CHECKLIST.md).

Status meanings:

- **Verified** — implemented and exercised by current local evidence.
- **External gate** — implementation and runbook exist, but completion requires rollout or
  publication outside this uncommitted repository.

Security/compatibility clarifications discovered during acceptance are explicit rather than hidden:

- PRD line 57 names `DefaultDomain`, while lines 29–30 require the invoking local mux domain.
  Because a user-configured default can be remote, the adapter re-reads the callback pane domain,
  requires it in `allowed_local_domains`, and spawns into that exact proven-local domain. Tests cover
  a non-default allowed domain and a callback pane that becomes remote before selection.
- PRD line 160 describes `name:` registration through `-L`. Registration retains that provenance,
  but every post-verification command is pinned with `-S` to the canonical trusted socket so a
  symlink or name cannot be retargeted after trust checks.
- Printable versioned record framing replaces control-byte framing because tmux 3.2–3.3 rewrites
  embedded control bytes on supported Linux environments; exact field counts preserve collision
  rejection.

## Product and UX

| PRD lines | Requirement group | Implementation | Verification | Status |
|---|---|---|---|---|
| 19–29 | One selector contains every native tab/pane and every trusted tmux session/window/pane; deterministic grouping/disambiguation; native activation, exact mapped-client reuse, or one local attachment; current target and cancel are no-ops. | `wezterm/tmx_switcher/model.lua`, `wezterm/tmx_switcher/init.lua` | `tests/lua/model_spec.lua`, `tests/lua/adapter_spec.lua`, `tests/switcher_integration.rs`; rendered macOS/X11/Wayland evidence | Verified |
| 31–40 | WezTerm owns native/UI effects; `tmx` owns endpoint trust, inventory, and mutation; only typed semantic JSON crosses the boundary; native choices survive every tmux failure mode. | `wezterm/tmx_switcher/`, `src/switcher/`, `src/bin/tmx-supervisor.rs` | Strict response tests, malformed/oversized/timeout/skew degradation tests, supervisor tests | Verified |
| 42–46 | Enumerate `wezterm.mux.all_windows()`, retain string identities, re-resolve before activation, avoid workspace creation on stale selection, separate activation from GUI focus. | `wezterm/tmx_switcher/init.lua` | `inactive_workspace_selection_activates_exact_pane_and_focuses_gui`, `stale_native_selection_reports_error_without_creating_workspace`, `focus_failure_after_activation_is_partial_and_not_retried`; real X11 focus evidence | Verified |
| 48–60 | One bounded read-only concurrent inventory; complete hierarchy and clients; healthy partial results; typed revalidation; exact mapped-client command; one local attachment with attach last; disclose shared active-window/pane state. | `src/switcher/inventory.rs`, `parser.rs`, `route.rs`, `runner.rs`; adapter and user docs | Real tmux multi-endpoint/client/attachment/race tests; `docs/WEZTERM_SWITCHER.md`; X11 exact attachment evidence | Verified |
| 61–65 | Deterministic pure merge, stable sanitization and endpoint qualification, no focusable separators, fresh invocation-local route map, no stacked selectors or leaked processes. | `wezterm/tmx_switcher/model.lua`, `init.lua`; `tmx-supervisor` | Model permutation/collision/sanitizer tests, repeat-invocation test, runner/supervisor reaping tests | Verified |
| 69–132 | All 44 user stories: fuzzy/non-fuzzy keyboard behavior, global native reachability, searchable accessible labels, complete multi-socket inventory, mapped reuse versus local attachment, stale/partial degradation, actionable held failures, additive compatibility, and old-WezTerm capability fallback. | Switcher Rust/Lua modules, example configuration, machine/docs surfaces | 58 Rust library tests, real integration suites, 31 Lua tests, compatibility matrices, rendered fuzzy/non-fuzzy/native-only/fallback selectors | Verified |

## Architecture, identity, and safety

| PRD lines | Requirement group | Implementation | Verification | Status |
|---|---|---|---|---|
| 138–149 | Six deep modules: endpoint registry, inventory service, inventory contract, route executor, pure choice model, thin adapter. | `src/switcher/{endpoint,inventory,contract,route,parser,runner}.rs`, `wezterm/tmx_switcher/{model,init}.lua` | Public-output tests and final independent filesystem review | Verified |
| 151–157 | Single authority per control plane; opaque invocation IDs; labels/stderr never become identity/code; argument vectors only; searched text is visible/searchable. | Typed DTOs and route maps; no shell execution path | Hostile metadata/argv tests and source audit; no `eval`/shell route construction | Verified |
| 158–167 | Explicit default/name/path selectors; deterministic canonical endpoint identity; configured aliases collapse; bounded deterministic discovery; effective-UID socket/parent/type/symlink checks; device/inode/UID revalidation; no arbitrary/group-shared endpoints. | `src/switcher/endpoint.rs`; every command pinned to verified canonical `-S` path | Endpoint unit tests including retargeted-parent symlink, trust rejection, configured priority, 32-endpoint cap, deterministic bounded discovery; live socket replacement tests | Verified |
| 168–186 | Versioned inventory envelope, endpoint-qualified hierarchy, generation, capabilities, limits, stable ordering, typed partial diagnostics, bounded strings/counts/output/deadline, additive-minor compatibility. | `src/switcher/contract.rs`, `inventory.rs`, `parser.rs` | Contract fixtures and tests, parser tests/fuzz, Lua strict validator tests, hostile/truncation fixtures | Verified |
| 187–194 | Full client fingerprint and deterministic local TTY/domain join; reject ambiguous, stale, remote, external-only, and reused-PTY matches. | Contract/client DTOs, route revalidation, Lua model join | Real PTY client deletion/reuse tests; remote/ambiguous model tests; exact unrelated-client state matrix | Verified |
| 195–208 | Versioned typed route contract and complete outcome set; request/plan identity; exact one-command mapped routing; validated session/window/pane attachment with attach last; postcondition `partial_success`; no detach/steal options. | `src/switcher/contract.rs`, `route.rs`; strict adapter response validation | Route outcome fixture, exact argv tests, session/window/pane integration, current no-op, postcondition race, held failure tests | Verified |
| 209–214 | Native identity includes domain/window/tab/pane; dedupe only proven identity; equal labels stay distinct; capability checks preserve existing bindings; independent upgrades degrade only augmentation. | Lua model/adapter | Collision/equal-label tests, capability-gap test, malformed/skew/timeout degradation suite | Verified |

## Test and acceptance policy

| PRD lines | Requirement group | Evidence | Status |
|---|---|---|---|
| 215–227 | Assert observable outputs; canonical fixtures normalize only volatile fields; isolated tmux runtime; real PTYs and bounded polling; deterministic barriers; pure Lua model and adapter spies; rendered integration separate from pure tests. | `tests/fixtures/`, `tests/support/mod.rs`, `tests/switcher_integration.rs`, Lua suites, release screenshots/logs | Verified |
| 228–246 | Required unit/contract/property/fuzz, multi-socket, multi-client, stale/race, exact argv, adapter, compatibility, performance, and manual platform matrix. | Local Rust/Lua suites; fuzz corpora/runs; Linux tmux 3.2/3.6a matrix; macOS/X11/Wayland acceptance matrix | Verified |
| 247–255 | PR, integration, safety, behavior, degradation, performance, and release gates. | `./scripts/validate.sh`; `./scripts/benchmark-switcher.sh`; Linux validation logs; rendered platform evidence; `artifacts/release-evidence/SHA256SUMS` | Verified |

## Delivery, compatibility, resources, and privacy

| PRD lines | Requirement group | Implementation/evidence | Status |
|---|---|---|---|
| 278–289 | Tracer delivery phases from contracts through registry, inventory, routing, hardening, opt-in canary, and default-on. | All implementation-phase outputs and local hardening evidence exist; the real canary, default-on transition, publication, and one-minor-release retention remain tracked as external gates below. | **Local implementation verified; rollout pending** |
| 291–298 | WezTerm floor `20230408-112425-69ae8472`, tmux floor 3.2, additive machine schemas, independent upgrades/downgrades, native fallback on skew/failure. | Documented compatibility matrix; tmux 3.2/3.6a Linux runs; capability and schema tests | Verified |
| 299–310 | Invocation, inventory, route, count, byte, depth, concurrency, process-group, zombie, and benchmark budgets. | Contract/config limits, bounded runner and independent supervisor; benchmark p50/p95/max; 0 zombies | Verified |
| 311–316 | Opaque normal logs, bounded sanitized diagnostics, no terminal-content capture, no socket-path leakage, JSON-only machine stdout, distinct expected/error states. | Redaction and bounded-diagnostic code/tests; duplicate-key/raw-UTF-8 strict JSON parser; privacy docs | Verified |

## Rollout and publication

| PRD lines | Requirement | Evidence/current state | Status |
|---|---|---|---|
| 319, 321 | Ship disabled, preserve the native-only emergency binding and fallback/backoff behavior, and support configuration-only rollback without killing user-owned tabs. | Default-off config, emergency binding, rendered fallback, rollback rehearsal and documentation | Verified for these local behavior clauses; canary/default-on and publication are tracked separately below |
| 319, 330 | Opt-in canary before default-on and record its duration in the release checklist. The release-evidence policy additionally requires publication-safe named users/hosts and an incident record. | Runbook/checklist is ready; no authorized real canary identity, host, or duration has been supplied. | **External gate** |
| 323–330 | Before default-on, publish compatibility, configuration/trust, operator runbook, machine contract/fixtures, troubleshooting/privacy/rollback, and release evidence. | All documents and checksum-verified evidence exist locally, and a whole-release review found the local implementation acceptance-ready. The repository still has no publication destination. Hosted CI/matrix/fuzz URLs and formal sign-off on a committed candidate are adopted release-evidence controls described below, not verbatim PRD clauses. | **External gate** |
| 286–287 | Retain native fallback/rollback through the opt-in canary and keep the kill switch for at least one minor release. | Kill switch is implemented and tested; elapsed released-version history cannot be created in the current uncommitted worktree. | **External gate** |

The PRD directly requires the rollout sequence, pre-default-on publication, canary-duration evidence,
and one-minor-release kill-switch retention. The exact origin-bound hosted URLs, clean committed-candidate
identity, and formal-sign-off artifact are fail-closed evidence controls adopted by
[`RELEASE_CHECKLIST.md`](RELEASE_CHECKLIST.md), not verbatim PRD clauses. A release owner must either
fulfill and authorize those controls or explicitly revise the release policy; until then they remain gates.

## Completion conclusion

All implementation, local behavior, safety, compatibility, performance, documentation-content,
and rendered-platform requirements are verified. The durable goal must remain active until the
three external rollout/publication rows above are evidenced. Required unblocking input is:

1. named canary users/hosts, authorization, and the duration/incident-recording criterion;
2. a Git remote or other publication destination plus authorization to commit/push and collect the
   hosted CI, compatibility-matrix, and scheduled-fuzz run URLs;
3. formal reviewer sign-off against the eventual committed release candidate; and
4. release history proving the kill switch survived at least one minor release.
