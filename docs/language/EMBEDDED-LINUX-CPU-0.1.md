# Embedded Linux CPU observation contract v0.1

Status: ELANG7a read-only host telemetry. This layer observes Linux scheduling
constraints and pressure; it does not change CPU affinity, cgroup quota,
scheduler policy, frequency, or power state.

## Signals

`LinuxCpuEnvironmentObserver` emits five independent custom signals:

| Signal | Unit | Source |
| --- | --- | --- |
| `linux-cpu-affinity-allowed-cpus` | `logical-cpus` | `/proc/self/status` `Cpus_allowed_list` |
| `linux-cpu-quota-cores` | `cpu-cores` | current cgroup-v2 `cpu.max` when finite |
| `linux-cpu-quota-unlimited` | `boolean` (`1` unlimited, `0` finite) | current cgroup-v2 `cpu.max` |
| `linux-cpu-pressure-some-avg10` | `fraction` | current cgroup `cpu.pressure`, falling back to `/proc/pressure/cpu` |
| `linux-cpu-pressure-full-avg10` | `fraction` | same PSI source when the `full` line exists |

The kernel PSI ABI expresses `avg10` as percent; Elastic converts it to a
fraction in `[0,1]`. The raw cgroup quota is converted to equivalent CPU cores
as `quota_us / period_us`.

## Cgroup resolution

The default observer reads `/proc/self/cgroup`, resolves the unified cgroup-v2
entry (`0::/...`) below `/sys/fs/cgroup`, and reads `cpu.max` and
`cpu.pressure` from that exact current cgroup. It does not assume those files
live at the cgroup mount root.

## Unlimited quota is not a fabricated number

For `cpu.max = max PERIOD`, Elastic emits:

- `linux-cpu-quota-unlimited = 1` (valid);
- `linux-cpu-quota-cores` as **unsupported**.

It does not substitute the affinity count, host CPU count, infinity, or zero as
a numeric quota. A finite quota emits its core equivalent and
`linux-cpu-quota-unlimited = 0`.

## Per-signal fail-closed behavior

Affinity, quota and PSI are independent evidence sources. If PSI is unavailable
but affinity/quota are valid, only the PSI observations are unsupported. Missing
or malformed telemetry is represented by the ordinary Elastic unsupported
observation (`NaN` numeric payload and no `PlanningContext` entry).

The process CPU-list parser accepts canonical Linux comma/range syntax, rejects
descending or malformed ranges, deduplicates overlap, and applies a bounded CPU
index. Telemetry files are read through a bounded 64 KiB reader.

## Reference-host qualification

The standard example can be run without write access:

```bash
cargo run -p memorithm-elastic --example linux_cpu_environment
```

ELANG7 qualification must report what the host actually exposes. Unsupported
quota/PSI on a reference environment is not converted into a synthetic value and
does not fail other independent signals.

## Authority boundary

This observer is `OBSERVE` only. Any future CPU affinity/quota actuator requires
its own capability contract, trusted validation immediately before effect,
verification, rollback, and anti-thrashing controls. These readings alone never
authorize CPU control.
