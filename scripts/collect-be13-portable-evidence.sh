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
FREQ_MODE=${BE13_FREQUENCY_MODE:-observe}
FREQ_KHZ=${BE13_FREQUENCY_KHZ:-}
FREQ_SAMPLE_INTERVAL_MS=${BE13_FREQ_SAMPLE_INTERVAL_MS:-5}
SETTLE_SECONDS=${BE13_SETTLE_SECONDS:-2}
PROCESS_METRICS_MODE=${BE13_PROCESS_METRICS:-auto}
METRIC_REPETITIONS=${BE13_METRIC_REPETITIONS:-10}
REQUIRE_QUALIFIED=${BE13_REQUIRE_QUALIFIED:-0}
SOURCE_REF=${BE13_SOURCE_REF:-}
EXPECTED_CODEGEN_PROFILE=${BE13_CODEGEN_PROFILE_EXPECTED:-}

for pair in \
  "BE13_REPETITIONS:$REPETITIONS" \
  "BE13_WARMUP:$WARMUP" \
  "BE13_ITERATIONS:$ITERATIONS" \
  "BE13_FREQ_SAMPLE_INTERVAL_MS:$FREQ_SAMPLE_INTERVAL_MS" \
  "BE13_METRIC_REPETITIONS:$METRIC_REPETITIONS"; do
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
if [[ ! "$SETTLE_SECONDS" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  echo "BE13_SETTLE_SECONDS must be a non-negative number" >&2
  exit 2
fi
case "$FREQ_MODE" in
  observe|lock-max) ;;
  *) echo "BE13_FREQUENCY_MODE must be observe or lock-max" >&2; exit 2 ;;
esac
case "$PROCESS_METRICS_MODE" in
  auto|required|off) ;;
  *) echo "BE13_PROCESS_METRICS must be auto, required, or off" >&2; exit 2 ;;
esac
case "$REQUIRE_QUALIFIED" in
  0|1) ;;
  *) echo "BE13_REQUIRE_QUALIFIED must be 0 or 1" >&2; exit 2 ;;
esac
case "$EXPECTED_CODEGEN_PROFILE" in
  ""|portable|native) ;;
  *) echo "BE13_CODEGEN_PROFILE_EXPECTED must be portable, native, or empty" >&2; exit 2 ;;
