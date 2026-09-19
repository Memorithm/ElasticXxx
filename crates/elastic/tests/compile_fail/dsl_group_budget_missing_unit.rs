use elastic::prelude::*;

elastic! {
    document broken_group {
        resource a {
            class(configurational);
            allow(capacity);
        }
        resource b {
            class(configurational);
            allow(capacity);
        }
        group g {
            members(a, b);
            budget memory {
                quantum(1);
                maximum(4);
                term(a, predicate("elastic.test", "a"), 2);
            }
        }
    }
}

fn main() {}
