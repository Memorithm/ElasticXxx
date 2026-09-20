# ELANG7 hosted CPU-only reference qualification v0.1

Status: ELANG7 final reference-environment gate.

The embedded/edge profile must not accidentally require a GPU, CUDA, ROCm,
WGPU device, or a production model-serving backend merely to use its generic
contracts. The repository therefore carries a separate hosted CPU-only gate on
`ubuntu-latest` in addition to the ARM64/Jetson Thor qualification used for
real Linux telemetry and other hardware-specific work.

## What “low-cost CPU-only reference” means here

The roadmap phrase is implemented as a **general-purpose hosted CPU-only CI
reference**, not as a claim about the purchase price, cloud price, energy cost,
or performance of a particular processor.

The gate records its observed CPU model, logical CPU count, memory size,
platform and cgroup membership. Those values are evidence about that run only;
no minimum performance result or price threshold is inferred from them.

## Accelerator exclusion

Before running the profile, the qualification script fails if it detects:

- any `/dev/nvidia*` device node;
- AMD KFD compute device `/dev/kfd`;
- a visible GPU returned by `nvidia-smi -L`.

It also clears/disables common accelerator selectors:

```text
CUDA_VISIBLE_DEVICES=
NVIDIA_VISIBLE_DEVICES=void
HIP_VISIBLE_DEVICES=
ROCR_VISIBLE_DEVICES=
ONEAPI_DEVICE_SELECTOR=cpu
SYCL_DEVICE_FILTER=cpu
```

These variables are defense-in-depth. The qualified Elastic path itself does
not require those accelerator runtimes.

## Exact toolchain

The workflow installs and requires Rust `1.89.0`, matching the project MSRV.
The reference script rejects another architecture or Rust version rather than
silently changing the qualification environment.

The current hosted reference is Linux `x86_64`. Jetson/ARM64 remains a separate
hardware-observer qualification and is not substituted for this CPU-only gate.

## Qualified public surfaces

The gate executes only public/facade entry points:

- Linux CPU affinity/quota/PSI observation;
- representation/precision <-> KV composition;
- transition hysteresis/cooldown/rate limiting;
- thermal/energy policy and transactional test-provider flow;
- the `elastic-downstream` facade-only suite, which covers grouped EIR,
  RAM/storage budgets, immutable safety reservations, model-profile policy
  binding, composite transactions and other public contracts.

None of these tests requires an accelerator device. Thermal/power domain tests
use their explicit test-provider contract; the real Linux thermal/power
observer remains independently qualified on suitable hardware and fails closed
when a host does not expose the signal.

## Evidence emitted by the run

`scripts/qualify-elang7-cpu-only-reference.sh` prints a JSON record with schema:

```text
elastic-elang7-cpu-only-reference/v1
```

It records environment facts and explicitly carries:

```text
cost_claimed = false
performance_claimed = false
gpu_compute_required = false
```

The terminal marker is:

```text
ELANG7_CPU_ONLY_REFERENCE_QUALIFIED
```

A green run establishes portability of the selected contracts on that hosted
CPU-only environment. It does not establish throughput, latency, power,
model-quality, or hardware-cost superiority.
