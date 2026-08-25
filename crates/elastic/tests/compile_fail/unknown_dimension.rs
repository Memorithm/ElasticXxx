use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(
    class(stateful),
    allow(capacity, representaton)
)]
struct TypoDimension;

fn main() {}
