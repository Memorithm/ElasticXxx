use elastic::prelude::*;

elastic! {
    document duplicate {
        resource worker {
            class(shared);
            allow(capacity);
        }
        resource worker {
            class(shared);
            allow(concurrency);
        }
    }
}

fn main() {}
