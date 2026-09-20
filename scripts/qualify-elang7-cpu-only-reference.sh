#!/usr/bin/env bash
set -euo pipefail

EXPECTED_RUSTC='rustc 1.89.0'
SCHEMA='elastic-elang7-cpu-only-reference/v1'

workspace="$(git rev-parse --show-toplevel)"
cd "$workspace"
source_sha="$(git rev-parse HEAD)"
expected_source_sha="${ELANG7_EXPECTED_SOURCE_SHA:-}"
if [[ -z "$expected_source_sha" || "$source_sha" != "$expected_source_sha" ]]; then
  echo "ELANG7 CPU-only reference requires exact checked-out source SHA; expected=${expected_source_sha:-unset} observed=$source_sha" >&2
  exit 1
fi
if [[ -n "$(git status --porcelain)" ]]; then
  echo 'ELANG7 CPU-only reference requires a clean source worktree' >&2
  git status --short >&2
  exit 1
fi

if [[ "$(uname -s)" != Linux ]]; then
  echo 'ELANG7 CPU-only reference requires Linux' >&2
  exit 1
fi
if [[ "$(uname -m)" != x86_64 ]]; then
  echo "ELANG7 CPU-only reference requires x86_64 hosted CPU; observed $(uname -m)" >&2
  exit 1
fi
if [[ "$(rustc +1.89.0 --version)" != "$EXPECTED_RUSTC"* ]]; then
  echo "ELANG7 CPU-only reference requires Rust 1.89.0" >&2
  rustc +1.89.0 --version >&2 || true
  exit 1
fi

