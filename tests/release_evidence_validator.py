#!/usr/bin/env python3
"""Exercise the release-evidence validator in isolated synthetic repositories."""

from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]
VALIDATOR = Path("scripts/validate-release-evidence.sh")
CURATED_FUZZ_SEEDS = {
    "delimiter-collision": b"$1|:tmx:v1:|bad|:tmx:v1:|split|:tmx:v1:|/tmp|:tmx:v1:|nope|:tmx:v1:|2|:tmx:v1:||:tmx:v1:|0|:tmx:v1:|1\n",
    "hostile-controls": b"$1|:tmx:v1:|\x1b[31mname\nnext|:tmx:v1:|/tmp|:tmx:v1:|1|:tmx:v1:|2|:tmx:v1:||:tmx:v1:|0|:tmx:v1:|1\n",
    "valid-session": b"$1|:tmx:v1:|work|:tmx:v1:|/tmp|:tmx:v1:|1|:tmx:v1:|2|:tmx:v1:||:tmx:v1:|0|:tmx:v1:|1\n",
}


def run(repo: Path, args: list[str], timeout: int = 60) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=repo,
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )


def require_success(result: subprocess.CompletedProcess[str], context: str) -> None:
    if result.returncode != 0:
        raise AssertionError(
            f"{context} failed with exit {result.returncode}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )


def commit(repo: Path, message: str) -> None:
    require_success(run(repo, ["git", "add", "-A"]), f"git add for {message}")
    require_success(run(repo, ["git", "commit", "-qm", message]), message)


def replace(repo: Path, relative: str, old: str, new: str) -> None:
    path = repo / relative
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise AssertionError(f"missing replacement anchor in {relative}: {old!r}")
    path.write_text(text.replace(old, new), encoding="utf-8")


def regenerate_manifest(repo: Path) -> int:
    evidence = repo / "artifacts/release-evidence"
    rows = [
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}"
        for path in sorted(evidence.iterdir(), key=lambda item: item.name)
        if path.is_file() and path.name != "SHA256SUMS"
    ]
    (evidence / "SHA256SUMS").write_text("\n".join(rows) + "\n", encoding="utf-8")
    return len(rows)


def candidate_files() -> list[Path]:
    listed = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=ROOT,
        check=False,
        capture_output=True,
    )
    if listed.returncode == 0:
        return [Path(raw.decode()) for raw in listed.stdout.split(b"\0") if raw]

    # Source archives and Git-free package rehearsals intentionally have no
    # repository metadata. Exclude only known generated/private directories.
    excluded_directories = {
        ".git",
        ".pi",
        ".pi-subagents",
        "__pycache__",
        "target",
    }
    files = []
    for path in ROOT.rglob("*"):
        relative = path.relative_to(ROOT)
        if any(part in excluded_directories for part in relative.parts):
            continue
        if relative.parts[:3] == ("fuzz", "corpus", "inventory_json"):
            continue
        if path.is_file() and not path.name.endswith((".pyc", ".pyo")):
            files.append(relative)
    return sorted(files)


def copy_candidate(destination: Path) -> None:
    for relative in candidate_files():
        source = ROOT / relative
        if source.is_file():
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)

    # The validator tests operate on temporary synthetic repositories. Give
    # those repositories an exact, self-contained policy fixture regardless of
    # whether the caller is a repository checkout or Cargo's source package.
    # scripts/validate.sh separately runs the real repository-candidate gate.
    target_corpus = destination / "fuzz/corpus/inventory_records"
    target_corpus.mkdir(parents=True, exist_ok=True)
    for name, expected in CURATED_FUZZ_SEEDS.items():
        target = target_corpus / name
        if target.exists() and target.read_bytes() != expected:
            raise AssertionError(f"repository curated fuzz seed differs from fixture: {name}")
        target.write_bytes(expected)