esac
if [[ -n "$SOURCE_REF" && "$SOURCE_REF" != refs/tags/* ]]; then
  echo "BE13_SOURCE_REF must be an explicit refs/tags/... permanent ref" >&2
  exit 2
fi
if (( REPETITIONS < 30 || REPETITIONS % 5 != 0 )); then
  echo "BE13_REPETITIONS must be at least 30 and divisible by 5 for the preregistered timing-stability gate" >&2
  exit 2
fi
if [[ -n "$CPU" ]] && ! command -v taskset >/dev/null 2>&1; then
  echo "BE13_CPU requires taskset for explicit CPU affinity" >&2
  exit 2
fi
if [[ "$FREQ_MODE" == lock-max ]] && ! command -v flock >/dev/null 2>&1; then
  echo "lock-max requires flock for exclusive CPUFreq policy control" >&2
  exit 2
fi

mkdir -p "$OUT_DIR"
if [[ -n "$(find "$OUT_DIR" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
  echo "output directory must be empty: $OUT_DIR" >&2
  exit 2
fi

TMP=$(mktemp -d)
SAMPLER_PID=
FREQ_LOCK_ACTIVE=0
FREQ_RESTORED=false
ORIGINAL_GOVERNOR=unavailable
ORIGINAL_MIN_KHZ=unavailable
ORIGINAL_MAX_KHZ=unavailable
LOCK_TARGET_KHZ=unavailable
CPUFREQ_DIR=unavailable
RELATED_CPUS=unavailable
SAMPLER_CPU=none

read_one() {
  local path=$1
  if [[ -r "$path" ]]; then
    tr -d '\0\n' <"$path"
  else
    printf 'unavailable'
  fi
}

restore_frequency() {
  if [[ "$FREQ_LOCK_ACTIVE" != 1 ]]; then
    return 0
  fi
  local ok=1
  if ! printf '%s\n' "$ORIGINAL_MAX_KHZ" >"$CPUFREQ_DIR/scaling_max_freq"; then ok=0; fi
  if ! printf '%s\n' "$ORIGINAL_MIN_KHZ" >"$CPUFREQ_DIR/scaling_min_freq"; then ok=0; fi
  if ! printf '%s\n' "$ORIGINAL_GOVERNOR" >"$CPUFREQ_DIR/scaling_governor"; then ok=0; fi
  if [[ "$ok" == 1 ]] \
    && [[ "$(read_one "$CPUFREQ_DIR/scaling_max_freq")" == "$ORIGINAL_MAX_KHZ" ]] \
    && [[ "$(read_one "$CPUFREQ_DIR/scaling_min_freq")" == "$ORIGINAL_MIN_KHZ" ]] \
    && [[ "$(read_one "$CPUFREQ_DIR/scaling_governor")" == "$ORIGINAL_GOVERNOR" ]]; then
    FREQ_RESTORED=true
    FREQ_LOCK_ACTIVE=0
    return 0
  fi
  echo "failed to restore CPUFreq policy at $CPUFREQ_DIR" >&2
  return 1
}

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "$SAMPLER_PID" ]] && kill -0 "$SAMPLER_PID" 2>/dev/null; then
    kill -TERM "$SAMPLER_PID" 2>/dev/null || true
    wait "$SAMPLER_PID" 2>/dev/null || true
  fi
  if [[ "$FREQ_LOCK_ACTIVE" == 1 ]]; then
    restore_frequency || status=20
  fi
  rm -rf "$TMP"
  exit "$status"
}
trap cleanup EXIT INT TERM

SOURCE_SHA=$(git rev-parse HEAD)
if [[ -z "$SOURCE_REF" ]]; then
  echo "all BE13 v2 attested evidence requires BE13_SOURCE_REF=refs/tags/..." >&2
  exit 2
fi
SOURCE_REF_SHA=$(git rev-parse "${SOURCE_REF}^{commit}" 2>/dev/null || true)
if [[ "$SOURCE_REF_SHA" != "$SOURCE_SHA" ]]; then
  echo "BE13_SOURCE_REF $SOURCE_REF resolves to ${SOURCE_REF_SHA:-missing}, expected $SOURCE_SHA" >&2
  exit 2
fi
COLLECTED_AT_UTC=$(date -u +'%Y-%m-%dT%H:%M:%SZ')
COLLECTOR_SHA256=$(sha256sum "${BASH_SOURCE[0]}" | awk '{print $1}')

HOST_TRIPLE=$(rustc +1.89.0 -Vv | awk -F': ' '$1 == "host" {print $2}')
TARGET_RUSTFLAGS_VAR="CARGO_TARGET_$(printf '%s' "$HOST_TRIPLE" | tr '[:lower:].-' '[:upper:]__')_RUSTFLAGS"
RUSTFLAGS_VALUE=${RUSTFLAGS:-}
CARGO_ENCODED_RUSTFLAGS_PRESENT=false
if [[ -v CARGO_ENCODED_RUSTFLAGS ]]; then
  CARGO_ENCODED_RUSTFLAGS_PRESENT=true
fi
CARGO_ENCODED_RUSTFLAGS_VALUE=${CARGO_ENCODED_RUSTFLAGS:-}
TARGET_RUSTFLAGS_VALUE=${!TARGET_RUSTFLAGS_VAR:-}
CARGO_BUILD_RUSTFLAGS_VALUE=${CARGO_BUILD_RUSTFLAGS:-}
CARGO_BUILD_TARGET_VALUE=${CARGO_BUILD_TARGET:-}
CARGO_INCREMENTAL_VALUE=${CARGO_INCREMENTAL:-}
RUSTC_VALUE=${RUSTC:-}
RUSTC_WRAPPER_VALUE=${RUSTC_WRAPPER:-}
RUSTC_WORKSPACE_WRAPPER_VALUE=${RUSTC_WORKSPACE_WRAPPER:-}
TARGET_LINKER_VAR="CARGO_TARGET_$(printf '%s' "$HOST_TRIPLE" | tr '[:lower:].-' '[:upper:]__')_LINKER"
TARGET_LINKER_VALUE=${!TARGET_LINKER_VAR:-}

base64_text() {
  python3 - "$1" <<'PY64'
import base64
import sys
print(base64.b64encode(sys.argv[1].encode('utf-8')).decode('ascii'))
PY64
}
RUSTFLAGS_BASE64=$(base64_text "$RUSTFLAGS_VALUE")
CARGO_ENCODED_RUSTFLAGS_BASE64=$(base64_text "$CARGO_ENCODED_RUSTFLAGS_VALUE")
TARGET_RUSTFLAGS_BASE64=$(base64_text "$TARGET_RUSTFLAGS_VALUE")
CARGO_BUILD_RUSTFLAGS_BASE64=$(base64_text "$CARGO_BUILD_RUSTFLAGS_VALUE")
CARGO_BUILD_TARGET_BASE64=$(base64_text "$CARGO_BUILD_TARGET_VALUE")
CARGO_INCREMENTAL_BASE64=$(base64_text "$CARGO_INCREMENTAL_VALUE")
RUSTC_BASE64=$(base64_text "$RUSTC_VALUE")
RUSTC_WRAPPER_BASE64=$(base64_text "$RUSTC_WRAPPER_VALUE")
RUSTC_WORKSPACE_WRAPPER_BASE64=$(base64_text "$RUSTC_WORKSPACE_WRAPPER_VALUE")
TARGET_LINKER_BASE64=$(base64_text "$TARGET_LINKER_VALUE")
METRICS_HELPER_SOURCE="$ROOT/tools/be13/process_metrics.c"
METRICS_HELPER_SHA256=$(sha256sum "$METRICS_HELPER_SOURCE" | awk '{print $1}')
TIMING_ANALYZER_SOURCE="$ROOT/scripts/be13_timing_stability.py"
TIMING_ANALYZER_SHA256=$(sha256sum "$TIMING_ANALYZER_SOURCE" | awk '{print $1}')

COMPILER_CFG="$OUT_DIR/compiler_cfg.txt"
CARGO_CONFIG_INVENTORY="$OUT_DIR/cargo_config_inventory.txt"
BUILD_ENV_INVENTORY="$OUT_DIR/build_env_inventory.txt"

python3 - "$BUILD_ENV_INVENTORY" <<'PYENV'
import base64
import os
import sys

path = sys.argv[1]
keys = sorted(key for key in os.environ if key.startswith("CARGO_PROFILE_"))
with open(path, "w", encoding="utf-8") as out:
    if not keys:
        out.write("none\n")
    else:
        for key in keys:
            value = base64.b64encode(os.environ[key].encode("utf-8")).decode("ascii")
            out.write(f"{key}={value}\n")
PYENV

: >"$TMP/cargo-config-inventory.unsorted"
config_dir=$(readlink -f "$ROOT")
while :; do
  for basename in config.toml config; do
    config_path="$config_dir/.cargo/$basename"
    if [[ -f "$config_path" ]]; then
      printf '%s\t%s\n' "$(sha256sum "$config_path" | awk '{print $1}')" "$config_path" \
        >>"$TMP/cargo-config-inventory.unsorted"
    fi
  done
  parent=$(dirname "$config_dir")
  [[ "$parent" == "$config_dir" ]] && break
  config_dir=$parent
done
for config_path in \
  "${CARGO_HOME:-$HOME/.cargo}/config.toml" \
  "${CARGO_HOME:-$HOME/.cargo}/config"; do
  if [[ -f "$config_path" ]]; then
    printf '%s\t%s\n' "$(sha256sum "$config_path" | awk '{print $1}')" "$config_path" \
      >>"$TMP/cargo-config-inventory.unsorted"
  fi
done
if [[ -s "$TMP/cargo-config-inventory.unsorted" ]]; then
  LC_ALL=C sort -u "$TMP/cargo-config-inventory.unsorted" >"$CARGO_CONFIG_INVENTORY"
else
  echo 'none' >"$CARGO_CONFIG_INVENTORY"
fi

CARGO_CONFIGS_PRESENT=false
[[ "$(cat "$CARGO_CONFIG_INVENTORY")" != none ]] && CARGO_CONFIGS_PRESENT=true
CARGO_PROFILE_OVERRIDES_PRESENT=false
[[ "$(cat "$BUILD_ENV_INVENTORY")" != none ]] && CARGO_PROFILE_OVERRIDES_PRESENT=true

CLEAN_BUILD_CONTEXT=true
[[ "$CARGO_ENCODED_RUSTFLAGS_PRESENT" == true ]] && CLEAN_BUILD_CONTEXT=false
for value in \
  "$CARGO_ENCODED_RUSTFLAGS_VALUE" \
  "$TARGET_RUSTFLAGS_VALUE" \
  "$CARGO_BUILD_RUSTFLAGS_VALUE" \
  "$CARGO_BUILD_TARGET_VALUE" \
  "$CARGO_INCREMENTAL_VALUE" \
  "$RUSTC_VALUE" \
  "$RUSTC_WRAPPER_VALUE" \
  "$RUSTC_WORKSPACE_WRAPPER_VALUE" \
  "$TARGET_LINKER_VALUE"; do
  [[ -n "$value" ]] && CLEAN_BUILD_CONTEXT=false
done
[[ "$CARGO_CONFIGS_PRESENT" == true ]] && CLEAN_BUILD_CONTEXT=false
[[ "$CARGO_PROFILE_OVERRIDES_PRESENT" == true ]] && CLEAN_BUILD_CONTEXT=false

if [[ "$CLEAN_BUILD_CONTEXT" == true && -z "$RUSTFLAGS_VALUE" ]]; then
  CODEGEN_PROFILE=portable
elif [[ "$CLEAN_BUILD_CONTEXT" == true && "$RUSTFLAGS_VALUE" == '-C target-cpu=native' ]]; then
  CODEGEN_PROFILE=native
else
  CODEGEN_PROFILE=custom
fi
if [[ "$REQUIRE_QUALIFIED" == 1 && -z "$EXPECTED_CODEGEN_PROFILE" ]]; then
  echo "qualified BE13 evidence requires BE13_CODEGEN_PROFILE_EXPECTED=portable|native" >&2
  exit 2
fi
if [[ -n "$EXPECTED_CODEGEN_PROFILE" && "$CODEGEN_PROFILE" != "$EXPECTED_CODEGEN_PROFILE" ]]; then
  echo "effective codegen profile $CODEGEN_PROFILE does not match expected $EXPECTED_CODEGEN_PROFILE" >&2
  exit 2
fi

if ! CARGO_TARGET_DIR="$TMP/cfg-target" cargo +1.89.0 rustc -p elastic-core --bench be13_portable -- \
  --print cfg >"$COMPILER_CFG" 2>"$TMP/compiler-cfg.stderr"; then
  cat "$TMP/compiler-cfg.stderr" >&2
  exit 4
fi
COMPILER_CFG_SHA256=$(sha256sum "$COMPILER_CFG" | awk '{print $1}')
CARGO_CONFIG_INVENTORY_SHA256=$(sha256sum "$CARGO_CONFIG_INVENTORY" | awk '{print $1}')
BUILD_ENV_INVENTORY_SHA256=$(sha256sum "$BUILD_ENV_INVENTORY" | awk '{print $1}')

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

METRICS_HELPER=
METRICS_HELPER_STATUS=unavailable
if [[ "$PROCESS_METRICS_MODE" != off ]] && command -v cc >/dev/null 2>&1; then
  if cc -std=c11 -O2 -Wall -Wextra -Werror "$METRICS_HELPER_SOURCE" -o "$TMP/be13-process-metrics" \
    2>"$TMP/metrics-helper.stderr"; then
    METRICS_HELPER="$TMP/be13-process-metrics"
    METRICS_HELPER_STATUS=available
  elif [[ "$PROCESS_METRICS_MODE" == required ]]; then
    cat "$TMP/metrics-helper.stderr" >&2
    exit 5
  fi
elif [[ "$PROCESS_METRICS_MODE" == required ]]; then
  echo "BE13 process metrics require a C compiler" >&2
  exit 5
fi

if [[ -n "$CPU" ]]; then
  CPUFREQ_DIR=$(readlink -f "/sys/devices/system/cpu/cpu${CPU}/cpufreq" 2>/dev/null || printf 'unavailable')
fi
if [[ "$CPUFREQ_DIR" != unavailable && -d "$CPUFREQ_DIR" ]]; then
  ORIGINAL_GOVERNOR=$(read_one "$CPUFREQ_DIR/scaling_governor")
  ORIGINAL_MIN_KHZ=$(read_one "$CPUFREQ_DIR/scaling_min_freq")
  ORIGINAL_MAX_KHZ=$(read_one "$CPUFREQ_DIR/scaling_max_freq")
  RELATED_CPUS=$(read_one "$CPUFREQ_DIR/related_cpus")
fi

if [[ "$FREQ_MODE" == lock-max ]]; then
  if [[ -z "$CPU" || "$CPUFREQ_DIR" == unavailable || ! -d "$CPUFREQ_DIR" ]]; then
    echo "lock-max requires BE13_CPU and a readable CPUFreq policy" >&2
    exit 6
  fi
  for path in scaling_governor scaling_min_freq scaling_max_freq scaling_cur_freq; do
    if [[ ! -r "$CPUFREQ_DIR/$path" ]]; then
      echo "lock-max requires readable $CPUFREQ_DIR/$path" >&2
      exit 6
    fi
  done
  for path in scaling_governor scaling_min_freq scaling_max_freq; do
    if [[ ! -w "$CPUFREQ_DIR/$path" ]]; then
      echo "lock-max requires writable $CPUFREQ_DIR/$path" >&2
      exit 6
    fi
  done
  if ! grep -qw performance "$CPUFREQ_DIR/scaling_available_governors"; then
    echo "lock-max requires the performance CPUFreq governor" >&2
    exit 6
  fi
  if ! [[ "$ORIGINAL_MIN_KHZ" =~ ^[0-9]+$ && "$ORIGINAL_MAX_KHZ" =~ ^[0-9]+$ ]]; then
    echo "lock-max requires numeric CPUFreq policy bounds" >&2
    exit 6
  fi
  LOCK_TARGET_KHZ=${FREQ_KHZ:-$ORIGINAL_MAX_KHZ}
  if ! [[ "$LOCK_TARGET_KHZ" =~ ^[1-9][0-9]*$ ]]; then
    echo "BE13_FREQUENCY_KHZ must be a positive integer" >&2
    exit 6
  fi
  if (( LOCK_TARGET_KHZ < ORIGINAL_MIN_KHZ || LOCK_TARGET_KHZ > ORIGINAL_MAX_KHZ )); then
    echo "requested frequency $LOCK_TARGET_KHZ is outside current policy [$ORIGINAL_MIN_KHZ,$ORIGINAL_MAX_KHZ]" >&2
    exit 6
  fi
  exec 9>"/tmp/elasticxxx-be13-cpufreq-$(basename "$CPUFREQ_DIR").lock"
  if ! flock -n 9; then
    echo "another BE13 collector holds the CPUFreq policy lock" >&2
    exit 6
  fi
  printf '%s\n' "$LOCK_TARGET_KHZ" >"$CPUFREQ_DIR/scaling_max_freq"
  printf '%s\n' "$LOCK_TARGET_KHZ" >"$CPUFREQ_DIR/scaling_min_freq"
  printf '%s\n' performance >"$CPUFREQ_DIR/scaling_governor"
  FREQ_LOCK_ACTIVE=1
  if [[ "$(read_one "$CPUFREQ_DIR/scaling_max_freq")" != "$LOCK_TARGET_KHZ" \
    || "$(read_one "$CPUFREQ_DIR/scaling_min_freq")" != "$LOCK_TARGET_KHZ" \
    || "$(read_one "$CPUFREQ_DIR/scaling_governor")" != performance ]]; then
    echo "CPUFreq policy did not accept the requested lock" >&2
    exit 6
  fi
fi

if [[ "$CPUFREQ_DIR" != unavailable && -r "$CPUFREQ_DIR/scaling_cur_freq" ]]; then
  SAMPLER_CPU=$(python3 - "$RELATED_CPUS" <<'PY'
import os
import sys
related = {int(x) for x in sys.argv[1].split() if x.isdigit()}
allowed = sorted(os.sched_getaffinity(0))
outside = [cpu for cpu in allowed if cpu not in related]
print(outside[0] if outside else "none")
PY
)
fi

if [[ "$SETTLE_SECONDS" != 0 && "$SETTLE_SECONDS" != 0.0 ]]; then
  sleep "$SETTLE_SECONDS"
fi

RAW="$OUT_DIR/raw.csv"
FREQUENCIES="$OUT_DIR/frequencies.csv"
FREQ_SAMPLES="$OUT_DIR/frequency_samples.csv"
PROCESS_METRICS="$OUT_DIR/process_metrics.csv"
TIMING_STABILITY="$OUT_DIR/timing_stability.json"
printf '%s\n' 'repetition,path,elapsed_ns,evaluations,ns_per_guard,candidates_per_second,stack_bytes_per_guard,allocations,memory_peak_bytes,branch_misses,result' >"$RAW"
printf '%s\n' 'repetition,path,cpu_frequency_khz_before,cpu_frequency_khz_after' >"$FREQUENCIES"
printf '%s\n' 'sample,monotonic_ns,frequency_khz' >"$FREQ_SAMPLES"
printf '%s\n' 'repetition,path,evaluations,branch_misses,branch_misses_per_guard,max_rss_bytes,result,counter_status,counter_errno' >"$PROCESS_METRICS"

if [[ "$CPUFREQ_DIR" != unavailable && -r "$CPUFREQ_DIR/scaling_cur_freq" ]]; then
  cat >"$TMP/frequency-sampler.py" <<'PY'
import pathlib
import signal
import sys
import time

path = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
interval = int(sys.argv[3]) / 1000.0
running = True

def stop(_signum, _frame):
    global running
    running = False

signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
with out.open("a", encoding="utf-8", buffering=1) as f:
    sample = 0
    while running:
        sample += 1
        try:
            value = path.read_text(encoding="utf-8").strip()
            if not value.isdigit() or int(value) <= 0:
                value = "unavailable"
        except OSError:
            value = "unavailable"
        f.write(f"{sample},{time.monotonic_ns()},{value}\n")
        time.sleep(interval)
PY
  if [[ "$SAMPLER_CPU" != none ]]; then
    taskset -c "$SAMPLER_CPU" python3 "$TMP/frequency-sampler.py" \
      "$CPUFREQ_DIR/scaling_cur_freq" "$FREQ_SAMPLES" "$FREQ_SAMPLE_INTERVAL_MS" &
  else
    python3 "$TMP/frequency-sampler.py" \
      "$CPUFREQ_DIR/scaling_cur_freq" "$FREQ_SAMPLES" "$FREQ_SAMPLE_INTERVAL_MS" &
  fi
  SAMPLER_PID=$!
fi

PATHS=(scalar_if_chain generic_bool_expr u64_compiled_guard multiword_guard batch_filter)
EXPECTED_HEADER='path,elapsed_ns,evaluations,ns_per_guard,candidates_per_second,stack_bytes_per_guard,allocations,memory_peak_bytes,branch_misses,result'

run_bench_path() {
  local path=$1
  if [[ -n "$CPU" ]]; then
    taskset -c "$CPU" "$BENCH_BIN" --warmup "$WARMUP" --iterations "$ITERATIONS" --path "$path"
  else
    "$BENCH_BIN" --warmup "$WARMUP" --iterations "$ITERATIONS" --path "$path"
  fi
}

append_timing_row() {
  local repetition=$1
  local path=$2
  local before run_out after header row_count
  before=$(if [[ "$CPUFREQ_DIR" != unavailable ]]; then read_one "$CPUFREQ_DIR/scaling_cur_freq"; else printf unavailable; fi)
  run_out=$(run_bench_path "$path" 2>>"$TMP/benchmark.stderr")
  after=$(if [[ "$CPUFREQ_DIR" != unavailable ]]; then read_one "$CPUFREQ_DIR/scaling_cur_freq"; else printf unavailable; fi)
  header=$(printf '%s\n' "$run_out" | head -n 1)
  row_count=$(printf '%s\n' "$run_out" | tail -n +2 | sed '/^$/d' | wc -l)
  if [[ "$header" != "$EXPECTED_HEADER" || "$row_count" != 1 ]]; then
    echo "unexpected benchmark CSV output for repetition $repetition path $path" >&2
    exit 3
  fi
  printf '%s\n' "$run_out" | tail -n +2 | awk -v r="$repetition" '{print r "," $0}' >>"$RAW"
  printf '%s,%s,%s,%s\n' "$repetition" "$path" "$before" "$after" >>"$FREQUENCIES"
}

for repetition in $(seq 1 "$REPETITIONS"); do
  offset=$(( (repetition - 1) % ${#PATHS[@]} ))
  for step in $(seq 0 $((${#PATHS[@]} - 1))); do
    index=$(( (offset + step) % ${#PATHS[@]} ))
    append_timing_row "$repetition" "${PATHS[$index]}"
  done
done

python3 "$TIMING_ANALYZER_SOURCE" "$RAW" --repetitions "$REPETITIONS" --output "$TIMING_STABILITY"
read -r TIMING_STABLE TIMING_WORST_BLOCK_SPREAD < <(python3 - "$TIMING_STABILITY" <<'PY2'
import json
import sys
data = json.load(open(sys.argv[1], encoding="utf-8"))
print("true" if data["stable"] else "false", f'{data["worst_block_median_spread_ratio"]:.9f}')
PY2
)

if [[ "$METRICS_HELPER_STATUS" == available ]]; then
  for repetition in $(seq 1 "$METRIC_REPETITIONS"); do
    offset=$(( (repetition - 1) % ${#PATHS[@]} ))
    for step in $(seq 0 $((${#PATHS[@]} - 1))); do
      index=$(( (offset + step) % ${#PATHS[@]} ))
      path=${PATHS[$index]}
      metric_file="$TMP/metric-${repetition}-${path}.txt"
      if [[ -n "$CPU" ]]; then
        run_out=$("$METRICS_HELPER" "$metric_file" -- taskset -c "$CPU" "$BENCH_BIN" \
          --warmup "$WARMUP" --iterations "$ITERATIONS" --path "$path" \
          2>>"$TMP/process-metrics.stderr")
      else
        run_out=$("$METRICS_HELPER" "$metric_file" -- "$BENCH_BIN" \
          --warmup "$WARMUP" --iterations "$ITERATIONS" --path "$path" \
          2>>"$TMP/process-metrics.stderr")
      fi
      header=$(printf '%s\n' "$run_out" | head -n 1)
      row=$(printf '%s\n' "$run_out" | tail -n +2 | sed '/^$/d')
      if [[ "$header" != "$EXPECTED_HEADER" || $(printf '%s\n' "$row" | wc -l) != 1 ]]; then
        echo "unexpected process-metric benchmark output for repetition $repetition path $path" >&2
        exit 3
      fi
      IFS=',' read -r row_path _elapsed evaluations _nspg _cps _stack _alloc _mem _branch result <<<"$row"
      if [[ "$row_path" != "$path" || "$result" != True ]]; then
        echo "process-metric semantic result mismatch for $path" >&2
        exit 3
      fi
      counter_status=$(awk -F= '$1=="branch_counter_status"{print $2}' "$metric_file")
      counter_errno=$(awk -F= '$1=="branch_counter_errno"{print $2}' "$metric_file")
      branch_misses=$(awk -F= '$1=="branch_misses"{print $2}' "$metric_file")
      max_rss_bytes=$(awk -F= '$1=="max_rss_bytes"{print $2}' "$metric_file")
      if [[ "$branch_misses" =~ ^[0-9]+$ ]]; then
        branch_per_guard=$(python3 - "$branch_misses" "$evaluations" <<'PY'
import sys
print(f"{int(sys.argv[1]) / int(sys.argv[2]):.9f}")
PY
)
      else
        branch_per_guard=unmeasured
      fi
      printf '%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
        "$repetition" "$path" "$evaluations" "$branch_misses" "$branch_per_guard" \
        "$max_rss_bytes" "$result" "$counter_status" "$counter_errno" >>"$PROCESS_METRICS"
    done
  done
fi

if [[ -n "$SAMPLER_PID" ]]; then
  kill -TERM "$SAMPLER_PID" 2>/dev/null || true
  wait "$SAMPLER_PID" || true
  SAMPLER_PID=
fi

FREQ_SAMPLE_COUNT=$(($(wc -l <"$FREQ_SAMPLES") - 1))
FREQ_STABLE=false
if [[ "$FREQ_MODE" == lock-max ]]; then
  FREQ_STABLE=$(python3 - "$FREQ_SAMPLES" "$FREQUENCIES" "$LOCK_TARGET_KHZ" <<'PY'
import csv
import sys
samples_path, frequencies_path, target_text = sys.argv[1:]
target = int(target_text)
samples = list(csv.DictReader(open(samples_path, encoding="utf-8")))
frequencies = list(csv.DictReader(open(frequencies_path, encoding="utf-8")))
if len(samples) < 10:
    print("false")
    raise SystemExit
sample_ok = all(row["frequency_khz"].isdigit() and int(row["frequency_khz"]) == target for row in samples)
edge_ok = all(
    row[field].isdigit() and int(row[field]) == target
    for row in frequencies
    for field in ("cpu_frequency_khz_before", "cpu_frequency_khz_after")
)
print("true" if sample_ok and edge_ok else "false")
PY
)
fi

python3 - "$RAW" "$REPETITIONS" <<'PY'
import csv
import math
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
        raise SystemExit(f"timing CSV field {field} must remain explicit unmeasured")
for row in rows:
    if int(row["elapsed_ns"]) <= 0 or int(row["evaluations"]) <= 0:
        raise SystemExit("timing row has non-positive count")
    if not math.isfinite(float(row["ns_per_guard"])) or float(row["ns_per_guard"]) < 0:
        raise SystemExit("invalid ns_per_guard")
PY

BRANCH_MISSES_STATE=unmeasured
MEMORY_PEAK_STATE=unmeasured
if [[ "$METRICS_HELPER_STATUS" == available ]]; then
  read -r BRANCH_MISSES_STATE MEMORY_PEAK_STATE < <(python3 - "$PROCESS_METRICS" "$METRIC_REPETITIONS" <<'PY'
import csv
import sys
from collections import Counter
path, repetitions = sys.argv[1], int(sys.argv[2])
rows = list(csv.DictReader(open(path, encoding="utf-8")))
expected = {"scalar_if_chain", "generic_bool_expr", "u64_compiled_guard", "multiword_guard", "batch_filter"}
counts = Counter(row["path"] for row in rows)
if set(counts) != expected or any(counts[p] != repetitions for p in expected):
    raise SystemExit(f"unexpected process metric matrix: {counts}")
if any(row["result"] != "True" for row in rows):
    raise SystemExit("process metric semantic result drifted")
if any(int(row["max_rss_bytes"]) <= 0 for row in rows):
    raise SystemExit("process max RSS must be positive")
all_branch = all(row["counter_status"] == "measured" and row["branch_misses"].isdigit() for row in rows)
print("measured_whole_process_user_space" if all_branch else "unmeasured", "measured_whole_process_ru_maxrss")
PY
)
fi

if [[ "$FREQ_LOCK_ACTIVE" == 1 ]]; then
  restore_frequency
fi

COMPARISON_QUALIFIED=false
if [[ "$FREQ_MODE" == lock-max && "$FREQ_STABLE" == true && "$FREQ_RESTORED" == true && "$TIMING_STABLE" == true ]]; then
  COMPARISON_QUALIFIED=true
fi

GOVERNOR_AFTER=$(if [[ "$CPUFREQ_DIR" != unavailable ]]; then read_one "$CPUFREQ_DIR/scaling_governor"; else printf unavailable; fi)
MIN_AFTER=$(if [[ "$CPUFREQ_DIR" != unavailable ]]; then read_one "$CPUFREQ_DIR/scaling_min_freq"; else printf unavailable; fi)
MAX_AFTER=$(if [[ "$CPUFREQ_DIR" != unavailable ]]; then read_one "$CPUFREQ_DIR/scaling_max_freq"; else printf unavailable; fi)
DEVICE_MODEL=$(read_one /proc/device-tree/model)

{
  echo 'schema=elasticxxx-be13-portable-evidence/v2'
  echo "source_sha=$SOURCE_SHA"
  echo "source_ref=${SOURCE_REF:-none}"
  echo 'codegen_attestation=cargo-build-context-v2'
  echo "codegen_profile=$CODEGEN_PROFILE"
  echo "rustflags_base64=$RUSTFLAGS_BASE64"
  echo "cargo_encoded_rustflags_base64=$CARGO_ENCODED_RUSTFLAGS_BASE64"
  echo "cargo_encoded_rustflags_present=$CARGO_ENCODED_RUSTFLAGS_PRESENT"
  echo "target_rustflags_variable=$TARGET_RUSTFLAGS_VAR"
  echo "target_rustflags_base64=$TARGET_RUSTFLAGS_BASE64"
  echo "cargo_build_rustflags_base64=$CARGO_BUILD_RUSTFLAGS_BASE64"
  echo "cargo_build_target_base64=$CARGO_BUILD_TARGET_BASE64"
  echo "cargo_incremental_base64=$CARGO_INCREMENTAL_BASE64"
  echo "rustc_override_base64=$RUSTC_BASE64"
  echo "rustc_wrapper_base64=$RUSTC_WRAPPER_BASE64"
  echo "rustc_workspace_wrapper_base64=$RUSTC_WORKSPACE_WRAPPER_BASE64"
  echo "target_linker_variable=$TARGET_LINKER_VAR"
  echo "target_linker_base64=$TARGET_LINKER_BASE64"
  echo "compiler_cfg_sha256=$COMPILER_CFG_SHA256"
  echo "cargo_config_inventory_sha256=$CARGO_CONFIG_INVENTORY_SHA256"
  echo "build_env_inventory_sha256=$BUILD_ENV_INVENTORY_SHA256"
  echo "cargo_configs_present=$CARGO_CONFIGS_PRESENT"
  echo "cargo_profile_overrides_present=$CARGO_PROFILE_OVERRIDES_PRESENT"
  echo "collected_at_utc=$COLLECTED_AT_UTC"
  echo "collector_sha256=$COLLECTOR_SHA256"
  echo "metrics_helper_sha256=$METRICS_HELPER_SHA256"
  echo "timing_analyzer_sha256=$TIMING_ANALYZER_SHA256"
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
  echo "cpufreq_policy=$CPUFREQ_DIR"
  echo "cpufreq_related_cpus=$RELATED_CPUS"
  echo "frequency_control=$FREQ_MODE"
  echo "frequency_original_governor=$ORIGINAL_GOVERNOR"
  echo "frequency_original_min_khz=$ORIGINAL_MIN_KHZ"
  echo "frequency_original_max_khz=$ORIGINAL_MAX_KHZ"
  echo "frequency_lock_target_khz=$LOCK_TARGET_KHZ"
  echo "frequency_sampler_cpu=$SAMPLER_CPU"
  echo "frequency_sample_interval_ms=$FREQ_SAMPLE_INTERVAL_MS"
  echo "frequency_sample_count=$FREQ_SAMPLE_COUNT"
  echo "frequency_stable=$FREQ_STABLE"
  echo "frequency_policy_restored=$FREQ_RESTORED"
  echo "frequency_after_governor=$GOVERNOR_AFTER"
  echo "frequency_after_min_khz=$MIN_AFTER"
  echo "frequency_after_max_khz=$MAX_AFTER"
  echo 'frequency_report_semantics=scaling_cur_freq_is_kernel_cpufreq_policy_report_not_guaranteed_instantaneous_hardware_frequency'
  echo "repetitions=$REPETITIONS"
  echo "metric_repetitions=$METRIC_REPETITIONS"
  echo "warmup=$WARMUP"
  echo "iterations=$ITERATIONS"
  echo "settle_seconds=$SETTLE_SECONDS"
  echo 'profile=bench'
  echo 'profile_contract=workspace_source_plus_captured_CARGO_PROFILE_environment'
  echo 'timing_execution=one_selected_path_per_process_with_deterministic_rotating_path_order'
  echo 'timing_region=Rust_Instant_around_evaluation_loop_after_path_specific_warmup'
  echo 'timing_stability_method=contiguous_nonoverlapping_blocks_of_5_median_spread_no_posthoc_row_deletion'
  echo 'timing_stability_minimum_repetitions=30'
  echo 'timing_stability_block_size=5'
  echo 'timing_stability_max_block_median_spread_ratio=0.10'
  echo "timing_stable=$TIMING_STABLE"
  echo "timing_worst_block_median_spread_ratio=$TIMING_WORST_BLOCK_SPREAD"
  echo "process_metrics_helper=$METRICS_HELPER_STATUS"
  echo 'process_metrics_method=linux_perf_event_open_PERF_COUNT_HW_BRANCH_MISSES_user_space_plus_wait4_ru_maxrss'
  echo 'allocations=unmeasured'
  echo "memory_peak_bytes=$MEMORY_PEAK_STATE"
  echo "branch_misses=$BRANCH_MISSES_STATE"
  echo 'branch_misses_scope=whole_selected_path_process_including_loader_setup_warmup_timed_region_and_output'
  echo 'memory_peak_scope=whole_selected_path_process_peak_RSS_not_per_guard_heap_bytes'
  echo "comparison_qualified=$COMPARISON_QUALIFIED"
  echo 'repo_clean_before_collection=true'
  echo 'interpretation=retained_development_measurement_no_speedup_hardware_energy_novelty_or_actuation_claim'
} >"$OUT_DIR/metadata.txt"

cat >"$OUT_DIR/README.md" <<EOF
# BE13 retained portable measurement evidence

Source: \`$SOURCE_SHA\` on \`$DEVICE_MODEL\` with Rust 1.89.0.

- permanent source ref: \`${SOURCE_REF:-none}\`;
- collector-derived codegen profile: \`$CODEGEN_PROFILE\`;
- effective Cargo/rustc cfg retained in \`compiler_cfg.txt\`;
- Cargo config inventory (workspace ancestors plus Cargo home) retained in \`cargo_config_inventory.txt\`;
- Cargo profile environment overrides retained in \`build_env_inventory.txt\`;

- timing repetitions: $REPETITIONS; warmup: $WARMUP; iterations: $ITERATIONS;
- timing paths run one-per-process in a deterministic rotating order;
- CPU affinity: ${CPU:-none}; CPUFreq mode: $FREQ_MODE; lock target: $LOCK_TARGET_KHZ kHz;
- continuous CPUFreq samples: $FREQ_SAMPLE_COUNT; stability check: $FREQ_STABLE;
- CPUFreq policy restored after collection: $FREQ_RESTORED;
- branch misses: $BRANCH_MISSES_STATE, measured only through the direct Linux generalized hardware counter when available;
- memory peak: $MEMORY_PEAK_STATE, whole-process peak RSS from \`wait4(2)\` rather than per-guard heap retention;
- allocations: unmeasured;
- preregistered timing-stability gate: **$TIMING_STABLE** (worst block-median spread ratio: $TIMING_WORST_BLOCK_SPREAD; limit: 0.10);
- comparison-qualified frequency + timing control: **$COMPARISON_QUALIFIED**.

The CPUFreq sample is a kernel policy report and is not claimed to be an exact instantaneous hardware-frequency measurement. Whole-process PMU/RSS metrics have a wider scope than the Rust timed evaluation loop and are retained separately in \`process_metrics.csv\`. This evidence makes no speedup, hardware, energy, scientific-novelty or actuation claim.
EOF

(cd "$OUT_DIR" && sha256sum raw.csv frequencies.csv frequency_samples.csv process_metrics.csv timing_stability.json compiler_cfg.txt cargo_config_inventory.txt build_env_inventory.txt metadata.txt README.md > SHA256SUMS)

echo "BE13 evidence written to $OUT_DIR for $SOURCE_SHA (comparison_qualified=$COMPARISON_QUALIFIED)"
if [[ "$REQUIRE_QUALIFIED" == 1 && "$COMPARISON_QUALIFIED" != true ]]; then
  echo "BE13 evidence did not satisfy the requested qualified frequency-and-timing gate" >&2
  exit 8
fi