shopt -s nullglob
nvidia_nodes=(/dev/nvidia*)
shopt -u nullglob
if (( ${#nvidia_nodes[@]} > 0 )); then
  printf 'ELANG7 CPU-only reference refuses NVIDIA device nodes:' >&2
  printf ' %s' "${nvidia_nodes[@]}" >&2
  printf '\n' >&2
  exit 1
fi
if [[ -e /dev/kfd ]]; then
  echo 'ELANG7 CPU-only reference refuses AMD KFD compute device /dev/kfd' >&2
  exit 1
fi
if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi -L 2>/dev/null | grep -qE '^GPU '; then
  echo 'ELANG7 CPU-only reference refuses a visible NVIDIA compute GPU' >&2
  exit 1
fi

export CUDA_VISIBLE_DEVICES=''
export NVIDIA_VISIBLE_DEVICES='void'
export HIP_VISIBLE_DEVICES=''
export ROCR_VISIBLE_DEVICES=''
export ONEAPI_DEVICE_SELECTOR='cpu'
export SYCL_DEVICE_FILTER='cpu'

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
observer_file="$tmp_dir/linux-cpu-observations.txt"
benchmark_file="$tmp_dir/be13.csv"
report_file="$tmp_dir/elang7-cpu-only-reference.json"

cargo +1.89.0 run -q -p memorithm-elastic --example linux_cpu_environment | tee "$observer_file"

python3 - "$observer_file" <<'PY'
import math
import re
import sys

lines = open(sys.argv[1], encoding='utf-8').read().splitlines()
values = {}
unsupported = {}
source = None
for line in lines:
    match = re.search(r'^observation source=(\S+) signal=(\S+) value=(\S+)', line)
    if match:
        observed_source, signal, value = match.groups()
        if signal in values or signal in unsupported:
            raise SystemExit(f'duplicate CPU observation: {signal}')
        numeric = float(value)
        if not math.isfinite(numeric):
            raise SystemExit(f'non-finite supported CPU observation: {signal}')
        values[signal] = numeric
        source = source or observed_source
        if source != observed_source:
            raise SystemExit('CPU observations use multiple sources')
        continue
    match = re.search(r'^observation unsupported source=(\S+) signal=(\S+) reason=(.*)$', line)
    if match:
        observed_source, signal, reason = match.groups()
        if signal in values or signal in unsupported:
            raise SystemExit(f'duplicate CPU observation: {signal}')
        unsupported[signal] = reason
        source = source or observed_source
        if source != observed_source:
            raise SystemExit('CPU observations use multiple sources')

expected = {
    'linux-cpu-affinity-allowed-cpus',
    'linux-cpu-quota-cores',
    'linux-cpu-quota-unlimited',
    'linux-cpu-pressure-some-avg10',
    'linux-cpu-pressure-full-avg10',
}
if set(values) | set(unsupported) != expected:
    raise SystemExit(f'CPU observer signal set mismatch: values={values} unsupported={unsupported}')

affinity = values.get('linux-cpu-affinity-allowed-cpus')
if affinity is None or not affinity.is_integer() or affinity < 1:
    raise SystemExit(f'invalid CPU affinity observation: {affinity}')

unlimited = values.get('linux-cpu-quota-unlimited')
quota = values.get('linux-cpu-quota-cores')
if unlimited not in (0.0, 1.0):
    raise SystemExit(f'invalid quota-unlimited discriminator: {unlimited}')
if unlimited == 1.0:
    if quota is not None or 'linux-cpu-quota-cores' not in unsupported:
        raise SystemExit('unlimited quota must leave numeric quota explicitly unsupported')
elif quota is None or not math.isfinite(quota) or quota <= 0.0:
    raise SystemExit(f'finite quota must expose positive finite cores: {quota}')

for signal in ('linux-cpu-pressure-some-avg10', 'linux-cpu-pressure-full-avg10'):
    if signal in values:
        if not 0.0 <= values[signal] <= 1.0:
            raise SystemExit(f'{signal} lies outside [0,1]: {values[signal]}')
    elif signal not in unsupported:
        raise SystemExit(f'{signal} is neither supported nor explicitly unsupported')

print('ELANG7 real hosted CPU observation valid')
PY

cargo +1.89.0 bench -q -p memorithm-elastic-core --bench be13_portable -- \
  --warmup 100 --iterations 10000 | tee "$benchmark_file"

python3 - "$benchmark_file" <<'PY'
import csv
import math
import sys

rows = list(csv.DictReader(open(sys.argv[1], encoding='utf-8')))
expected = [
    'scalar_if_chain',
    'generic_bool_expr',
    'u64_compiled_guard',
    'multiword_guard',
    'batch_filter',
]
if [row['path'] for row in rows] != expected:
    raise SystemExit(f'BE13 path order mismatch: {[row["path"] for row in rows]}')
for row in rows:
    if row['result'] != 'True':
        raise SystemExit(f'BE13 semantic result is not True for {row["path"]}')
    if int(row['evaluations']) <= 0 or int(row['elapsed_ns']) <= 0:
        raise SystemExit(f'BE13 counters are non-positive for {row["path"]}')
    if not math.isfinite(float(row['ns_per_guard'])) or float(row['ns_per_guard']) <= 0.0:
        raise SystemExit(f'invalid BE13 ns_per_guard for {row["path"]}')
    if not math.isfinite(float(row['candidates_per_second'])) or float(row['candidates_per_second']) <= 0.0:
        raise SystemExit(f'invalid BE13 candidates_per_second for {row["path"]}')
print('ELANG7 BE13 CPU-only semantic smoke valid')
PY

python3 - "$SCHEMA" "$source_sha" "$observer_file" "$benchmark_file" "$report_file" <<'PY'
import csv
import json
import os
import platform
import re
import sys
from pathlib import Path

schema, source_sha, observer_path, benchmark_path, report_path = sys.argv[1:]
cpu_model = None
for line in Path('/proc/cpuinfo').read_text(errors='replace').splitlines():
    if line.lower().startswith('model name') and ':' in line:
        cpu_model = line.split(':', 1)[1].strip()
        break
mem_kib = None
for line in Path('/proc/meminfo').read_text(errors='replace').splitlines():
    if line.startswith('MemTotal:'):
        mem_kib = int(line.split()[1])
        break

values = {}
unsupported = {}
observation_source = None
for line in Path(observer_path).read_text(encoding='utf-8').splitlines():
    match = re.search(r'^observation source=(\S+) signal=(\S+) value=(\S+)', line)
    if match:
        observation_source = observation_source or match.group(1)
        values[match.group(2)] = float(match.group(3))
        continue
    match = re.search(r'^observation unsupported source=(\S+) signal=(\S+) reason=(.*)$', line)
    if match:
        observation_source = observation_source or match.group(1)
        unsupported[match.group(2)] = match.group(3)

bench_rows = []
for row in csv.DictReader(Path(benchmark_path).read_text(encoding='utf-8').splitlines()):
    bench_rows.append({
        'path': row['path'],
        'elapsed_ns': int(row['elapsed_ns']),
        'evaluations': int(row['evaluations']),
        'ns_per_guard': float(row['ns_per_guard']),
        'candidates_per_second': float(row['candidates_per_second']),
        'result': row['result'],
    })

report = {
    'schema': schema,
    'source_sha': source_sha,
    'qualification_kind': 'hosted-general-purpose-cpu-only-reference',
    'cost_claimed': False,
    'performance_claimed': False,
    'gpu_compute_required': False,
    'platform': platform.platform(),
    'machine': platform.machine(),
    'logical_cpus': os.cpu_count(),
    'cpu_model': cpu_model,
    'mem_total_kib': mem_kib,
    'cgroup_membership': Path('/proc/self/cgroup').read_text(errors='replace').strip().splitlines(),
    'elastic_cpu_observation': {
        'source': observation_source,
        'values': values,
        'unsupported': unsupported,
    },
    'be13_semantic_smoke': {
        'warmup': 100,
        'iterations': 10000,
        'rows': bench_rows,
    },
    'nvidia_device_nodes_present': False,
    'amd_kfd_present': False,
    'accelerator_environment': {
        'CUDA_VISIBLE_DEVICES': os.environ.get('CUDA_VISIBLE_DEVICES'),
        'NVIDIA_VISIBLE_DEVICES': os.environ.get('NVIDIA_VISIBLE_DEVICES'),
        'HIP_VISIBLE_DEVICES': os.environ.get('HIP_VISIBLE_DEVICES'),
        'ROCR_VISIBLE_DEVICES': os.environ.get('ROCR_VISIBLE_DEVICES'),
        'ONEAPI_DEVICE_SELECTOR': os.environ.get('ONEAPI_DEVICE_SELECTOR'),
        'SYCL_DEVICE_FILTER': os.environ.get('SYCL_DEVICE_FILTER'),
    },
}
Path(report_path).write_text(json.dumps(report, indent=2, sort_keys=True) + '\n', encoding='utf-8')
print(json.dumps(report, sort_keys=True))
PY

cargo +1.89.0 test -p memorithm-elastic \
  --test elang7_linux_cpu_observer_public \
  --test elang7_representation_kv_composition_public \
  --test elang7_transition_stability_public \
  --test be14h_thermal_energy_policy_public \
  --test be14h_thermal_energy_transaction_public \
  -- --nocapture

cargo +1.89.0 test -p elastic-downstream -- --nocapture

if [[ -n "$(git status --porcelain)" ]]; then
  echo 'ELANG7 CPU-only qualification modified tracked source state' >&2
  git status --short >&2
  exit 1
fi

echo 'ELANG7_CPU_ONLY_REFERENCE_QUALIFIED'
