#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mode="release"
if (( $# == 1 )) && [[ "$1" == "--local" ]]; then
  mode="local"
elif (( $# != 0 )); then
  printf 'usage: %s [--local]\n' "$0" >&2
  exit 2
fi
export TMX_RELEASE_EVIDENCE_MODE="$mode"

failures=0
fail() {
  printf 'release-evidence error: %s\n' "$1" >&2
  failures=$((failures + 1))
}

if [[ "$mode" == "release" ]]; then
  if ! git remote get-url origin >/dev/null 2>&1; then
    fail "no origin remote is configured; publication and hosted-run evidence are impossible"
  fi

  if [[ -n "$(git status --porcelain)" ]]; then
    fail "the release candidate is not a clean committed worktree"
  fi
fi

if command -v shasum >/dev/null 2>&1; then
  checksum_command=(shasum -a 256 -c SHA256SUMS)
elif command -v sha256sum >/dev/null 2>&1; then
  checksum_command=(sha256sum -c SHA256SUMS)
else
  checksum_command=()
  fail "neither shasum nor sha256sum is available"
fi

if (( ${#checksum_command[@]} > 0 )) && ! (
  cd artifacts/release-evidence
  "${checksum_command[@]}" >/dev/null
); then
  fail "release evidence checksums do not match SHA256SUMS"
fi

python3 - <<'PY' || failures=$((failures + 1))
import hashlib
import os
import re
import stat
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from urllib.parse import unquote, urlparse

errors = []
release_mode = os.environ.get("TMX_RELEASE_EVIDENCE_MODE") == "release"
checklist = Path("docs/RELEASE_CHECKLIST.md").read_text(encoding="utf-8")

if release_mode:
    unchecked = [
        f"line {number}: {line}"
        for number, line in enumerate(checklist.splitlines(), 1)
        if "[ ]" in line
    ]
    if unchecked:
        errors.append("unchecked release rows:\n  " + "\n  ".join(unchecked))

placeholder_patterns = {
    "WORKTREE release commit placeholder": r"Commit: `WORKTREE`",
    "unassigned canary version": r"Canary version: not assigned",
    "unassigned canary users/hosts": r"Canary users/hosts: not authorized or assigned",
    "missing canary duration": r"Canary start/end/duration: not started",
    "missing canary incident log": r"Canary incident log: not started",
    "missing publication destination": r"Publication destination/release commit: no Git remote configured",
    "missing hosted CI run evidence": r"(?:Hosted CI run: not available|hosted CI run URL remains required)",
    "missing hosted compatibility-job evidence": r"(?:Hosted compatibility job: not available|hosted CI job URLs remain required)",
    "missing hosted scheduled-fuzz evidence": r"(?:Hosted scheduled-fuzz run: not available|scheduled-job URL remains required)",
    "missing canary incident artifact": r"Canary incident artifact: not available",
    "missing formal reviewer acceptance": r"Formal release sign-off: pending",
    "missing kill-switch retention release": r"Kill-switch retention release: not reached",
}
if release_mode:
    for description, pattern in placeholder_patterns.items():
        if re.search(pattern, checklist):
            errors.append(description)

checksum_manifest = Path("artifacts/release-evidence/SHA256SUMS")
evidence_root = checksum_manifest.parent
manifest_entries = []
for line in checksum_manifest.read_text(encoding="utf-8").splitlines():
    match = re.fullmatch(r"([0-9a-f]{64})  ([^/]+)", line)
    if not match:
        errors.append(f"invalid SHA256SUMS entry: {line!r}")
        continue
    manifest_entries.append(match.group(2))
listed_evidence = set(manifest_entries)
actual_evidence = {
    path.name for path in evidence_root.iterdir()
    if path.is_file() and path.name != checksum_manifest.name
}
if len(manifest_entries) != len(listed_evidence):
    errors.append("SHA256SUMS must list every evidence filename exactly once")
if listed_evidence != actual_evidence:
    missing = sorted(actual_evidence - listed_evidence)
    stale = sorted(listed_evidence - actual_evidence)
    errors.append(f"SHA256SUMS coverage mismatch; unlisted={missing}, missing={stale}")
evidence_count_match = re.search(r"Evidence integrity: all (\d+) ", checklist)
if not evidence_count_match or int(evidence_count_match.group(1)) != len(actual_evidence):
    errors.append("documented evidence-integrity count must match the exhaustive checksum manifest")


def field_value(label):
    match = re.search(rf"(?m)^- {re.escape(label)}:\s*(.+?)\s*$", checklist)
    return match.group(1).strip() if match else None


def git_output(*args):
    completed = subprocess.run(
        ["git", *args], check=False, capture_output=True, text=True
    )
    return completed.stdout.strip() if completed.returncode == 0 else None


SEMVER_PATTERN = re.compile(
    r"v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)


def parse_semver(value):
    match = SEMVER_PATTERN.fullmatch(value or "")
    if not match:
        return None
    prerelease = match.group(4)
    if prerelease and any(
        identifier.isdigit() and len(identifier) > 1 and identifier.startswith("0")
        for identifier in prerelease.split(".")
    ):
        return None
    return {
        "core": tuple(map(int, match.groups()[:3])),
        "prerelease": prerelease,
        "build": match.group(5),
    }


def parse_repo(value):
    scp_match = re.fullmatch(r"git@([^:]+):(.+)", value or "")
    if scp_match:
        host, path = scp_match.groups()
    else:
        parsed = urlparse(value or "")
        if parsed.scheme not in {"https", "ssh", "git"} or not parsed.hostname:
            return None
        host, path = parsed.hostname, parsed.path
    path = path.strip("/")
    if path.endswith(".git"):
        path = path[:-4]
    if not host or len(path.split("/")) < 2:
        return None
    return host.lower(), path


def evidence_url(value, label, origin_repo, required_suffix):
    urls = re.findall(r"https://[^\s)>`;]+", value or "")
    if len(urls) != 1:
        errors.append(f"{label} must contain exactly one HTTPS URL")
        return None
    parsed = urlparse(urls[0])
    try:
        port = parsed.port
    except ValueError:
        port = -1
    if (
        parsed.scheme != "https"
        or not parsed.hostname
        or parsed.username is not None
        or parsed.password is not None
        or port is not None
        or parsed.query
        or parsed.fragment
        or unquote(parsed.path) != parsed.path
    ):
        errors.append(
            f"{label} must use a canonical HTTPS URL without credentials, ports, "
            "encoding, query, or fragment"
        )
        return None
    path_parts = parsed.path.split("/")[1:]
    if (
        not parsed.path.startswith("/")
        or len(path_parts) < 2
        or any(part in {"", ".", ".."} for part in path_parts)
    ):
        errors.append(f"{label} must use a canonical path without empty or dot segments")
        return None
    if origin_repo:
        origin_host, origin_path = origin_repo
        expected_path = rf"/{re.escape(origin_path)}{required_suffix}"
        if parsed.hostname.lower() != origin_host or not re.fullmatch(
            expected_path, parsed.path
        ):
            errors.append(f"{label} must reference the configured origin repository")
            return None
    elif not re.search(required_suffix, parsed.path):
        errors.append(f"{label} has an invalid evidence URL shape")
        return None
    return f"https://{parsed.hostname.lower()}{parsed.path}"


def local_artifact(value, label):
    raw = (value or "").strip("`")
    path = Path(raw)
    if (
        not raw
        or path.is_absolute()
        or ".." in path.parts
        or path.parent != evidence_root
        or path.name not in listed_evidence
    ):
        errors.append(
            f"{label} must be a checksummed file directly under "
            "artifacts/release-evidence"
        )
        return None
    tracked = subprocess.run(
        ["git", "ls-files", "--error-unmatch", "--", str(path)],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    ).returncode == 0
    if release_mode and not tracked:
        errors.append(f"{label} must identify a tracked release artifact: {raw}")
        return None
    if not path.is_file() or not path.read_bytes():
        errors.append(f"{label} does not identify a non-empty file: {raw}")
        return None
    try:
        path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        errors.append(f"{label} must be UTF-8 text: {raw}")
        return None
    return path


if release_mode:
    origin_value = git_output("remote", "get-url", "origin")
    origin_repo = parse_repo(origin_value)
    if origin_value and not origin_repo:
        errors.append("origin must be an HTTPS or SSH repository URL")

    commit_value = field_value("Commit") or ""
    commit_match = re.fullmatch(r"`([0-9a-f]{40})`(?:\s+.*)?", commit_value)
    candidate_commit = commit_match.group(1) if commit_match else None
    if "WORKTREE" not in commit_value and not candidate_commit:
        errors.append("Commit must contain one full lowercase 40-hex candidate commit")
    if candidate_commit:
        if git_output("cat-file", "-e", f"{candidate_commit}^{{commit}}") is None:
            errors.append("Commit does not exist in the local repository")
        elif subprocess.run(
            ["git", "merge-base", "--is-ancestor", candidate_commit, "HEAD"],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        ).returncode != 0:
            errors.append("Commit must be an ancestor of the release-evidence commit")
        else:
            changed_output = subprocess.run(
                ["git", "diff", "--name-only", "-z", f"{candidate_commit}..HEAD"],
                check=True,
                capture_output=True,
            ).stdout
            changed_paths = [
                Path(raw.decode()) for raw in changed_output.split(b"\0") if raw
            ]
            disallowed_changes = [
                str(path)
                for path in changed_paths
                if path != Path("docs/RELEASE_CHECKLIST.md")
                and path.parent != evidence_root
            ]
            if disallowed_changes:
                errors.append(
                    "only the release checklist and release-evidence artifacts may "
                    f"change after the candidate commit: {sorted(disallowed_changes)}"
                )

    version_value = field_value("Canary version") or ""
    canary_semver = parse_semver(version_value)
    canary_version = canary_semver["core"] if canary_semver else None
    if "not assigned" not in version_value and not canary_semver:
        errors.append("Canary version must be a valid Semantic Version 2.0 value")

    identities_value = field_value("Canary users/hosts") or ""
    if "not authorized or assigned" not in identities_value:
        identities = [part.strip() for part in identities_value.split(",")]
        if not identities or any(
            re.fullmatch(r"[A-Za-z0-9._-]+@[A-Za-z0-9._-]+", item) is None
            for item in identities
        ):
            errors.append("Canary users/hosts must be comma-separated user@host aliases")

    duration_value = field_value("Canary start/end/duration") or ""
    if "not started" not in duration_value:
        duration_match = re.fullmatch(r"(.+?)\s+/\s+(.+?)\s+/\s+(\d+)\s+hours?", duration_value)
        if not duration_match:
            errors.append("Canary duration must be ISO-8601 start / end / whole hours")
        else:
            try:
                start = datetime.fromisoformat(duration_match.group(1).replace("Z", "+00:00"))
                end = datetime.fromisoformat(duration_match.group(2).replace("Z", "+00:00"))
                stated_hours = int(duration_match.group(3))
                if start.tzinfo is None or end.tzinfo is None or end <= start:
                    raise ValueError
                actual_hours = (end - start).total_seconds() / 3600
                if actual_hours != stated_hours:
                    errors.append("Canary stated duration does not match its start and end")
            except ValueError:
                errors.append("Canary start/end must be ordered timezone-bearing ISO-8601 values")

    publication_value = field_value("Publication destination/release commit") or ""
    if "no Git remote configured" not in publication_value:
        evidence_url(
            publication_value,
            "Publication destination/release commit",
            origin_repo,
            rf"/commit/{candidate_commit or '[0-9a-f]{40}'}$",
        )
        if candidate_commit and candidate_commit not in publication_value:
            errors.append("Publication destination must identify the candidate commit")

    hosted_urls = []
    hosted_specs = (
        ("Hosted CI run", r"/actions/runs/\d+$"),
        ("Hosted compatibility job", r"/actions/runs/\d+/job/\d+$"),
        ("Hosted scheduled-fuzz run", r"/actions/runs/\d+$"),
    )
    for label, suffix in hosted_specs:
        value = field_value(label) or ""
        if "not available" not in value:
            url = evidence_url(value, label, origin_repo, suffix)
            if url:
                hosted_urls.append(url)
    if len(hosted_urls) == 3 and len(set(hosted_urls)) != 3:
        errors.append("hosted CI, compatibility, and scheduled-fuzz evidence URLs must be distinct")

    incident_path = None
    incident_value = field_value("Canary incident artifact") or ""
    if "not available" not in incident_value:
        incident_path = local_artifact(incident_value, "Canary incident artifact")
        if incident_path and candidate_commit:
            incident_text = incident_path.read_text(encoding="utf-8")
            required_terms = [candidate_commit, version_value, identities_value]
            if any(term not in incident_text for term in required_terms):
                errors.append("Canary incident artifact must identify commit, version, and users/hosts")
            if not re.search(r"(?mi)^Incidents:\s*\S.+$", incident_text):
                errors.append("Canary incident artifact must record an incident disposition")

    signoff_path = None
    signoff_value = field_value("Formal release sign-off") or ""
    if "pending committed candidate" not in signoff_value:
        signoff_path = local_artifact(signoff_value, "Formal release sign-off")
        if signoff_path:
            signoff_text = signoff_path.read_text(encoding="utf-8")
            if candidate_commit and candidate_commit not in signoff_text:
                errors.append("Formal release sign-off must identify the candidate commit")
            for pattern, description in (
                (r"(?mi)^Reviewer:\s*\S.+$", "reviewer identity"),
                (r"(?mi)^Decision:\s*(?:accepted|approved)\s*$", "acceptance decision"),
            ):
                if not re.search(pattern, signoff_text):
                    errors.append(f"Formal release sign-off is missing {description}")
            date_match = re.search(r"(?mi)^Date:\s*(\d{4}-\d{2}-\d{2})\s*$", signoff_text)
            if not date_match:
                errors.append("Formal release sign-off is missing review date")
            else:
                try:
                    datetime.strptime(date_match.group(1), "%Y-%m-%d")
                except ValueError:
                    errors.append("Formal release sign-off has an invalid review date")
    if incident_path and signoff_path and incident_path == signoff_path:
        errors.append("Canary incident artifact and formal release sign-off must be distinct files")

    retention_value = field_value("Kill-switch retention release") or ""
    if "not reached" not in retention_value:
        retention_match = re.fullmatch(r"([^;\s]+);\s*(https://\S+)", retention_value)
        retention_semver = parse_semver(retention_match.group(1)) if retention_match else None
        if not retention_match or not retention_semver:
            errors.append(
                "Kill-switch retention release must be a valid Semantic Version 2.0 "
                "value and HTTPS URL"
            )
        else:
            retention_label, retention_url = retention_match.groups()
            retention_version = retention_semver["core"]
            if retention_semver["prerelease"]:
                errors.append("Kill-switch retention release must be a stable release")
            if canary_version and (
                retention_version[0] != canary_version[0]
                or retention_version[1] <= canary_version[1]
            ):
                errors.append(
                    "Kill-switch retention release must be a later minor release in "
                    "the same major series"
                )
            evidence_url(
                retention_url,
                "Kill-switch retention release",
                origin_repo,
                rf"/releases/tag/{re.escape(retention_label)}$",
            )

repo_path = Path.cwd().resolve()
markdown_files = [Path("README.md"), *sorted(Path("docs").glob("*.md"))]
for source in markdown_files:
    text = source.read_text(encoding="utf-8")
    targets = re.findall(r"\[[^]]*\]\(([^)]+)\)", text)
    targets.extend(re.findall(r"(?m)^\s*\[[^]]+\]:\s*(\S+)", text))
    for raw_target in targets:
        target = raw_target.strip()
        if target.startswith("<") and ">" in target:
            target = target[1:target.index(">")]
        elif " " in target:
            target = target.split(None, 1)[0]
        parsed_target = urlparse(target)
        if parsed_target.scheme or parsed_target.netloc or not parsed_target.path:
            continue
        relative_path = unquote(parsed_target.path)
        destination = (source.parent / relative_path).resolve()
        try:
            destination.relative_to(repo_path)
        except ValueError:
            errors.append(f"relative link escapes repository in {source}: {raw_target}")
            continue
        if not destination.exists():
            errors.append(f"broken relative link in {source}: {raw_target}")

config = Path("config.example.toml").read_text(encoding="utf-8")
switcher_match = re.search(r"(?ms)^\[switcher\]\s*(.*?)(?=^\[|\Z)", config)
if not switcher_match or not re.search(
    r"(?m)^enabled\s*=\s*false\s*$", switcher_match.group(1)
):
    errors.append("config.example.toml must keep [switcher].enabled = false before rollout")

records_corpus = Path("fuzz/corpus/inventory_records")
expected_seed_sha256 = {
    "delimiter-collision": "04d07f0784db7d53f8c2320ff0039b05e822e8c16292ddcbc979b95fe9dfdc2e",
    "hostile-controls": "cf27bd3fc1cc2ad95027fd05bc60235d837782edd27c1bd52b47a4d677813f10",
    "valid-session": "c24f0d3048f550c5131b322a81161aa449e05e571cc9284bd1127f124fa93a6e",
}
actual_seeds = (
    {path.name for path in records_corpus.iterdir() if path.is_file()}
    if records_corpus.is_dir()
    else set()
)
if actual_seeds != set(expected_seed_sha256):
    errors.append(
        "fuzz/corpus/inventory_records must contain exactly the three curated seeds; "
        f"found {sorted(actual_seeds)}"
    )
else:
    mismatched_seeds = [
        name
        for name, expected in expected_seed_sha256.items()
        if hashlib.sha256((records_corpus / name).read_bytes()).hexdigest() != expected
    ]
    if mismatched_seeds:
        errors.append(
            "curated fuzz seed content checksum mismatch: "
            + ", ".join(mismatched_seeds)
        )
generated_json_corpus = Path("fuzz/corpus/inventory_json")
if generated_json_corpus.exists() and any(generated_json_corpus.iterdir()):
    errors.append("generated fuzz/corpus/inventory_json output must not enter release evidence")

try:
    candidate_listing = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        check=False,
        capture_output=True,
    )
except FileNotFoundError:
    candidate_listing = None
if candidate_listing is not None and candidate_listing.returncode == 0:
    candidate_paths = [
        Path(raw.decode()) for raw in candidate_listing.stdout.split(b"\0") if raw
    ]
elif release_mode:
    candidate_paths = []
    errors.append("release candidate files could not be listed from Git")
else:
    excluded_directories = {
        ".git",
        ".pi",
        ".pi-subagents",
        "__pycache__",
        "target",
    }
    candidate_paths = []
    for path in repo_path.rglob("*"):
        relative = path.relative_to(repo_path)
        if any(part in excluded_directories for part in relative.parts):
            continue
        if relative.parts[:3] == ("fuzz", "corpus", "inventory_json"):
            continue
        if path.is_file() and not path.name.endswith((".pyc", ".pyo")):
            candidate_paths.append(relative)
    candidate_paths.sort()
for path in candidate_paths:
    if path.is_symlink():
        errors.append(f"release candidate must not contain symlink: {path}")
        continue
    if not path.is_file():
        continue
    data = path.read_bytes()
    if not data:
        errors.append(f"release candidate must not contain empty file: {path}")
        continue
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        continue
    if "\r\n" in text:
        errors.append(f"release candidate text must use LF line endings: {path}")

for script_path in (
    Path("scripts/benchmark-switcher.sh"),
    Path("scripts/validate-release-evidence.sh"),
    Path("scripts/validate.sh"),
):
    if not script_path.is_file() or not stat.S_IMODE(script_path.stat().st_mode) & 0o111:
        errors.append(f"required release script must be executable: {script_path}")

if errors:
    for error in errors:
        print(f"release-evidence error: {error}", file=sys.stderr)
    raise SystemExit(1)

print(
    f"validated {len(markdown_files)} markdown files, {len(candidate_paths)} candidate paths, "
    "the default-off rollout flag, and 3 curated fuzz seeds"
)
PY

if (( failures > 0 )); then
  printf '%s evidence is not ready; resolve every error above\n' "$mode" >&2
  exit 1
fi

if [[ "$mode" == "local" ]]; then
  printf 'local evidence is internally linked, checksum-valid, and default-off\n'
else
  printf 'release evidence is complete, internally linked, checksum-valid, committed, and publishable\n'
fi
