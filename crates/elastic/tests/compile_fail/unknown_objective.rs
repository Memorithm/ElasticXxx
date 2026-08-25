use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(
    class(stateful),
    allow(capacity),
    optimize(latnecy)
)]
struct TypoObjective;

fn main() {}
