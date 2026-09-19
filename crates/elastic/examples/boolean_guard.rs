//! Run with `cargo run -p memorithm-elastic --example boolean_guard`.
//! This example constructs and evaluates policy data; it performs no actuation.

use elastic::prelude::*;
use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let capacity = predicate("example.ram", "capacity-ok")?;
    let pressure = predicate("example.ram", "pressure-critical")?;
    let predicates = ElasticPredicates::new([capacity.clone(), pressure.clone()])?;
    let guard = elastic_guard! {
        predicates: predicates,
        scope: GuardScope::Resource,
        when: (capacity && !pressure),
    }?;

    let mut facts = BTreeMap::from([(capacity, TruthValue::True)]);
    println!("missing pressure evidence: {:?}", guard.evaluate(&facts)?);
    facts.insert(pressure.clone(), TruthValue::False);
    println!("explicitly safe pressure: {:?}", guard.evaluate(&facts)?);
    facts.insert(pressure, TruthValue::True);
    println!(
        "explicitly critical pressure: {:?}",
        guard.evaluate(&facts)?
    );
    println!("guard identity: {}", guard.fingerprint());
    println!("No resource transition or actuation was performed.");
    Ok(())
}
