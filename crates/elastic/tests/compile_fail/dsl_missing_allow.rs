use elastic::prelude::*;

elastic! {
    resource worker_pool {
        class(stateful);
    }
}

fn main() {}