def build_valid_repository(destination: Path) -> tuple[str, int]:
    copy_candidate(destination)
    require_success(run(destination, ["git", "init", "-q"]), "git init")
    require_success(
        run(destination, ["git", "config", "user.name", "Release Validator Test"]),
        "git user.name",
    )
    require_success(
        run(
            destination,
            ["git", "config", "user.email", "validator@example.invalid"],
        ),
        "git user.email",
    )
    commit(destination, "synthetic candidate")
    candidate = run(destination, ["git", "rev-parse", "HEAD"]).stdout.strip()

    canary_version = "v0.1.0+canary.1"
    retention_version = "v0.2.0+retention.1"
    identities = "canary-user@canary-host"
    evidence = destination / "artifacts/release-evidence"
    (evidence / "synthetic-canary-incident.md").write_text(
        f"Candidate commit: {candidate}\n"
        f"Canary version: {canary_version}\n"
        f"Canary users/hosts: {identities}\n"
        "Incidents: none\n",
        encoding="utf-8",
    )
    (evidence / "synthetic-formal-signoff.md").write_text(
        "Reviewer: Synthetic Reviewer\n"
        "Date: 2026-07-22\n"
        "Decision: accepted\n"
        f"Candidate commit: {candidate}\n",
        encoding="utf-8",
    )

    checklist = destination / "docs/RELEASE_CHECKLIST.md"
    text = checklist.read_text(encoding="utf-8")
    replacements = {
        "- Commit: `WORKTREE` (replace with release commit)": f"- Commit: `{candidate}`",
        "- Canary version: not assigned": f"- Canary version: {canary_version}",
        "- Canary users/hosts: not authorized or assigned": f"- Canary users/hosts: {identities}",
        "- Canary start/end/duration: not started; acceptance duration requires release-owner input": (
            "- Canary start/end/duration: 2026-07-01T00:00:00Z / "
            "2026-07-02T00:00:00Z / 24 hours"
        ),
        "- Canary incident log: not started": (
            "- Canary incident log: zero incidents; see synthetic canary artifact"
        ),
        "- Publication destination/release commit: no Git remote configured; authorization required": (
            f"- Publication destination/release commit: "
            f"https://github.com/example/tmx/commit/{candidate}"
        ),
        "- Hosted CI run: not available; requires an authorized hosted run": (
            "- Hosted CI run: https://github.com/example/tmx/actions/runs/100"
        ),
        "- Hosted compatibility job: not available; requires an authorized hosted job": (
            "- Hosted compatibility job: "
            "https://github.com/example/tmx/actions/runs/101/job/200"
        ),
        "- Hosted scheduled-fuzz run: not available; requires an authorized scheduled run": (
            "- Hosted scheduled-fuzz run: https://github.com/example/tmx/actions/runs/102"
        ),
        "- Canary incident artifact: not available; requires an authorized canary": (
            "- Canary incident artifact: "
            "artifacts/release-evidence/synthetic-canary-incident.md"
        ),
        "- Formal release sign-off: pending committed candidate": (
            "- Formal release sign-off: "
            "artifacts/release-evidence/synthetic-formal-signoff.md"
        ),
        "- Kill-switch retention release: not reached": (
            f"- Kill-switch retention release: {retention_version}; "
            f"https://github.com/example/tmx/releases/tag/{retention_version}"
        ),
        "all 24 review/log/screenshot artifacts": (
            "all 26 review/log/screenshot artifacts"
        ),
        "hosted CI run URL remains required for release publication": (
            "hosted CI: https://github.com/example/tmx/actions/runs/100"
        ),
        "hosted CI job URLs remain required for release publication": (
            "hosted compatibility job: "
            "https://github.com/example/tmx/actions/runs/101/job/200"
        ),
        "scheduled-job URL remains required for release publication": (
            "scheduled fuzz: https://github.com/example/tmx/actions/runs/102"
        ),
        "formal release sign-off is deferred to the committed candidate": (
            "formal release sign-off is recorded in `synthetic-formal-signoff.md`"
        ),
        "- [ ]": "- [x]",
    }
    for old, new in replacements.items():
        if old not in text:
            raise AssertionError(f"missing checklist fixture anchor: {old!r}")
        text = text.replace(old, new)
    checklist.write_text(text, encoding="utf-8")

    evidence_count = regenerate_manifest(destination)
    if evidence_count != 26:
        raise AssertionError(f"expected 26 synthetic evidence files, found {evidence_count}")
    commit(destination, "synthetic release evidence")
    require_success(
        run(
            destination,
            ["git", "remote", "add", "origin", "https://github.com/example/tmx.git"],
        ),
        "add synthetic origin",
    )
    return candidate, evidence_count


