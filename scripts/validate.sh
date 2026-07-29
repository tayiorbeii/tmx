#!/usr/bin/env bash
set -euo pipefail

command -v tmux >/dev/null || { echo "tmux is required for integration validation" >&2; exit 1; }
command -v lua >/dev/null || { echo "Lua is required for WezTerm adapter validation" >&2; exit 1; }

python3 - <<'PY'
import hashlib
import re
import tomllib
from pathlib import Path


def require(condition, message):
    if not condition:
        raise SystemExit(message)


msrv = tomllib.loads(Path('Cargo.toml').read_text(encoding='utf-8'))['package']['rust-version']
parts = msrv.split('.')
require(len(parts) in (2, 3) and all(part.isdigit() for part in parts), 'Cargo.toml rust-version must be numeric major.minor[.patch]')
ci_msrv = msrv if len(parts) == 3 else f'{msrv}.0'
require(f'Rust {msrv}+' in Path('README.md').read_text(encoding='utf-8'), 'README Rust floor differs from Cargo.toml')
require(f'# {msrv}+' in Path('docs/BUILD_AND_RUN.md').read_text(encoding='utf-8'), 'build-guide Rust floor differs from Cargo.toml')
require(f'toolchain: {ci_msrv}' in Path('.github/workflows/ci.yml').read_text(encoding='utf-8'), 'CI MSRV differs from Cargo.toml')
print(f'validated Rust {msrv} package/documentation/CI floor')

expected_jobs = {
    '.github/workflows/ci.yml': {'validate', 'msrv', 'tmux-version-matrix', 'release-budgets'},
    '.github/workflows/nightly-fuzz.yml': {'fuzz'},
}
expected_workflow_sha256 = {
    '.github/workflows/ci.yml': '9827b3eff06e242d036b0737a97dcbb0f5c28f2212c7da7c2efee76c41656011',
    '.github/workflows/nightly-fuzz.yml': '9ec3dac8062d8cb65ddd607858b3a6f1116751d9ba441a1802cb848b228ecf31',
}
expected_actions = {
    'actions/checkout': '11d5960a326750d5838078e36cf38b85af677262',
    'dtolnay/rust-toolchain': '2c7215f132e9ebf062739d9130488b56d53c060c',
}
workflow_paths = sorted(
    str(path) for suffix in ('*.yml', '*.yaml') for path in Path('.github/workflows').glob(suffix)
)
require(workflow_paths == sorted(expected_jobs), f'unreviewed workflow set: {workflow_paths}')
checkout_steps = []
toolchains = []
for path_string in workflow_paths:
    workflow_bytes = Path(path_string).read_bytes()
    actual_sha256 = hashlib.sha256(workflow_bytes).hexdigest()
    require(
        actual_sha256 == expected_workflow_sha256[path_string],
        f'{path_string}: content differs from the reviewed canonical workflow policy',
    )
    text = workflow_bytes.decode('utf-8')
    permission_headers = re.findall(r'(?m)^\s*permissions:', text)
    require(len(permission_headers) == 1, f'{path_string}: permissions must be declared once at workflow scope')
    require(re.search(r'(?m)^permissions:\n  contents: read\n(?:\n|$)', text), f'{path_string}: workflow token must be contents-read-only')

    raw_uses = re.findall(r'(?m)^\s*- uses:\s+(\S+)', text)
    parsed_uses = re.findall(r'(?m)^\s*- uses:\s+([^@\s]+)@([0-9a-f]{40})(?:\s+#.*)?$', text)
    require(len(raw_uses) == len(parsed_uses), f'{path_string}: every action must use an immutable full commit SHA')
    for action, revision in parsed_uses:
        require(action in expected_actions, f'{path_string}: unreviewed third-party action {action}')
        require(revision == expected_actions[action], f'{path_string}: unexpected revision for {action}')

    lines = text.splitlines()
    for index, line in enumerate(lines):
        if 'uses: actions/checkout@' in line:
            checkout_steps.append('\n'.join(lines[index:index + 4]))
        if 'uses: dtolnay/rust-toolchain@' in line:
            block = '\n'.join(lines[index:index + 6])
            match = re.search(r'(?m)^\s+toolchain: (\S+)$', block)
            require(match, f'{path_string}: Rust action must declare its toolchain explicitly')
            toolchains.append(match.group(1))

    jobs_text = text.split('\njobs:\n', 1)
    require(len(jobs_text) == 2, f'{path_string}: jobs mapping is missing')
    starts = list(re.finditer(r'(?m)^  ([a-z0-9-]+):\n', jobs_text[1]))
    discovered_jobs = {match.group(1) for match in starts}
    require(discovered_jobs == expected_jobs[path_string], f'{path_string}: unreviewed job set {discovered_jobs}')
    for index, match in enumerate(starts):
        end = starts[index + 1].start() if index + 1 < len(starts) else len(jobs_text[1])
        block = jobs_text[1][match.start():end]
        timeouts = re.findall(r'(?m)^    timeout-minutes: ([1-9][0-9]*)$', block)
        require(len(timeouts) == 1, f'{path_string}:{match.group(1)} must have exactly one positive timeout')

