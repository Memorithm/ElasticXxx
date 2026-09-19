use elastic::prelude::*;

elastic! {
    resource worker_pool {
        class(stateful),
        allow(concurrency);
    }
}

fn main() {}
