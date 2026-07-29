# Formal independent release-code review

## Verdict

**Local implementation: acceptance-ready.** I found no unresolved blocker, high, or medium implementation defect in the current filesystem. The unified switcher is default-off, its local validation suite passes, its checked evidence is checksum-valid, and the reviewed safety invariants are implemented and regression-tested.

**Public/default-on release: not ready.** This is expected and correctly fail-closed. The external canary, publication/hosted-CI, formal checklist update, and one-minor-release retention gates remain open. They are rollout evidence gaps, not local code-acceptance failures.

## Findings

### Blocker

None.

### High

None.

### Medium

None.

### Confirmed safety and contract evidence

- **Canonical endpoint routing and trust:** `src/switcher/endpoint.rs:41-46` makes every tmux command use the canonical socket path that passed registration checks. `src/switcher/endpoint.rs:258-275` rejects symlink leaves, non-sockets, and wrong ownership; registration also checks the parent trust boundary. Discovery retains a bounded max-heap and emits the lexicographically smallest valid names independently of `read_dir` order (`src/switcher/endpoint.rs:318-365`). The passing suite includes canonical-path pinning, parent-symlink retargeting, ownership/type, configured-priority, and deterministic bounded-discovery regressions.
- **Typed, endpoint-qualified routing:** route commands are constructed as argument vectors, not shell text (`src/switcher/route.rs:367-390`); hostile labels and client names have exact-argv tests. New attachment resolves the endpoint from the trusted registry, compares server generation and exact target before and after the test barrier, re-verifies socket trust, and checks the deadline immediately before process creation (`src/switcher/route.rs:243-339`). Trust, timeout, schema, stale-target, stale-client, command-failure, partial-success, and success outcomes are exercised by the passing unit/contract/integration suites.
- **Local-domain attachment:** the adapter re-reads the callback pane's domain and rejects an empty or disallowed domain before spawning; the new tab is created in that exact proven-local domain (`wezterm/tmx_switcher/init.lua:261-275`). Tests passed for a non-default allowed local domain, remote invocation, and a pane that becomes remote before selection.
- **Bounded cleanup:** bounded child commands are placed in a private process group (`src/switcher/runner.rs:51-96`), are terminated on deadline/read/wait failure, and successful leaders also trigger descendant cleanup (`src/switcher/runner.rs:98-127,194-225`). Both runner-level and outer-supervisor descendant regressions passed. The interactive attachment at `src/switcher/route.rs:340-345` is intentionally user-owned and therefore not killed by the inventory/route deadline, consistent with the PRD's attachment semantics.
- **Degradation and repeated invocation:** native choices are collected first; tmux inventory is attempted only from an allowed local domain, and failures are converted to a concise selector status without removing native rows (`wezterm/tmx_switcher/init.lua:279-320`). The invocation guard is released on cancel/callback/presentation failure. All 31 Lua model/JSON/adapter tests passed, including malformed/oversized/incompatible/timeout inventory, native preservation, ordering, duplicate-key rejection, old-WezTerm fallback, and selector non-stacking.
- **Fail-closed evidence gate:** `scripts/validate-release-evidence.sh:22-30` requires an origin and a clean committed worktree in release mode; `:58-83` rejects unchecked rows and all named placeholders; `:84-106` checks links and default-off configuration; `:109-118` refuses success if any check failed. The local mode passed, while release mode failed for the expected external and worktree conditions.

## Documentation and checklist accuracy

No checked local implementation claim was found to materially overstate the evidence available in the repository. `./scripts/validate.sh` freshly reproduced the cited Rust and Lua results, and `./scripts/validate-release-evidence.sh --local` verified the evidence checksums, internal links, and default-off example.

The documentation does **not** claim that public/default-on release is complete: `docs/RELEASE_CHECKLIST.md:3,7-15,79-83` records the `WORKTREE` placeholder, missing canary identity/duration/incidents, absent publication destination/hosted run URLs, and unchecked rollout rows. `docs/PRD_TRACEABILITY.md:73-75` classifies canary, publication, and one-minor-release kill-switch retention as external gates. Its completion language at `docs/PRD_TRACEABILITY.md:79-85` should be read as local/content acceptance only, exactly as the following external-gate paragraph states.

One scope caveat is worth preserving: the prior artifact `artifacts/release-evidence/final-code-review.md:5-16` explicitly reviewed four targeted fixes rather than serving by itself as a whole-release acceptance. This formal review supplies the broader independent review, but the checklist still must be updated on the eventual committed release candidate; the current release validator correctly continues to reject its formal-acceptance placeholder.

## Commands and evidence inspected

- `./scripts/validate.sh` — passed: 57 library tests, 2 completion tests, 6 inventory-contract tests, 3 supervisor tests, 19 switcher integration tests with 1 release benchmark intentionally ignored, 2 tmux integration tests, and 31 Lua tests; fixture validation, formatting/lint/build stages also completed.
- `./scripts/validate-release-evidence.sh --local` — passed: six Markdown files validated, evidence checksums valid, default-off confirmed.
- `./scripts/validate-release-evidence.sh` — failed as designed: no origin, dirty/uncommitted worktree, three unchecked rollout rows, canary/publication/hosted-run/formal-review placeholders.
- `git diff --check` — passed.
- `git status --porcelain=v1` — worktree is intentionally dirty/uncommitted.
- `git diff --cached --name-only` — empty; no staged files.

