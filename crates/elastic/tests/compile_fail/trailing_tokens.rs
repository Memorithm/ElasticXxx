use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(
    class(stateful not-a-class),
    allow(capacity)
)]
struct TrailingTokens;

fn main() {}
