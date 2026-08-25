use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(
    class(custom("agent-memory" junk)),
    allow(capacity)
)]
struct NestedTrailing;

fn main() {}
