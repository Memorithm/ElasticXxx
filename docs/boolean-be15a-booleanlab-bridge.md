# BE15a BooleanLab exact-vector consumer bridge

Status: **destination-owned regression bridge; no runtime or scientific claim**.

ElasticXxx snapshots the versioned BooleanLab `exact-vectors-v1` test fixture and adjacent provenance manifest from `Memorithm/BooleanLab` main `2646e7ca675d3dcde71abed10c7531e1d36c93b8`. The fixture was originally generated on BooleanLab source commit `7fd929a62cfc219a525b21ff02801fbc8bcb012e`; the source manifest pins the generator module SHA-256 and fixture SHA-256. ElasticXxx carries no BooleanLab runtime dependency.

The consumer test recomputes the fixture SHA-256 locally, parses the bounded RPN grammar independently, and evaluates every fully grounded three-bit assignment through ElasticXxx's generic `BoolExpr`, `CompiledGuard`, and `MultiwordCompiledGuard` surfaces. It also checks satisfiability and tautology with the dependency-free exact oracle and maps that oracle through the solver-neutral `SymbolicBackend` boundary as a test-only adapter. Any mismatch fails the destination repository's tests.

The BooleanLab fingerprint and algebraic metrics remain provenance/experimental metadata. ElasticXxx does not reinterpret them as proof, novelty, runtime authority, validation authority, or actuation permission. The copied fixture is immutable test data: future BooleanLab versions require an explicit new snapshot/version and destination-owned requalification rather than silent replacement.

No performance, memory, branch, hardware, model-quality, energy, scientific-confirmation, or production-actuation claim follows from passing these vectors.
