# Linux thermal and direct-power observers

Status: BE14h real-observer foundation. These providers are observation-only and
never authorize or perform physical actuation.

ElasticXxx exposes two built-in host signals relevant to thermal/energy control:

- `thermal-margin`: **degrees Celsius** between the current thermal-zone
  temperature and the lowest kernel-declared `critical` trip point;
- `energy-rate`: **watts** from one explicitly selected Linux hwmon
  `power*_input` channel.

The runtime types are `LinuxThermalMarginObserver` and
`LinuxHwmonPowerObserver`. Both implement the ordinary `Observer` contract and
therefore compose through `ObserverSet` and the existing planning/fact-derivation
pipeline.

## Thermal semantics

Linux thermal sysfs reports `temp` and trip temperatures in millidegrees
Celsius. ElasticXxx reads the configured thermal-zone directory, finds every
`trip_point_*_type` equal to `critical`, chooses the lowest corresponding trip
temperature, and emits:

```text
thermal-margin = (critical_mC - current_mC) / 1000
```

A negative margin is retained as a negative value. It is not clamped to zero.
ElasticXxx does not invent a thermal limit when the kernel exposes no critical
trip point.

Unit constant: `THERMAL_MARGIN_SOURCE_UNIT = "degrees-celsius"`.

## Power semantics

Linux hwmon `power*_input` is a direct instantaneous power channel in
microwatts. ElasticXxx converts the configured channel to watts:

```text
energy-rate = power_input_uW / 1_000_000
```

Only a file whose name matches `power*_input` is accepted by this provider.
ElasticXxx deliberately does **not** infer power by multiplying independently
reported voltage and current channels. Such reconstruction would need a
separate reviewed sensor/rail contract.

Unit constant: `ENERGY_RATE_SOURCE_UNIT = "watts"`.

`energy-rate` is instantaneous power, not an integrated energy counter.

## Provenance and failure behavior

The caller supplies explicit sysfs paths. This avoids assuming that thermal-zone
or hwmon numeric indices are stable across machines or boots. Every emitted
observation records the configured path in its `ObservationSource`.

Missing files, malformed integer values, missing critical thermal trips, invalid
channel names, or values outside the supported numeric transport produce an
explicit unsupported observation. Unsupported observations do not enter the
`PlanningContext`, so downstream three-valued predicates remain `Unknown` rather
than receiving a fabricated zero.

The observation timestamp is captured at collection time with `Instant`; normal
ElasticXxx freshness predicates remain responsible for deciding whether a
reading is still usable.

## Example

```bash
cargo run -p elastic --example linux_thermal_energy -- \
  /sys/class/thermal/thermal_zoneN \
  /sys/class/hwmon/hwmonN/power1_input
```

The example takes paths explicitly; it contains no board-specific path or
thermal threshold.

During BE14h qualification on an NVIDIA Jetson AGX Thor development host, Linux
exposed real CPU/SoC thermal zones with critical trip points and an INA238 direct
`power1_input` channel. That observation establishes that a real provider exists
for this reference environment. It does not establish an energy saving,
performance improvement, safe operating threshold, or production actuation
policy.

The `BE14h real thermal power observer` workflow repeats this qualification on a
trusted Thor runner. It discovers sensors by the kernel names `cpu-thermal` and
`ina238`, not by unstable numeric sysfs indices, then runs the public example and
requires finite source-backed `thermal-margin` and `energy-rate` observations.

## Trust boundary

These providers only implement **OBSERVE**. A future BE14h policy may consume
these readings for Boolean eligibility or numeric planning, but a `True` guard
will still not authorize arbitrary clock, fan, power-mode, scheduler, or device
changes. Any physical control path must define its own capabilities, validation,
verification, rollback/fail-closed behavior, and differential baseline before
promotion.
