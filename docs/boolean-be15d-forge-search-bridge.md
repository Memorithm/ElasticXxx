# BE15d — Forge optional search bridge

Status: **candidate until exact-head CI/review/merge evidence is recorded**.

This slice starts from ElasticXxx main `f0c1d65287810eb0ec693205e65568cef7354b03`
and reviews Forge main `da68e9703d7a523f5d3703ebedd3de85b39cb153`. Forge already publishes
versioned generic external-domain and candidate-envelope contracts. ElasticXxx does not add a
runtime dependency on Forge: the destination runtime owns the candidate intake schema and
revalidates every proposed Boolean guard and pseudo-Boolean constraint with canonical Elastic
types before the proposal can enter later planning or validation.

`ForgeSearchCandidateV1` is bounded and fail-closed. It accepts only schema v1, requires the
producer repository identity `Memorithm/Forge`, checks Git/SHA syntax, rejects oversized/deep
JSON, validates the embedded stable-key `GuardConfigV1`, rejects constraint predicates that are
not declared by that configuration, requires canonical base-10 `i128` strings, and lowers through
`PseudoBooleanConstraintDeclaration`. Unknown/future/malformed input is a rejection.

The producer commit, candidate id and fingerprints are provenance fields, not authentication.
`ForgeRevalidatedPolicyV1` contains descriptive guard/constraint policy only; it exposes no
actuator, permit, verification verdict or promotion bit. Forge may propose/mutate candidate forms,
but executed evidence remains authoritative and the ordinary Elastic `VALIDATE` boundary remains
mandatory before any actuation. This slice does not qualify Forge FG5 end to end, does not claim a
sandbox or remote-worker trust boundary, and makes no performance, novelty or scientific-result
claim.
