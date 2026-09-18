# BE13 retained portable benchmark evidence

This directory retains raw, reproducible development measurements for the
portable Boolean screening harness. Every evidence set is bound to an existing
source commit and records its collector hash, Rust toolchain, kernel, hardware
identity, CPU affinity, repetition count, warmup and timed iterations.

Retained evidence is descriptive. It is not, by itself, a speedup, hardware,
energy, scientific-novelty or actuation claim. A comparison is eligible for a
performance claim only after the exact raw data, methodology and semantic
parity of the compared paths have been reviewed. Fields that were not directly
measured remain the literal string `unmeasured` and must never be interpreted as
zero.

The collector also retains `frequencies.csv`, sampling CPU0's reported scaling
frequency immediately before and after each repetition when the kernel exposes
that sysfs signal. This is not continuous frequency telemetry; variation is a
confound to retain, not a value to normalize away after the fact.
