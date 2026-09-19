use elastic::prelude::*;

elastic! {
    resource worker_pool {
        allow(concurrency);
    }
}

fn main() {}