require(checkout_steps and all('persist-credentials: false' in block for block in checkout_steps), 'checkout must not persist credentials')
require(sorted(toolchains) == ['1.85.0', 'nightly', 'stable', 'stable', 'stable'], f'unreviewed Rust toolchains: {toolchains}')

ci = Path('.github/workflows/ci.yml').read_text(encoding='utf-8')
expected_tmux = {
    '3.2': '664d345338c11cbe429d7ff939b92a5191e231a7c1ef42f381cebacb1e08a399',
    '3.6a': 'b6d8d9c76585db8ef5fa00d4931902fa4b8cbe8166f528f44fc403961a3f3759',
}
tmux_entries = dict(re.findall(r"(?m)^          - tmux: '([^']+)'\n            sha256: ([0-9a-f]{64})$", ci))
require(tmux_entries == expected_tmux, f'unreviewed tmux source matrix: {tmux_entries}')
require("--proto '=https' --proto-redir '=https' --tlsv1.2" in ci, 'tmux download must require modern HTTPS')
require('sha256sum --check --strict -' in ci, 'tmux source checksum enforcement is missing')

nightly = Path('.github/workflows/nightly-fuzz.yml').read_text(encoding='utf-8')
require(nightly.count('cargo install cargo-fuzz --version 0.13.2 --locked') == 1, 'cargo-fuzz must be version-pinned and locked')
print(f'validated {len(workflow_paths)} byte-canonical least-privilege workflows, {len(expected_actions)} action pins, {len(expected_tmux)} tmux hashes, and pinned cargo-fuzz')
PY

validation_context="${TMX_VALIDATION_CONTEXT:-repository}"
case "$validation_context" in
  repository)
    if [[ -f .cargo_vcs_info.json && ! -e .git ]]; then
      echo "Cargo package self-tests require explicit TMX_VALIDATION_CONTEXT=package" >&2
      exit 2
    fi
    ./scripts/validate-release-evidence.sh --local
    ;;
  package)
    python3 - <<'PY'
import json
import re
from pathlib import Path


def require(condition, message):
    if not condition:
        raise SystemExit(message)


metadata_path = Path('.cargo_vcs_info.json')
require(metadata_path.is_file(), 'package validation requires Cargo-generated .cargo_vcs_info.json')
require(not Path('.git').exists(), 'package validation refuses repository metadata')
require(not Path('fuzz').exists(), 'package validation expects Cargo to exclude the nested fuzz workspace')
try:
    metadata = json.loads(metadata_path.read_text(encoding='utf-8'))
except (OSError, UnicodeError, json.JSONDecodeError) as error:
    raise SystemExit(f'invalid Cargo VCS metadata: {error}') from error
git = metadata.get('git')
require(isinstance(git, dict), 'Cargo VCS metadata must contain a git object')
require(re.fullmatch(r'[0-9a-f]{40}', str(git.get('sha1', ''))), 'Cargo VCS metadata must contain one lowercase 40-hex revision')
require(
    'dirty' not in git or isinstance(git['dirty'], bool),
    'Cargo VCS metadata dirty field must be boolean when present',
)
require(metadata.get('path_in_vcs') == '', 'Cargo VCS metadata path_in_vcs must identify the package root')
print('package self-test context validated from explicit caller mode and Cargo metadata structure')
PY
    echo "repository-candidate release-evidence gate: NOT RUN (explicit package self-test; this is not release validation)"
    ;;
  *)
    echo "TMX_VALIDATION_CONTEXT must be repository or package" >&2
    exit 2
    ;;
esac

cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
lua tests/lua/run.lua
python3 tests/release_evidence_validator.py
python3 - <<'PY'
import json
from pathlib import Path
files = sorted(Path('tests/fixtures').glob('**/*.json'))
if not files:
    raise SystemExit('machine-contract fixtures are missing')
for path in files:
    with path.open(encoding='utf-8') as handle:
        json.load(handle)
print(f'validated {len(files)} machine-contract JSON fixtures')
PY
