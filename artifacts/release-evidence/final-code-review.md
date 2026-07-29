# Final code review

## Findings

No unresolved blocker, high, or medium defects found in the four targeted fixes.

## Code verdict

**PASS.** Direct inspection of the current filesystem confirms:

- `RegisteredEndpoint::tmux_prefix()` emits canonical `-S <socket_path>` arguments; inventory and both route paths call it. Inventory diagnostics redact both original and canonical path spellings, and the parent-symlink retarget regression test is present.
- The WezTerm attachment path reads `spawn_domain` from the callback pane, validates it with `is_allowed_local_domain`, and spawns with `domain=spawn_domain`. Tests cover a non-default allowed domain and a pane changed to a remote domain before selection.
- Successful-leader process-group cleanup remains implemented and covered by runner and supervisor descendant tests.
- Discovery retains a bounded max-heap of the lexicographically smallest candidates, with order-independence coverage.

The targeted Rust tests and all 31 Lua switcher tests passed.

## Focused follow-up after release-gate and route hardening

A fresh read-only local review of the current filesystem found **no remaining blocker, high, or medium findings**. This is implementation-review evidence, not formal sign-off against a committed release candidate.

The follow-up verified:

- post-candidate changes are restricted to the checklist and direct release-evidence files;
- incident and sign-off artifacts must be tracked, checksummed, direct evidence files;
- hosted/publication URLs are canonical, repository-bound, and reject credentials, ports, encoding, queries, fragments, dot segments, and repeated separators;
- Semantic Version 2.0 build metadata and stable same-major later-minor retention boundaries have durable coverage;
- direct local validation works without repository metadata and without a Git executable;
- mapped-client routing performs one final full identity snapshot, then an output/deadline-bounded client-only postcondition query;
- the postcondition requires the complete endpoint/generation/name/TTY/PID/creation-time/UID fingerprint before target equality, including a same-name/PTY replacement regression;
- behavioral fixtures use extended deadlines only to isolate semantics from loaded-host scheduling, while a separate non-ignored live route test omits the deadline flag and exercises the product's 250 ms CLI default.

Fresh full validation passed with 58 Rust library tests, 20 live switcher integration tests plus one release-benchmark test ignored by default, all other Rust suites, 31 Lua tests, and the release-validator suite's Git-free success paths plus 22 fail-closed cases.

## Focused governance/provenance follow-up

A fresh read-only advisory review found **no blocker, high, or medium findings** after requirement provenance was clarified. This remains local review evidence, not formal sign-off against a committed release candidate.

The follow-up verified that:

- the traceability matrix cites the PRD's direct opt-in-canary/default-on sequence, pre-default-on publication, canary-duration record, and one-minor-release kill-switch retention requirements;
- origin-bound hosted URLs, clean committed-candidate identity, publication-safe canary aliases, direct incident/sign-off artifacts, and URL/commit coupling are accurately labeled as adopted fail-closed release-evidence controls rather than verbatim PRD clauses;
- those adopted controls remain enforceable unless a release owner explicitly revises policy; and
- the delivery-phase traceability status distinguishes completed local implementation/hardening from the still-pending real canary, default-on transition, publication, and elapsed retention history.

## Packaging metadata and MSRV follow-up

A final packaging audit found two stale pre-switcher claims: README/build documentation still advertised Rust 1.70, although Cargo 1.70 cannot parse the shipped v4 lockfile, and README described a single executable despite the shipped `tmx-supervisor` companion. Both claims were corrected without changing runtime behavior.

The package now declares `rust-version = "1.85"`, its description includes the bounded WezTerm/tmux switcher, README/build documentation describes both executables and the same Rust floor, and CI has a dedicated Rust 1.85.0 all-target/all-feature compile job. An isolated Rust 1.85.0 toolchain compiled the locked workspace successfully and was then removed. Local validation checks that Cargo metadata, both documentation surfaces, and CI remain synchronized. This is local packaging-review evidence, not formal committed-candidate sign-off.

## Workflow-policy enforcement follow-up

A focused read-only local review found two medium fail-open paths in the initial durable workflow policy: valid YAML using alternate spacing or comment masking could evade regex/string matching, and Python `assert` gates disappear when `PYTHONOPTIMIZE=1`. Both findings are closed.

The final policy verifies exact SHA-256 digests for both reviewed workflow byte streams before retaining detailed action, permission, job, timeout, toolchain, tmux-source, transport, checksum, and fuzzer checks. Every plain assertion in the inline policy and fixture validators was replaced with explicit fail-closed control flow. The hosted validation job now permanently runs the full suite with `PYTHONOPTIMIZE=1`.

Fresh negative probes under optimized Python rejected the reviewer's alternate-spacing YAML attack, comment-masked floating `cargo-fuzz` attack, and an MSRV-documentation drift. The complete optimized-mode validation then passed 58 Rust library tests, all integration suites including 20 live switcher tests plus one intentionally ignored release benchmark, 31 Lua tests, the release-validator suite, and seven machine-contract fixtures. `shellcheck`, `actionlint` 1.7.7, local evidence validation, and `git diff --check` also passed. A narrow closure review independently matched both workflow digests, counted zero plain assertions in the scoped validator, marked both medium findings closed, and found no blocker, high, or medium regression. This remains local advisory evidence, not formal committed-candidate release sign-off.

## Exact-crate validation follow-up

A fresh extraction of the deterministic 103-file crate found that Cargo intentionally excludes the nested `fuzz` workspace package from the main crate. The crate therefore cannot satisfy the repository-candidate evidence gate, while the regression suite still needs complete synthetic repositories to test that gate's behavior.

A narrow advisory review found that the first workaround was fail-open: a forgeable `.cargo_vcs_info.json` filename selected a relaxed branch, name-only synthetic seeds could satisfy a filename-only production check, and the extracted crate's `validate.sh` passed even though its direct local evidence gate failed. The workaround was removed. `scripts/validate.sh` now runs `validate-release-evidence.sh --local` by default. Explicit `TMX_VALIDATION_CONTEXT=package` validates Cargo metadata structure and the expected package layout, accepts Cargo's clean-package omission of the optional `git.dirty` member while requiring it to be Boolean when present, clearly prints that the repository gate was not run and that package self-testing is not release validation, and then runs only package-applicable validation. The regression harness unconditionally gives its temporary synthetic repositories exact curated fixture bytes and no longer infers package provenance.

The production local-evidence gate now pins SHA-256 for `delimiter-collision`, `hostile-controls`, and `valid-session`; one negative case corrupts all three while retaining their names and requires all three mismatches. Repository execution directly validates those real seeds before the synthetic 22-case suite. Extracting the exact crate and running `TMX_VALIDATION_CONTEXT=package PYTHONOPTIMIZE=1 ./scripts/validate.sh` passes 58 library tests, every integration suite including 20 live switcher tests plus one intentionally ignored release benchmark, 31 Lua tests, the release-validator's Git-free and 22 negative cases, and seven machine-contract fixtures. Direct `validate-release-evidence.sh --local` fails there by design. A fresh narrow advisory closure review independently matched the three seed digests, reran the 22-case validator suite and direct repository gate, and marked the prior one high, two medium, and one low findings closed with no new scoped finding. This is explicitly labelled local package-integrity evidence, not repository release validation or formal committed-candidate sign-off.
