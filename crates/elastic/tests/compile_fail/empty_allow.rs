use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(
    class(stateful),
    allow()
)]
struct EmptyAllow;

fn main() {}
