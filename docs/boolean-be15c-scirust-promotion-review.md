# BE15c — SciRust promotion review

Status: **candidate no-promotion decision until exact-head CI/review/merge evidence is recorded**.

This review compares ElasticXxx main `b56cb87ae8b5e203293eb09cf5dafc1a921ccee3`
with SciRust master `d3ef8a2687163cffcb85d2cc38e10d4ce2d4ea3b`. It applies the BE15c rule:
only a mathematically general primitive with multiple independent real consumers may be
promoted to SciRust, and promotion must not introduce an ElasticXxx ↔ SciRust dependency
cycle.

## Candidate inventory

| ElasticXxx surface | Generality review | Independent-consumer evidence | Decision |
| --- | --- | --- | --- |
| `TruthValue` strong-Kleene operations | mathematically general semantics | current qualified uses are ElasticXxx runtime surfaces plus destination-owned BooleanLab/TDI regression adapters; those adapters are not independent SciRust consumers | do not promote yet |
| `BoolExpr`, `PredicateId`, `FactSet`, compiled guards | representation is shaped around bounded Elastic eligibility evaluation and fast-path compilation | no second independent runtime consumer established | keep in ElasticXxx |
| exact Boolean/Kleene oracles | underlying logic is general, but the current API is expressed directly over Elastic `BoolExpr`/`PredicateId` | no independent consumer of this API established | keep in ElasticXxx; do not copy an Elastic-shaped API into SciRust |
| pseudo-Boolean constraints | integer inequalities are general, but current declarations bind stable Elastic `PredicateKey`, units/scales and decision-trace semantics | no independent consumer of the exact Elastic API established | keep in ElasticXxx |
| multiword fact/guard screening | implementation is a hardware-friendly Elastic policy-evaluation optimization | no independent scientific consumer established | keep in ElasticXxx |

SciRust already owns exact Boolean-function mathematics in `scirust-modalg`, including
Möbius/ANF degree and Walsh-derived metrics. BE15c therefore does not copy those algorithms
back out of SciRust or create competing Elastic implementations.

## Dependency decision

No SciRust dependency is added to ElasticXxx. Existing ElasticXxx design contracts state that
SciRust is external scientific/R&D tooling rather than an ElasticXxx runtime dependency.
Conversely, SciRust's current ecosystem roadmap assigns adaptive resource runtime semantics to
ElasticXxx. Adding either repository as a runtime dependency of the other solely to share the
current Boolean policy types would weaken that ownership boundary and create a future cycle risk.

The correct BE15c result at these exact source revisions is therefore **no promotion**. This is a
negative architectural result, not an assertion that the primitives can never be shared. A future
promotion requires concrete evidence of at least two independent consumers of the same
mathematical contract, an API factored free of Elastic resource/runtime semantics, independent
reference tests, and a dependency direction that remains acyclic.

This review makes no performance, novelty, scientific-result, actuation, release, or compatibility
claim. It does not alter the qualified BE14/BE15a/BE15b runtime behavior.
