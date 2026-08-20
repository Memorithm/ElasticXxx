# Trisk: Task-Centric Data Stream Reconfiguration

## Status

**SOURCE-DERIVED MECHANISM REVIEW + ELASTIC RELATION.**

Reference: Yancan Mao et al., *Trisk: Task-Centric Data Stream Reconfiguration*, ACM SoCC 2021. Code artifact: `sane-lab/Trisk`.

## 1. Problem

Trisk addresses online reconfiguration of distributed stream-processing jobs without forcing control-policy authors to encode low-level Flink reconfiguration machinery for every strategy.

Its central abstraction is **task-centric**: control can change three dimensions of a running execution plan:

1. workload distribution;
2. execution logic;
3. resource allocation / placement.

## 2. Abstract execution plan

The artifact's `ExecutionPlan` exposes both observations and mutations. Representative mutation APIs are:

```text
assignWorkload(operator, distribution)
assignExecutionLogic(operator, function)
assignResources(operator, deployment)
update(custom transformation)
```

The concrete `TriskImpl` records affected transformations such as upstream/downstream mapping changes, state redistribution, remapping, redeployment and function update.

This establishes strong prior art for separating a desired reconfiguration from the low-level physical execution steps.

## 3. Primitive operations

The artifact's `PrimitiveOperation` interface exposes low-level asynchronous operations including:

```text
prepareExecutionPlan
synchronizeTasks
updateTaskResources
updateKeyMapping
updateState
updateFunction
```

The source explicitly describes these as low-level primitive operations. They are composed by a control plane rather than being exposed as raw runtime internals to a policy author.

## 4. Policy / mechanism boundary

Trisk's architecture separates control policies/controllers from the reconfiguration executor. The controller manipulates an abstract execution plan; the runtime computes and applies physical differences.

**SOURCE-DERIVED conclusion:** composable reconfiguration primitives and policy/mechanism separation are established prior art. ElasticXxx must not claim either idea by itself as novel.

## 5. Synchronization and affected tasks

Trisk synchronizes affected tasks and uses partial pause/resume instead of necessarily stopping the entire job. The artifact explicitly represents sets of affected tasks for transformations.

This is consistent with later work such as Fries, which more formally minimizes the consistency region and gives transactional semantics to reconfiguration.

## 6. Elastic relation

### ADOPT

- desired execution-plan state separate from physical execution mechanism;
- primitive operations that can be composed into larger reconfigurations;
- explicit identification of affected components/tasks;
- policy/control-plane separation from runtime actuation.

### ADAPT

Trisk's primitive vocabulary is stream/Flink specific. ElasticXxx needs resource-semantic primitives whose legality is defined by the resource adapter rather than by one streaming engine.

Candidate general shape:

```text
ReconfigurationPrimitive {
    preconditions,
    effects,
    required_capabilities,
    consistency_requirements,
    apply,
    verification,
    reversibility_or_compensation,
}
```

This is an **ELASTIC PROPOSAL**, not Trisk terminology.

### ADAPT — composition safety

A sequence of individually valid primitive operations is not automatically a valid transaction. Chi and Fries show that concurrent/reordered reconfigurations can require explicit serialization/consistency semantics.

Therefore Elastic primitive composition should interact with:

```text
EffectSet
ConsistencyClosure
TransitionOperationGraph
ReconfigurationTransaction
```

rather than rely only on imperative sequencing.

### REJECT from generic core

Do not hardcode concepts such as Flink key groups, JobVertex, TaskManager, mailbox pause semantics, or Flink slot IDs into Elastic core.

## 7. Candidate Elastic primitive families

A preliminary domain-independent vocabulary may include:

```text
Acquire
Release
Resize
MoveAuthority
TransferState
ChangeRouting
ChangeRepresentation
Replicate
DropReplica
Recompute
Quiesce
Resume
Verify
Commit
```

This list is not yet an API commitment. The test is whether these primitives span multiple resource domains without collapsing important semantics.

## 8. Experiment

**EXPERIMENT REQUIRED.** Express at least the following reconfigurations using the same primitive/effect framework:

1. CPU worker resize;
2. RAM↔VRAM migration;
3. KV representation change;
4. shard redistribution;
5. replica placement change;
6. task-routing update.

Measure:

- number of primitives;
- validation overhead;
- closure size;
- rollback/compensation needs;
- amount of domain-specific escape-hatch code;
- whether the abstraction hides any correctness-critical distinction.

## 9. SciRust

No SciRust gap is implied. Trisk primarily contributes control-plane/runtime architecture and execution mechanisms, not a missing general scientific primitive.
