# BE13 retained portable benchmark evidence

This directory retains raw development measurements for the portable Boolean
screening harness. Every evidence set records an exact source commit, Rust
version, kernel/hardware identity, CPU affinity, collection bounds, checksums and
an explicit interpretation boundary.

Two schemas are supported:

- `elasticxxx-be13-portable-evidence/v1`: historical repeated timing with
  before/after CPU-frequency samples. Existing v1 evidence remains valid and is
  not silently reinterpreted.
- `elasticxxx-be13-portable-evidence/v2`: one-path-per-process timing with
  deterministic path rotation, optional transactional CPUFreq lock control,
  continuous CPUFreq samples, and separately scoped direct process metrics.

For v2, `comparison_qualified=true` means only that the declared CPUFreq control
was accepted, sampled stable at the target throughout collection, and restored
afterwards. Linux `scaling_cur_freq` is a CPUFreq policy report and is not
claimed to be an exact instantaneous hardware-frequency measurement.

`process_metrics.csv` may contain direct user-space branch-miss counts from
Linux `perf_event_open(PERF_COUNT_HW_BRANCH_MISSES)` and whole-process peak RSS
from `wait4(2)/ru_maxrss`. Those measurements include loader/setup/warmup/output
and therefore have a wider scope than the `raw.csv` Rust timed loop. Allocation
count remains `unmeasured` until a separately reviewed region-scoped method is
qualified.

Retained evidence is descriptive. It is not, by itself, a speedup, hardware,
energy, scientific-novelty or actuation claim. Unmeasured fields remain the
literal string `unmeasured` and must never be interpreted as zero.
