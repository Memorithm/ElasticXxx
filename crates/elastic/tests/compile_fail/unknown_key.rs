use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(
    clas(representational),
    allow(capacity)
)]
struct Bad;

fn main() {}
