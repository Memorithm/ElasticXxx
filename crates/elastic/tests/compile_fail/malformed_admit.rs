use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(
    class(stateful),
    allow(capacity),
    admit(reencode capacity)
)]
struct MalformedAdmit;

fn main() {}