## Residual risks / gates

1. Assign named, authorized canary users/hosts and record start/end/duration and incidents.
2. Commit the exact release candidate, configure/authorize its publication destination, and attach hosted CI, compatibility-job, and scheduled-fuzz URLs.
3. Update the checklist to reference this formal review and the immutable release commit/evidence.
4. Keep the kill switch through at least one published minor release before closing that historical gate.

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "No blocker/high/medium defect found; reviewed invariants and external-gate distinctions are cited with concrete file and line ranges throughout this report."
    }
  ],
  "changedFiles": [
    ".gitignore",
    "Cargo.lock",
    "Cargo.toml",
    "README.md",
    "config.example.toml",
    "docs/BUILD_AND_RUN.md",
    "docs/MACHINE_API.md",
    "docs/PRD_TRACEABILITY.md",
    "docs/RELEASE_CHECKLIST.md",
    "docs/WEZTERM_SWITCHER.md",
    "plans/unified-wezterm-tmx-switcher-prd.md",
    "scripts/benchmark-switcher.sh",
    "scripts/validate-release-evidence.sh",
    "scripts/validate.sh",
    "src/bin/tmx-supervisor.rs",
    "src/cli.rs",
    "src/commands/mod.rs",
    "src/config.rs",
    "src/lib.rs",
    "src/switcher/contract.rs",
    "src/switcher/endpoint.rs",
    "src/switcher/inventory.rs",
    "src/switcher/mod.rs",
    "src/switcher/parser.rs",
    "src/switcher/route.rs",
    "src/switcher/runner.rs",
    "src/tmux/formats.rs",
    "src/tmux/mod.rs",
    "tmux.example.conf",
    "wezterm/tmx_switcher/init.lua",
    "wezterm/tmx_switcher/json.lua",
    "wezterm/tmx_switcher/model.lua",
    ".github/workflows/ci.yml",
    ".github/workflows/nightly-fuzz.yml",
    "fuzz/fuzz_targets/inventory_json.rs",
    "fuzz/fuzz_targets/inventory_records.rs"
  ],
  "testsAddedOrUpdated": [
    "tests/inventory_contract.rs",
    "tests/lua/adapter_spec.lua",
    "tests/lua/json_spec.lua",
    "tests/lua/model_spec.lua",
    "tests/supervisor.rs",
    "tests/switcher_integration.rs",
    "tests/tmux_integration.rs",
    "tests/fixtures/inventory/v1/*",
    "tests/fixtures/route/v1/outcomes.json"
  ],
  "commandsRun": [
    {
      "command": "./scripts/validate.sh",
      "result": "passed",
      "summary": "All required local validation passed, including 57 library, 12 contract/supervisor/completion, 19 switcher integration, 2 tmux integration, and 31 Lua tests; one release-only benchmark test was intentionally ignored."
    },
    {
      "command": "./scripts/validate-release-evidence.sh --local",
      "result": "passed",
      "summary": "Links, checksums, and default-off configuration validated."
    },
    {
      "command": "./scripts/validate-release-evidence.sh",
      "result": "failed",
      "summary": "Expected fail-closed result for uncommitted worktree, no origin, unchecked canary/publication/minor-release rows, hosted-run gaps, and placeholders."
    },
    {
      "command": "git diff --check",
      "result": "passed",
      "summary": "No whitespace errors."
    },
    {
      "command": "git status --porcelain=v1 && git diff --cached --name-only",
      "result": "passed",
      "summary": "Current worktree changes enumerated; staged-file list was empty."
    }
  ],
  "validationOutput": [
    "57 Rust library tests passed",
    "2 completion, 6 inventory-contract, 3 supervisor, 19 switcher integration, and 2 tmux integration tests passed",
    "31 Lua switcher tests passed",
    "Local release evidence checksum/link/default-off validation passed",
    "Release-mode evidence validation failed closed for the explicitly pending gates"
  ],
  "residualRisks": [
    "External canary identities, duration, and incident record are pending",
    "Publication destination, immutable release commit, hosted CI/matrix/fuzz URLs are pending",
    "Kill-switch retention through one published minor release cannot yet be evidenced"
  ],
  "noStagedFiles": true,
  "diffSummary": "Large uncommitted unified-switcher change adding typed Rust inventory/routing/supervision, WezTerm Lua adapter/model/JSON handling, tests/fixtures/fuzz/CI, documentation, and release evidence; no staged files.",
  "reviewFindings": [
    "no blockers",
    "no high findings",
    "no medium findings",
    "local implementation acceptance-ready; public/default-on release remains blocked by explicit external gates"
  ],
  "manualNotes": "No files in the project were edited. This report was written only to the required external artifact path."
}
```
