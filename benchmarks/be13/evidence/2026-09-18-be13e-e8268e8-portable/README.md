# BE13 retained portable measurement evidence

Source: `e8268e81f882503a07dd7163fa97578d55f60514` on `NVIDIA Jetson AGX Thor Developer Kit` with Rust 1.89.0.

- timing repetitions: 30; warmup: 10000; iterations: 500000;
- timing paths run one-per-process in a deterministic rotating order;
- CPU affinity: 0; CPUFreq mode: lock-max; lock target: 2601000 kHz;
- continuous CPUFreq samples: 3038; stability check: true;
- CPUFreq policy restored after collection: true;
- branch misses: measured_whole_process_user_space, measured only through the direct Linux generalized hardware counter when available;
- memory peak: measured_whole_process_ru_maxrss, whole-process peak RSS from `wait4(2)` rather than per-guard heap retention;
- allocations: unmeasured;
- preregistered timing-stability gate: **true** (worst block-median spread ratio: 0.073688569; limit: 0.10);
- comparison-qualified frequency + timing control: **true**.

The CPUFreq sample is a kernel policy report and is not claimed to be an exact instantaneous hardware-frequency measurement. Whole-process PMU/RSS metrics have a wider scope than the Rust timed evaluation loop and are retained separately in `process_metrics.csv`. This evidence makes no speedup, hardware, energy, scientific-novelty or actuation claim.
