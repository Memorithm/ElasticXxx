#!/usr/bin/env bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"

if [[ -n "$(git status --porcelain)" ]]; then
  echo "BE13 evidence collection requires a clean worktree" >&2
  exit 2
fi

OUT_DIR=${1:-}
if [[ -z "$OUT_DIR" ]]; then
  echo "usage: $0 OUTPUT_DIR" >&2
  exit 2
fi

REPETITIONS=${BE13_REPETITIONS:-30}
WARMUP=${BE13_WARMUP:-10000}
ITERATIONS=${BE13_ITERATIONS:-500000}
CPU=${BE13_CPU:-0}

for pair in \
  "BE13_REPETITIONS:$REPETITIONS" \
  "BE13_WARMUP:$WARMUP" \
  "BE13_ITERATIONS:$ITERATIONS"; do
  name=${pair%%:*}
  value=${pair#*:}
  if ! [[ "$value" =~ ^[1-9][0-9]*$ ]]; then
    echo "$name must be a positive integer" >&2
    exit 2
  fi
done
if [[ -n "$CPU" ]] && ! [[ "$CPU" =~ ^[0-9]+$ ]]; then
  echo "BE13_CPU must be an integer CPU id or empty to disable affinity" >&2
  exit 2
fi

mkdir -p "$OUT_DIR"
if [[ -n "$(find "$OUT_DIR" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
  echo "output directory must be empty: $OUT_DIR" >&2
  exit 2
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
SOURCE_SHA=$(git rev-parse HEAD)
COLLECTED_AT_UTC=$(date -u +'%Y-%m-%dT%H:%M:%SZ')
COLLECTOR_SHA256=$(sha256sum "${BASH_SOURCE[0]}" | awk '{print $1}')
FREQ_BEFORE=$(readlink -f /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq >/dev/null 2>&1 && tr -d '\0\n' </sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq || printf 'unavailable')

BUILD_JSON="$TMP/build.jsonl"
if ! cargo +1.89.0 bench -p elastic-core --bench be13_portable --no-run \
  --message-format=json >"$BUILD_JSON" 2>"$TMP/build.stderr"; then
  cat "$TMP/build.stderr" >&2
  exit 4
fi

BENCH_BIN=$(python3 - "$BUILD_JSON" <<'PY'
import json
import sys

found = []
for line in open(sys.argv[1], encoding="utf-8"):
    try:
        item = json.loads(line)
    except json.JSONDecodeError:
        continue
    if (
        item.get("reason") == "compiler-artifact"
        and item.get("target", {}).get("name") == "be13_portable"
        and item.get("executable")
    ):
        found.append(item["executable"])
if len(found) != 1:
    raise SystemExit(f"expected one be13_portable executable, found {len(found)}")
print(found[0])
PY
)

RAW="$OUT_DIR/raw.csv"
FREQUENCIES="$OUT_DIR/frequencies.csv"
printf '%s\n' 'repetition,path,elapsed_ns,evaluations,ns_per_guard,candidates_per_second,stack_bytes_per_guard,allocations,memory_peak_bytes,branch_misses,result' >"$RAW"
printf '%s\n' 'repetition,cpu0_frequency_khz_before,cpu0_frequency_khz_after' >"$FREQUENCIES"

run_bench() {
  if [[ -n "$CPU" ]]; then
    taskset -c "$CPU" "$BENCH_BIN" --warmup "$WARMUP" --iterations "$ITERATIONS"
  else
    "$BENCH_BIN" --warmup "$WARMUP" --iterations "$ITERATIONS"
  fi
}

for repetition in $(seq 1 "$REPETITIONS"); do
  freq_before=$(readlink -f /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq >/dev/null 2>&1 && tr -d '\0\n' </sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq || printf 'unavailable')
  run_out=$(run_bench 2>>"$TMP/benchmark.stderr")
  freq_after=$(readlink -f /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq >/dev/null 2>&1 && tr -d '\0\n' </sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq || printf 'unavailable')
  printf '%s,%s,%s\n' "$repetition" "$freq_before" "$freq_after" >>"$FREQUENCIES"
  header=$(printf '%s\n' "$run_out" | head -n 1)
  expected='path,elapsed_ns,evaluations,ns_per_guard,candidates_per_second,stack_bytes_per_guard,allocations,memory_peak_bytes,branch_misses,result'
  if [[ "$header" != "$expected" ]]; then
    echo "unexpected benchmark CSV header on repetition $repetition" >&2
    exit 3
  fi
  printf '%s\n' "$run_out" | tail -n +2 | awk -v r="$repetition" '{print r "," $0}' >>"$RAW"
done

python3 - "$RAW" "$REPETITIONS" <<'PY'
import csv
import sys
from collections import Counter

path, repetitions = sys.argv[1], int(sys.argv[2])
rows = list(csv.DictReader(open(path, encoding="utf-8")))
expected_paths = {
    "scalar_if_chain",
    "generic_bool_expr",
    "u64_compiled_guard",
    "multiword_guard",
    "batch_filter",
}
counts = Counter(row["path"] for row in rows)
if set(counts) != expected_paths or any(counts[p] != repetitions for p in expected_paths):
    raise SystemExit(f"unexpected path/repetition matrix: {counts}")
if any(row["result"] != "True" for row in rows):
    raise SystemExit("semantic result drifted from True")
for field in ("allocations", "memory_peak_bytes", "branch_misses"):
    if any(row[field] != "unmeasured" for row in rows):
        raise SystemExit(f"{field} must remain explicit unmeasured in this collector")
PY

read_one() {
  local path=$1
  if [[ -r "$path" ]]; then
    tr -d '\0\n' <"$path"
  else
    printf 'unavailable'
  fi
}

GOVERNOR=$(read_one /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)
FREQ_AFTER=$(read_one /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq)
DEVICE_MODEL=$(read_one /proc/device-tree/model)

{
  echo 'schema=elasticxxx-be13-portable-evidence/v1'
  echo "source_sha=$SOURCE_SHA"
  echo "collected_at_utc=$COLLECTED_AT_UTC"
  echo "collector_sha256=$COLLECTOR_SHA256"
  echo 'toolchain=1.89.0'
  echo "rustc_version=$(rustc +1.89.0 -V)"
  rustc +1.89.0 -Vv | while IFS=: read -r key value; do
    case "$key" in
      commit-hash|commit-date|host|release|'LLVM version')
        normalized=${key// /_}
        echo "rustc_${normalized}=${value# }"
        ;;
    esac
  done
  echo "cargo_version=$(cargo +1.89.0 -V)"
  uname -srmo | sed 's/^/kernel=/'
  echo "device_model=$DEVICE_MODEL"
  lscpu | grep -E '^(Architecture|CPU\(s\)|On-line CPU\(s\) list|Vendor ID|BIOS Vendor ID|BIOS Model name|Thread\(s\) per core|Core\(s\) per cluster|Socket\(s\)|Cluster\(s\)|CPU max MHz|CPU min MHz|L1d cache|L1i cache|L2 cache|NUMA node\(s\)|NUMA node0 CPU\(s\)):' | while IFS=: read -r key value; do
    normalized=${key// /_}
    echo "lscpu_${normalized}=${value# }"
  done
  echo "cpu_affinity=${CPU:-none}"
  echo "cpu0_governor=$GOVERNOR"
  echo "cpu0_frequency_khz_before=$FREQ_BEFORE"
  echo "cpu0_frequency_khz_after=$FREQ_AFTER"
  echo "repetitions=$REPETITIONS"
  echo "warmup=$WARMUP"
  echo "iterations=$ITERATIONS"
  echo 'profile=bench/optimized'
  echo 'frequency_sampling=cpu0_scaling_cur_freq_before_and_after_each_repetition_not_continuous'
  echo 'repo_clean_before_collection=true'
  echo 'allocations=unmeasured'
  echo 'memory_peak_bytes=unmeasured'
  echo 'branch_misses=unmeasured'
  echo 'interpretation=development_measurement_only_no_speedup_hardware_energy_or_novelty_claim'
} >"$OUT_DIR/metadata.txt"

(cd "$OUT_DIR" && sha256sum raw.csv frequencies.csv metadata.txt > SHA256SUMS)
echo "BE13 evidence written to $OUT_DIR for $SOURCE_SHA"
