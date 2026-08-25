use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(allow(capacity))]
struct MissingClass;

fn main() {}
