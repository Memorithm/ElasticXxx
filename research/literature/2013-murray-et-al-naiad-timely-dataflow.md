# Murray et al. (2013) — Naiad: A Timely Dataflow System

## Status

**SOURCE-DERIVED mechanism review with ELASTIC PROPOSALS clearly separated.**

Primary source: Derek G. Murray, Frank McSherry, Rebecca Isaacs, Michael Isard, Paul Barham, Martín Abadi, *Naiad: A Timely Dataflow System*, SOSP 2013.

## Problem

Naiad targets distributed dataflow computations that simultaneously need:

- low-latency streaming;
- iterative/cyclic computation;
- incremental updates;
- consistent intermediate results and notifications.

Its central mechanism is **timely dataflow**, where logical timestamps encode structure such as input epochs and nested loop iterations.

## Logical time

Messages carry timestamps such as:

```text
(epoch, loop_counter_1, ..., loop_counter_k)
```

The timestamp is not wall-clock time. It represents a logical point in the computation and is transformed by ingress, egress, and feedback edges.

The graph structure and timestamp transformations define which events can causally result in which later events.

## Pointstamps

Naiad combines a logical timestamp with a graph location into a **pointstamp**:

```text
Pointstamp = (logical timestamp, location)
```

This matters because a timestamp alone does not establish causal precedence between work at different operators. The topology contributes to the `could-result-in` relation.

## Frontiers and notifications

For active pointstamps, Naiad tracks occurrence and precursor counts. A pointstamp with no active precursor belongs to the frontier of active pointstamps.

The scheduler may deliver a notification only when it can establish that no outstanding event can still result in data at or before the requested logical point.

The distributed implementation maintains local views of progress. The paper states an important safety property: a local frontier does not move ahead of the true global frontier, allowing notifications to be delivered safely from local knowledge once the corresponding frontier condition is met.

## Why this matters for Elastic

A frontier is not simply “the latest epoch.” In a partially ordered execution there may be several incomparable minimal points still in flight.

Therefore a distributed adaptive runtime should not assume that safe decisions always correspond to one scalar global version.

Examples of operations that may need frontier semantics include:

- releasing historical state;
- compacting deltas;
- declaring a derived materialization complete;
- delivering a stable observation to a planner;
- committing some multi-resource transitions;
- deciding that no earlier causal work can still invalidate a result.

## Coordination cost

Naiad does not treat progress tracking as free. A naive implementation would broadcast every progress change and create impractical communication volume. The paper introduces optimizations to reduce progress-tracking traffic.

This reinforces a recurring Elastic principle:

```text
observation / coordination metadata has a cost
```

and a more precise version:

```text
stronger global knowledge can require more coordination
```

## Stateful vertices and execution

Vertices may maintain state and process timestamped records asynchronously. Cycles are explicit in the graph. Timely dataflow permits multiple epochs/iterations to execute concurrently while still providing a mechanism to know when a logical region of work is complete.

## Elastic relation

### ADOPT

- Separate wall-clock time from logical version/progress.
- Treat progress as a partial-order problem where appropriate.
- Represent a progress boundary by a set/frontier rather than forcing a single scalar timestamp.
- Charge progress tracking and coordination to the system's overhead budget.

### ADAPT

ElasticXxx does not need to reproduce Naiad's dataflow runtime. Instead, domains that need distributed causal progress may expose something equivalent to:

```text
LogicalVersion
CausalLocation
ProgressFrontier
```

The exact semantics remain domain-specific.

### REJECT

Do not impose pointstamps or timely-dataflow graph restrictions on resources whose transitions are simple and locally ordered. A scalar generation remains the cheaper and clearer mechanism when it is sufficient.

## Elastic proposal: safe frontier gates

A future trusted-runtime primitive may conceptually support a condition such as:

```text
SafeAfter(frontier, operation)
```

where an operation becomes legal only after the relevant causal frontier has advanced far enough.

Examples:

```text
compact history before F
release old replica after F
publish stable derived result after F
commit migration after all pre-F writers drain
```

This is an ELASTIC PROPOSAL, not a claim that Naiad exposes this exact API.

## Interaction with epochs/generations

Keep three distinct concepts:

```text
ResourceGeneration
    freshness / revocation for one logical resource

LogicalVersion
    domain-specific point in partially ordered computation

ProgressFrontier
    antichain/minimal boundary describing outstanding causal work
```

Conflating these would make simple local freshness checks unnecessarily expensive and distributed progress checks incorrectly weak.

## Experiment required

Construct a cyclic two-stage resource workflow with two independent logical dimensions and compare:

1. one global scalar epoch barrier;
2. local scalar generations without causal frontier;
3. partial-order frontier tracking.

Measure false-early commits, waiting time, coordination messages, metadata footprint, and useful progress.
