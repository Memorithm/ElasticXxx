use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(class(stateful))]
struct MissingAllow;

fn main() {}
