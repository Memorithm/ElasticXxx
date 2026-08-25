use elastic::prelude::*;

#[derive(ElasticResource)]
#[elastic(
    class(stock),
    allow(capacity)
)]
enum NotAStruct {
    Variant,
}

fn main() {}
