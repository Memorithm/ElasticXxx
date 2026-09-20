#!/usr/bin/env bash
set -euo pipefail

EXPECTED_RUSTC='rustc 1.89.0'
SCHEMA='elastic-elang7-cpu-only-reference/v1'

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

# Keep common accelerator selectors explicitly CPU-only even though ElasticXxx
# does not require these runtimes for the qualified path.
export CUDA_VISIBLE_DEVICES=''
export NVIDIA_VISIBLE_DEVICES='void'
export HIP_VISIBLE_DEVICES=''
export ROCR_VISIBLE_DEVICES=''
export ONEAPI_DEVICE_SELECTOR='cpu'
export SYCL_DEVICE_FILTER='cpu'

python3 - "$SCHEMA" <<'PY'
import json
import os
import platform
import sys
from pathlib import Path

schema = sys.argv[1]
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
cgroup = Path('/proc/self/cgroup').read_text(errors='replace').strip().splitlines()
report = {
    'schema': schema,
    'qualification_kind': 'hosted-commodity-cpu-only-reference',
    'cost_claimed': False,
    'performance_claimed': False,
    'gpu_compute_required': False,
    'platform': platform.platform(),
    'machine': platform.machine(),
    'logical_cpus': os.cpu_count(),
    'cpu_model': cpu_model,
    'mem_total_kib': mem_kib,
    'cgroup_membership': cgroup,
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
print(json.dumps(report, sort_keys=True))
PY

# Public-facade-only ELANG7 surfaces. These paths deliberately require no CUDA,
# WGPU, ROCm, accelerator device, or model-serving backend.
cargo +1.89.0 test -p memorithm-elastic \
  --test elang7_linux_cpu_observer_public \
  --test elang7_representation_kv_composition_public \
  --test elang7_transition_stability_public \
  --test be14h_thermal_energy_policy_public \
  --test be14h_thermal_energy_transaction_public \
  -- --nocapture

# The downstream crate has only the public `elastic` dependency and exercises
# RAM/storage budgets, immutable safety reservations, grouped EIR, model-profile
# binding and the other facade contracts without implementation-crate imports.
cargo +1.89.0 test -p elastic-downstream -- --nocapture

echo 'ELANG7_CPU_ONLY_REFERENCE_QUALIFIED'
