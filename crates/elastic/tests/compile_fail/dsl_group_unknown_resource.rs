use elastic::prelude::*;

elastic! {
    document broken_group {
        resource flight {
            class(configurational);
            allow(capacity);
        }
        group drone {
            members(flight, missing);
        }
    }
}

fn main() {}
