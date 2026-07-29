#!/usr/bin/env bash
set -euo pipefail
command -v tmux >/dev/null
command -v lua >/dev/null
cargo build --locked --release --quiet

root="$(mktemp -d /tmp/tmx-bench.XXXXXX)"
name="bench$$"
config="$root/config.toml"
cleanup() { TMUX_TMPDIR="$root" tmux -L "$name" kill-server >/dev/null 2>&1 || true; rm -rf "$root"; }
trap cleanup EXIT
TMUX_TMPDIR="$root" tmux -L "$name" -f /dev/null new-session -d -s benchmark -c /tmp
socket="$root/tmux-$(id -u)/$name"
cat >"$config" <<EOF
[switcher]
enabled = true
deadline_ms = 400
endpoint_soft_timeout_ms = 150
[[switcher.endpoints]]
selector = "path:$socket"
alias = "benchmark"
EOF

TMX_CONFIG="$config" target/release/tmx inventory --schema 1 --json >/dev/null
TMX_CONFIG="$config" python3 - <<'PY'
import os, statistics, subprocess, time
cmd=['target/release/tmx','inventory','--schema','1','--json','--deadline-ms','400']
env={**os.environ,'TMX_CONFIG':os.environ['TMX_CONFIG']}
samples=[]
for _ in range(100):
    start=time.perf_counter_ns()
    result=subprocess.run(cmd,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
    assert result.returncode == 0, result.stderr.decode()
    samples.append((time.perf_counter_ns()-start)/1_000_000)
samples.sort()
p95=samples[int(len(samples)*.95)-1]
processes=subprocess.check_output(['ps','-axo','stat=,command='],text=True)
zombies=[line for line in processes.splitlines() if line.lstrip().startswith('Z') and 'target/release/tmx' in line]
print(f'inventory_100_invocations p50={statistics.median(samples):.3f}ms p95={p95:.3f}ms max={max(samples):.3f}ms zombies={len(zombies)}')
assert p95 <= 100, f'inventory p95 {p95:.3f}ms exceeds 100ms'
assert not zombies, f'leaked zombie tmx processes: {zombies}'
PY
lua tests/lua/benchmark.lua
cargo test --locked --release --test switcher_integration route_execution_p95_is_within_budget -- --ignored --nocapture
