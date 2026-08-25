# Resource Adapters v0.1

**Status:** normative for `crates/elastic-adapters` (RAM budget and
concurrency permits).

---

## 1. Role in the architecture

Adapters are the **trusted boundary** between validated intent and physical
action — the layer the whitepaper reserves for capability discovery,
validation against live state, and actuation:

```text
declaration → EIR → planner proposal → ADAPTER VALIDATION → PHYSICAL ACTION
                                              ↑ invariants re-checked here
```

These first adapters are deliberately portable and dependency-free: their
effects are real (an actual allocation, an actual licensed width) but local.
No OS probing, no NUMA migration, no accelerator code — those belong to later,
platform-specific adapters.

## 2. The adapter contract (normative)

Both adapters demonstrate the same five rules:

1. **Declaration-first.** Every adapter constructs its
   `ResourceSpec` internally from typed built-ins, lowers it to EIR once at
   construction, and keeps the normalized node (`RamBudget::ir`). Invalid
   declarations cannot become adapters.
2. **Observations are derived, never probed.** `observe()` computes signals
   from adapter state plus operator-supplied configuration (`host_total`,
   `max_width` model trusted discovery results). No environment reads.
3. **Planners propose; adapters dispose.** Proposals are advisory candidates
   (`TransitionCandidate::with_magnitude`). Every action method
   (`apply`, width changes) **re-validates bounds, step limits, and
   invariants immediately before the effect** — identical conditions to the
   pre-check methods, so planners can predict refusals honestly.
4. **Invariants bind at action time.** `PreserveContents` on the RAM budget:
   shrinking below recorded in-use bytes is structurally refused
   (`WouldViolateContents`). `PreserveIdentity` + holder safety on permits:
   widths below active holders are refused (`WouldStrandHolders`).
5. **Structured refusals.** Every refusal is a typed `AdapterError`
   variant; nothing panics, nothing silently clamps.

## 3. RAM budget (`ram::RamBudget`)

- Materialization: a real zeroed allocation; growth reserves memory,
  shrink releases it.
- Elastic dimension: `capacity`; admitted mechanism: `reinterpret`
  (resize-in-place); required capability: same pair — grounded candidate.
- Optional `max_step`: maximum delta per action, demonstrating bounded
  adaptation (a planner proposing a jump larger than the step is refused).
- Usage ledger: `record_use`/`release_use` model bytes handed to the
  application; they drive the contents invariant.

## 4. Concurrency permits (`permits::ConcurrencyPermits`)

- Licensed execution width with an acquire/release ledger.
- Elastic dimension: `concurrency`; same grounded admission pattern as RAM.
- Width changes refuse to strand active holders; overflow and underflow of
  the ledger are structured errors.

## 5. Determinism and thread-safety

Observations are pure functions of state plus immutable configuration.
Adapters own their state exclusively (no interior locking); sharing across
threads is the caller's composition decision, mirroring the no-hidden-runtime
rule. All types are `Send + Sync`.

## 6. Non-goals

No page-cache integration, no huge pages, no cgroup/NUMA topology, no async
runtimes, no background actuators. Platform-specific discovery and migration
belong to later phases and must enter through this same contract.
