use elastic::prelude::*;

elastic! {
    document broken_policy {
        resource ram {
            class(configurational);
            allow(capacity);
        }
        policy bad {
            version(1, 0, 0);
            target(ram);
        }
    }
}

fn main() {}
