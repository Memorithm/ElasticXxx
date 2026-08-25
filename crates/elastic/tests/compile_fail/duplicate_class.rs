use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(
    class(representational),
    class(stateful),
    allow(capacity)
)]
struct Bad;

fn main() {}
