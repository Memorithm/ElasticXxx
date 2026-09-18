# BE15b — TDI-9.3 non-final representation adapter

Status: **candidate until exact-head CI/review/merge evidence is recorded**.

This slice consumes the versioned, non-final TDI-9.3 C3 representation contract
from TDI PR #505, merged as
`7dab3bfa97e74eeff7965cefcada56c59b4322ab`. ElasticXxx keeps a byte-exact
snapshot plus source-module and fixture digests. CI verifies those pins without a
runtime or network dependency on TDI.

The production boundary is intentionally narrow. `elastic-adapters` exposes
`Tdi93C3FactsV1` and the nine `Tdi93C3PredicateV1` positions. Fully grounded TDI
binary rows map losslessly to Elastic `True`/`False`. A missing downstream
predicate maps to Elastic `Unknown`, never to TDI `false`, and any row containing
`Unknown` cannot be projected back into the binary TDI carrier. Stable
`PredicateKey` values use the versioned namespace `tdi.9.3.c3.v1`; ephemeral
`PredicateId` values are not persisted by the adapter.

The differential suite consumes all 512 pinned source rows and verifies binary
roundtrip for every position. It then removes every predicate in every row and
verifies `Unknown` plus fail-closed guard evaluation. The fixture partition
(120 action-labelled, 8 unrecoverable, 384 invalid) is checked only as source
snapshot integrity. ElasticXxx does **not** reimplement the TDI carrier validator
or action policy and exposes no TDI action API.

This bridge grants no TDI-9.1 freeze authority, TDI-9.2 confirmation authority,
preregistration/holdout/final permission, scientific verdict, Elastic validation,
or physical actuation authority. It contains no trajectory samples, thresholds,
quality measurements, performance measurements, or final/protected experiment
data. TDI remains authoritative for its scientific semantics; ElasticXxx owns
only the generic three-valued representation boundary.
