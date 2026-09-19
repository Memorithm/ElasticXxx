use elastic::prelude::*;

elastic! {
    resource worker_pool {
        class(stateful);
        allow(telepathy);
    }
}

fn main() {}
