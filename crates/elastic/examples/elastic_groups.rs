//! Declare a multi-resource group with dependency, shared budget and invariant.
//! Run with `cargo run -p memorithm-elastic --example elastic_groups`.

use elastic::prelude::*;

elastic! {
    document edge_runtime {
        resource critical_service {
            class(configurational);
            id("critical-service");
            allow(capacity);
        }
        resource inference {
            class(configurational);
            id("inference");
            allow(capacity, energy);
        }
        resource vision {
            class(configurational);
            id("vision");
            allow(capacity, energy);
        }

        group onboard {
            members(critical_service, inference, vision);
            depends(inference -> critical_service);
            depends(vision -> critical_service);

            budget memory {
                unit("mib");
                quantum(1);
                maximum(8192);
                term(inference, predicate("elastic.example", "inference-large"), 6144);
                term(vision, predicate("elastic.example", "vision-large"), 3072);
            }

            invariant(
                contract("critical-service-reservation"),
                owner(critical_service),
                participants(critical_service, inference, vision)
            );
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let grouped = edge_runtime::grouped_document()?;
    let group = grouped.group("onboard").expect("declared group exists");

    println!("document fingerprint: {}", grouped.document().fingerprint());
    println!("group fingerprint: {}", group.fingerprint());
    println!("members: {}", group.members().join(", "));
    println!("dependencies: {}", group.dependencies().len());
    println!("shared budgets: {}", group.shared_budgets().len());
    println!(
        "cross-resource invariants: {}",
        group.cross_invariants().len()
    );

    Ok(())
}
