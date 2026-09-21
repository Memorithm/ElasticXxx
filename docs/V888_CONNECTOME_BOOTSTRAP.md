# BANC V888 sparse-runtime elasticity bootstrap for ElasticXxx

Status: research integration bootstrap. ElasticXxx adapts resources; it does not decide whether a connectomic architecture is scientifically useful.

## Scope

Only FlyWire BANC v888 is in scope for connectome-derived experiments. ElasticXxx does not ingest or store raw V888 data. It receives workload/resource telemetry from SciRust, NNIS, FLAT-ATTENTION or SML through versioned contracts.

Codex currently identifies BANC v888 as Female Adult Fly Brain and Nerve Cord, snapshot 2026-05-20, with 158,262 neurons and 3,037,361 aggregated connections.

Sources:
- https://codex.flywire.ai/?dataset=banc
- https://codex.flywire.ai/faq

## Opportunity

Sparse recurrent/event-driven workloads have variable instantaneous cost. Relevant state includes:
- active-node ratio;
- active-edge ratio;
- event queue depth;
- event arrival rate;
- graph frontier size;
- memory residency;
- sparse/dense crossover;
- recurrent depth;
- attention candidate density in FLAT hybrids.

These are natural elastic dimensions only when the runtime can switch representations/execution plans without changing model semantics.

## Bootstrap sequence

### EX-V888-0 — telemetry schema

Define versioned observations:
- active_nodes;
- active_edges;
- total_edges;
- queue_depth;
- event_rate;
- frontier_size;
- resident_graph_bytes;
- candidate_attention_edges;
- recurrent_cycle;
- latency window.

Unknown/missing observations remain Unknown and cannot authorize unsafe actuation.

### EX-V888-1 — execution-mode resource

Model legal states:
- fixed-step sparse;
- event-driven;
- batched event window;
- dense fallback only where semantically equivalent.

Declare invariants that preserve model state and event ordering semantics.

### EX-V888-2 — sparse representation resource

Candidate representations:
- CSR;
- CSC;
- dual CSR+CSC;
- compact active-edge list;
- module-partitioned adjacency.

Representation transitions require exact memory accounting and state-equivalence validation.

### EX-V888-3 — device-placement resource

Allow CPU/WGPU placement through NNIS capability contracts:
- no NVIDIA-specific requirement;
- explicit transfer cost;
- preserve state identity;
- reject unsupported plans before actuation.

### EX-V888-4 — event-window elasticity

Adapt bounded event batching/window size based on:
- queue pressure;
- latency objective;
- memory budget.

Verify event-order equivalence or declared approximation policy before commit.

### EX-V888-5 — FLAT candidate-density integration

For hybrid attention:
- observe admitted-key density;
- choose qualified sparse versus dense FLAT kernel only when both implement equivalent semantics;
- include switching cost;
- never change the routing predicate to win latency unless the model contract explicitly allows it.

### EX-V888-6 — SML active-page/edge integration

Coordinate:
- SML weight-page residency;
- graph active front;
- recurrent depth;
- memory budget.

Keep each dimension independently observable so savings are not double-counted.

### EX-V888-7 — forecast models

Start with simple deterministic baselines:
- last value;
- moving average;
- bounded linear trend.

Only then test learned forecasts of activity/frontier/queue pressure. Forecast failure may not bypass invariants.

### EX-V888-8 — policy evaluation

Compare:
- fixed static plan;
- threshold policy;
- forecasted adaptive policy;
- Boolean guard-assisted policy.

Measure latency, memory, transition count, rollback count and objective value.

### EX-V888-9 — rollback/crash semantics

Prove:
- failed representation/device transition does not corrupt recurrent state;
- event queues are restored or fail closed;
- partially applied buffer migrations cannot be committed;
- evidence tokens identify the exact before/after state.

### EX-V888-10 — cross-repository publication gate

Publish stable contracts only after:
- NNIS supplies real actuation;
- SciRust supplies deterministic equivalence oracles;
- FLAT/SML define semantics;
- TDI supplies a workload that makes adaptation scientifically meaningful.

## Non-goals

ElasticXxx must not:
- embed V888 topology;
- invent neuron dynamics;
- choose a topology;
- retrain SML;
- alter attention masks for performance without semantic authorization;
- infer that lower resource use means higher model quality.

## Exit gate

The bootstrap is complete when one real sparse recurrent workload can execute the full:
OBSERVE -> FORECAST -> PLAN -> VALIDATE -> ACT -> VERIFY -> COMMIT/ROLLBACK
loop across at least two semantically equivalent execution/representation states on a non-NVIDIA path, with static-baseline comparison and replayable evidence.
