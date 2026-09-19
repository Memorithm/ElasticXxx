# ElasticXxx 0.1.x migration contract

Status: pre-release migration notes. No registry publication is authorized by this document.

## Current boundary

The supported downstream Rust entry point is the `elastic` facade. The workspace is still on version `0.1.0`; there is no earlier ElasticXxx registry release whose users must be silently migrated. Consumers pinned to Git commits remain source-qualified against those exact commits and do not automatically inherit later repository changes.

Persisted Boolean guards, operator configuration, decision traces, evidence envelopes, and other versioned wire contracts keep their declared schema identities. A reader must reject a future schema it does not implement. A strict v1 object must not be extended incompatibly under the same schema number.

## Rules for a future incompatible change

An intentional incompatible public-facade change requires all of the following before merge or release:

1. move to a new pre-1.0 minor line (`0.2.0` or later), unless a narrowly documented safety/correctness exception makes retention impossible;
2. add explicit source-to-target migration instructions here or in a successor migration document;
3. preserve old persisted evidence as historical evidence rather than rewriting it in place;
4. introduce a new wire schema identity for incompatible serialized semantics;
5. requalify facade-only downstream compilation and every affected cross-repository consumer at exact source revisions;
6. rerun the declared MSRV, packageability, rustdoc, fuzz/Miri-applicable, and ordinary CI gates on the exact release candidate.

## Current cross-repository consumers

The machine-readable source pins live in [`PRODUCTIZATION-V1.json`](PRODUCTIZATION-V1.json), with scope explained in [`CROSS_REPO_COMPATIBILITY.md`](CROSS_REPO_COMPATIBILITY.md). These pins are provenance and compatibility evidence, not permission for ElasticXxx to absorb domain semantics owned by those repositories.

## Non-goals

These notes do not authorize a registry publish, assert package-name ownership, claim stable 1.0 semantics, relicense third-party material, or convert research/fixture evidence into production, performance, hardware, or scientific claims.
