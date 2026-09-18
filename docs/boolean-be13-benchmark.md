# BE13 portable Boolean screening benchmark protocol

Status: **benchmark harness candidate; no performance claim**.

The dependency-free `be13_portable` benchmark compares five declared execution
paths on the same three-valued conjunction and stable fact assignment:
`scalar_if_chain`, generic `BoolExpr`, `u64_compiled_guard`,
`multiword_guard`, and `batch_filter`.

Run it only from a clean exact revision and record that revision beside the raw
output:

```bash
git rev-parse HEAD
cargo bench -p elastic-core --bench be13_portable -- --warmup 10000 --iterations 200000
```

The CSV fields report elapsed nanoseconds, evaluated guards, derived
`ns_per_guard`, derived candidates/second, and the Rust stack size of the guard
value. `stack_bytes_per_guard` is **not** total retained memory and excludes heap
storage.

Allocation count, peak memory and branch misses are deliberately emitted as
`unmeasured`. They must not be interpreted as zero. A later measurement slice
may add a reviewed portable/instrumented allocator or platform counter path,
but only if its dependencies, licences and measurement semantics are explicit.

The benchmark performs no actuation and grants no validation authority. Timing
numbers are host- and build-specific development evidence only. No speedup claim
is valid unless raw output, toolchain/build mode, hardware identity and exact
commit SHA are retained and the compared paths are semantically equivalent.
