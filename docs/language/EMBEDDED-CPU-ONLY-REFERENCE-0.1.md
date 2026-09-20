# ELANG7 hosted CPU-only reference qualification v0.1

Status: ELANG7 reference-environment gate; no hardware-cost or performance claim.

The embedded/edge profile must not accidentally require a GPU, CUDA, ROCm,
WGPU device, or a production model-serving backend merely to use its generic
contracts. The repository therefore carries a separate hosted CPU-only gate on
`ubuntu-latest` in addition to the ARM64/Jetson Thor qualification used for
real Linux telemetry and other hardware-specific work.

## What “low-cost CPU-only reference” means here

The roadmap phrase is implemented conservatively as a **general-purpose hosted
CPU-only CI reference**, not as a claim about the purchase price, cloud price,
energy cost, or performance of a particular processor.

The gate records its observed CPU model, logical CPU count, memory size,
platform and cgroup membership. Those values are evidence about that run only;
no minimum performance result or price threshold is inferred from them.
Economic classification remains outside ElasticXxx.

## Exact source binding

The workflow passes the pull-request head SHA (or push SHA) to the qualification
script. The script requires:

- the checked-out `HEAD` to equal that expected SHA exactly;
- a clean source worktree before qualification;
- a clean tracked source state after qualification.

The emitted JSON record includes `source_sha`. This prevents a green run on an
accidental checkout from being treated as evidence for another commit.

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

## Real Elastic CPU observation

The gate executes the public `linux_cpu_environment` example on the hosted
runner and validates the actual `LinuxCpuEnvironmentObserver` output.

It requires:

- a positive integral affinity count;
- an exact quota-unlimited discriminator in `{0,1}`;
- a positive finite quota when the quota is finite;
- an explicitly unsupported numeric quota when `cpu.max` is unlimited;
- PSI values in `[0,1]` when exposed, otherwise an explicit unsupported state;
- one consistent observation source and the complete five-signal set.

Missing telemetry is not converted to zero or to a fabricated capability.

## Portable Boolean semantic smoke

The same hosted CPU runs all five existing BE13 portable paths:

- `scalar_if_chain`;
- `generic_bool_expr`;
- `u64_compiled_guard`;
- `multiword_guard`;
- `batch_filter`.

The gate requires each path to preserve the expected `True` semantic result and
to emit positive finite timing counters. Timings are retained in the JSON record
as observations only. No throughput or latency claim is made.

## Exact toolchain

The workflow installs and requires Rust `1.89.0`, matching the project MSRV.
The reference script rejects another architecture or Rust version rather than
silently changing the qualification environment.

The current hosted reference is Linux `x86_64`. Jetson/ARM64 remains a separate
hardware-observer qualification and is not substituted for this CPU-only gate.

## Qualified public surfaces

After the real observation and BE13 semantic smoke, the gate executes public
facade contracts for:

- Linux CPU affinity/quota/PSI observation;
- representation/precision <-> KV composition;
- transition hysteresis/cooldown/rate limiting;
- thermal/energy policy and transactional test-provider flow;
- the `elastic-downstream` facade-only suite, which covers grouped EIR,
  RAM/storage budgets, immutable safety reservations, model-profile policy
  binding, composite transactions and other public contracts.

None of these tests requires an accelerator device. Thermal/power domain tests
use their explicit test-provider contract; the real Linux thermal/power observer
remains independently qualified on suitable hardware and fails closed when a
host does not expose the signal.

## Evidence emitted by the run

`scripts/qualify-elang7-cpu-only-reference.sh` emits a JSON record with schema:

```text
elastic-elang7-cpu-only-reference/v1
```

It records the exact source SHA, environment facts, parsed real Elastic CPU
observations, and the five BE13 semantic-smoke rows. It explicitly carries:

```text
cost_claimed = false
performance_claimed = false
gpu_compute_required = false
```

The terminal marker is:

```text
ELANG7_CPU_ONLY_REFERENCE_QUALIFIED
```

A green run establishes portability of the selected contracts on that observed
hosted CPU-only environment. It does not establish throughput, latency, power,
model-quality, or hardware-cost superiority.

## Local reproduction

From a clean checkout of the exact source:

```bash
ELANG7_EXPECTED_SOURCE_SHA="$(git rev-parse HEAD)" \
  bash scripts/qualify-elang7-cpu-only-reference.sh
```

This is a functional/reference qualification gate. A future claim about a
specific inexpensive physical processor requires naming that processor,
recording the external cost criterion, and collecting evidence on that physical
host; this gate deliberately does not infer such a classification.