def release_case(
    base: Path,
    root: Path,
    name: str,
    mutate: Callable[[Path], None],
    expected: str,
    *,
    local: bool = False,
    commit_mutation: bool = True,
) -> None:
    repo = root / name
    shutil.copytree(base, repo, symlinks=True)
    mutate(repo)
    if commit_mutation:
        commit(repo, f"negative case: {name}")
    args = [str(VALIDATOR), "--local"] if local else [str(VALIDATOR)]
    result = run(repo, args)
    if result.returncode != 1 or expected not in result.stderr:
        raise AssertionError(
            f"negative case {name!r} did not fail as expected for {expected!r}\n"
            f"exit={result.returncode}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="tmx-release-validator-tests-") as raw:
        work = Path(raw)
        git_free = work / "git-free-local"
        copy_candidate(git_free)
        require_success(
            run(git_free, [str(VALIDATOR), "--local"]),
            "direct Git-free local evidence validation",
        )

        tool_path = work / "tools-without-git"
        tool_path.mkdir()
        checksum_tool = shutil.which("shasum") or shutil.which("sha256sum")
        required_tools = {
            "bash": shutil.which("bash"),
            "dirname": shutil.which("dirname"),
            "python3": shutil.which("python3"),
            Path(checksum_tool).name if checksum_tool else "": checksum_tool,
        }
        if "" in required_tools or any(path is None for path in required_tools.values()):
            raise AssertionError("Git-free local validation prerequisites are unavailable")
        for name, source in required_tools.items():
            (tool_path / name).symlink_to(source)
        no_git_environment = os.environ.copy()
        no_git_environment["PATH"] = str(tool_path)
        no_git = subprocess.run(
            [str(git_free / VALIDATOR), "--local"],
            cwd=git_free,
            env=no_git_environment,
            text=True,
            capture_output=True,
            timeout=60,
            check=False,
        )
        require_success(no_git, "direct local evidence validation without Git executable")

        base = work / "valid"
        candidate, evidence_count = build_valid_repository(base)
        valid = run(base, [str(VALIDATOR)])
        require_success(valid, "complete synthetic release")

        def corrupt_all_curated_seeds(repo: Path) -> None:
            corpus = repo / "fuzz/corpus/inventory_records"
            for name in CURATED_FUZZ_SEEDS:
                path = corpus / name
                path.write_bytes(path.read_bytes() + b"corrupt")

        cases: list[tuple[str, Callable[[Path], None], str]] = [
            (
                "mutated-curated-seed-content",
                corrupt_all_curated_seeds,
                "curated fuzz seed content checksum mismatch: delimiter-collision, hostile-controls, valid-session",
            ),
            (
                "source-drift-after-candidate",
                lambda repo: replace(
                    repo,
                    "config.example.toml",
                    "selector_backend = \"fzf\"",
                    "selector_backend = \"builtin\"",
                ),
                "only the release checklist and release-evidence artifacts may change",
            ),
            (
                "query-string-hosted-url",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "https://github.com/example/tmx/actions/runs/102",
                    "https://github.com/example/tmx/actions/runs/102?attempt=2",
                ),
                "without credentials, ports, encoding, query, or fragment",
            ),
            (
                "credentialed-hosted-url",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "https://github.com/example/tmx/actions/runs/100",
                    "https://user@github.com/example/tmx/actions/runs/100",
                ),
                "without credentials, ports, encoding, query, or fragment",
            ),
            (
                "dot-segment-hosted-url",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "https://github.com/example/tmx/actions/runs/102",
                    "https://github.com/example/tmx/../../other/tmx/actions/runs/102",
                ),
                "canonical path without empty or dot segments",
            ),
            (
                "repeated-separator-hosted-url",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "https://github.com/example/tmx/actions/runs/102",
                    "https://github.com/example//tmx/actions/runs/102",
                ),
                "canonical path without empty or dot segments",
            ),
            (
                "invalid-leading-zero-semver",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "v0.1.0+canary.1",
                    "v00.1.0",
                ),
                "valid Semantic Version 2.0",
            ),
            (
                "invalid-prerelease-semver",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "v0.1.0+canary.1",
                    "v0.1.0-...",
                ),
                "valid Semantic Version 2.0",
            ),
            (
                "prerelease-retention",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "v0.2.0+retention.1",
                    "v0.2.0-rc.1",
                ),
                "must be a stable release",
            ),
            (
                "major-jump-retention",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "v0.2.0+retention.1",
                    "v1.0.0",
                ),
                "later minor release in the same major series",
            ),
            (
                "cross-repository-publication",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "https://github.com/example/tmx/commit/",
                    "https://github.com/other/tmx/commit/",
                ),
                "must reference the configured origin repository",
            ),
            (
                "duration-mismatch",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "/ 24 hours",
                    "/ 25 hours",
                ),
                "stated duration does not match",
            ),
            (
                "same-minor-retention",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "v0.2.0+retention.1",
                    "v0.1.1",
                ),
                "later minor release in the same major series",
            ),
            (
                "duplicate-hosted-run",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "https://github.com/example/tmx/actions/runs/102",
                    "https://github.com/example/tmx/actions/runs/100",
                ),
                "evidence URLs must be distinct",
            ),
            (
                "artifact-traversal",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "artifacts/release-evidence/synthetic-canary-incident.md",
                    "../synthetic-canary-incident.md",
                ),
                "must be a checksummed file directly under",
            ),
            (
                "shared-formal-artifact",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    "artifacts/release-evidence/synthetic-formal-signoff.md",
                    "artifacts/release-evidence/synthetic-canary-incident.md",
                ),
                "must be distinct files",
            ),
            (
                "malformed-candidate-commit",
                lambda repo: replace(
                    repo,
                    "docs/RELEASE_CHECKLIST.md",
                    f"- Commit: `{candidate}`",
                    "- Commit: `abc`",
                ),
                "one full lowercase 40-hex",
            ),
        ]
        for name, mutate, expected in cases:
            release_case(base, work, name, mutate, expected)

        def missing_incident_disposition(repo: Path) -> None:
            replace(
                repo,
                "artifacts/release-evidence/synthetic-canary-incident.md",
                "Incidents: none\n",
                "",
            )
            regenerate_manifest(repo)

        release_case(
            base,
            work,
            "missing-incident-disposition",
            missing_incident_disposition,
            "must record an incident disposition",
        )

        def invalid_signoff_date(repo: Path) -> None:
            replace(
                repo,
                "artifacts/release-evidence/synthetic-formal-signoff.md",
                "Date: 2026-07-22",
                "Date: 2026-99-99",
            )
            regenerate_manifest(repo)

        release_case(
            base,
            work,
            "invalid-signoff-date",
            invalid_signoff_date,
            "has an invalid review date",
        )

        def unchecksummed_signoff(repo: Path) -> None:
            manifest = repo / "artifacts/release-evidence/SHA256SUMS"
            rows = [
                row
                for row in manifest.read_text(encoding="utf-8").splitlines()
                if not row.endswith("  synthetic-formal-signoff.md")
            ]
            manifest.write_text("\n".join(rows) + "\n", encoding="utf-8")

        release_case(
            base,
            work,
            "unchecksummed-formal-artifact",
            unchecksummed_signoff,
            "must be a checksummed file directly under",
        )

        def ignored_untracked_signoff(repo: Path) -> None:
            source = repo / "artifacts/release-evidence/synthetic-formal-signoff.md"
            target = repo / "artifacts/release-evidence/untracked-signoff.md"
            shutil.copyfile(source, target)
            info_exclude = repo / ".git/info/exclude"
            with info_exclude.open("a", encoding="utf-8") as handle:
                handle.write("\nartifacts/release-evidence/untracked-signoff.md\n")
            replace(
                repo,
                "docs/RELEASE_CHECKLIST.md",
                "artifacts/release-evidence/synthetic-formal-signoff.md",
                "artifacts/release-evidence/untracked-signoff.md",
            )
            regenerate_manifest(repo)

        release_case(
            base,
            work,
            "ignored-untracked-formal-artifact",
            ignored_untracked_signoff,
            "must identify a tracked release artifact",
        )

        release_case(
            base,
            work,
            "escaping-markdown-link",
            lambda repo: (
                repo / "README.md"
            ).write_text(
                (repo / "README.md").read_text(encoding="utf-8")
                + "\n[escape](../../../../etc/passwd)\n",
                encoding="utf-8",
            ),
            "relative link escapes repository",
            local=True,
            commit_mutation=False,
        )

        print(
            "release-evidence validator tests passed: "
            f"1 Git-free local tree (with and without Git executable), "
            f"1 complete synthetic release, "
            f"22 fail-closed cases, {evidence_count} checksummed synthetic artifacts"
        )


if __name__ == "__main__":
    main()
