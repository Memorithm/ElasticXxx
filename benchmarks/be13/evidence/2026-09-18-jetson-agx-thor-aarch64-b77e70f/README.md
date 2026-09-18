# Interpretation boundary

This retained set was collected from source `b77e70f56077354dd0998101b4de072fd8727168` on the declared NVIDIA Jetson AGX Thor Developer Kit using Rust 1.89.0, 30 repetitions, 10,000 warmup evaluations and 500,000 timed iterations with CPU 0 affinity.

The sampled CPU0 scaling frequency varied from 972000 kHz to 2601000 kHz across the before/after repetition samples. This is retained as an uncontrolled DVFS confound. The set is therefore development timing evidence only and is **not** sufficient to authorize BE13e, a speedup claim, an architecture-specific optimization claim, or a hardware-performance conclusion. No post-hoc frequency normalization is applied.

Allocation count, peak memory and branch misses were not directly measured and remain explicit `unmeasured` fields. All 150 retained path rows report the expected `True` semantic result; this does not replace the separate semantic equivalence tests in the codebase.
