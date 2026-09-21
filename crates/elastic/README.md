# elastic (package: `memorithm-elastic`)

User-facing Rust facade for **ElasticXxx**: a typed adaptive-resource runtime
that observes, forecasts, plans, validates, actuates, verifies, and
commits or rolls back under explicit invariants.

Install the registry package `memorithm-elastic` and import the stable crate
name `elastic`:

```toml
[dependencies]
elastic = { package = "memorithm-elastic", version = "0.1.0" }
```

> **Pre-1.0 / not yet published.** Workspace manifests keep `publish = false`.
> crates.io publication is a separate authorization; this README is packaging
> documentation only and does not claim registry ownership or availability.

## Minimal example

```rust
use elastic::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("session-kv")?,
    )
    .allow(DimensionId::REPRESENTATION)
    .preserve(Invariant::new(InvariantKind::PreserveContents))
    .optimize(ObjectiveId::LATENCY)
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .build()?;

    let document = lower(&spec)?;
    println!("{document}");
    Ok(())
}
```

Embedded declarations via `elastic!` and `#[derive(ElasticResource)]` lower to
the same typed core. Prefer this facade over direct imports of implementation
crates (`elastic-core`, `elastic-eir`, …).

## Compatibility

| Item | Value |
| --- | --- |
| MSRV | Rust **1.89** |
| Release line | `0.1.x` (pre-1.0; see repository compatibility policy) |
| License | [PolyForm Noncommercial 1.0.0](https://github.com/Memorithm/ElasticXxx/blob/main/LICENSE.md); commercial path in [LICENSING.md](https://github.com/Memorithm/ElasticXxx/blob/main/LICENSING.md) |
| Repository | https://github.com/Memorithm/ElasticXxx |

## Non-claims

This crate documents contracts and APIs. It does **not** claim production
performance, FPS/latency/memory/energy wins, scientific novelty, or crates.io
name ownership. Benchmarks and research notes in the repository are scoped
evidence, not product guarantees.

## Further reading

- Repository release docs: `docs/release/` (packageability, naming, compatibility)
- Language and surface docs under `docs/language/` and `docs/surface/`
