use elastic::prelude::*;

elastic! {
    document broken_policy {
        resource ram {
            class(configurational);
            allow(capacity);
        }
        policy bad {
            id("runtime.bad");
            version(1, 0, 0);
            target(ram);
            predicate(ok, "elastic.test", "a");
            predicate(ok, "elastic.test", "b");
        }
    }
}

fn main() {}
