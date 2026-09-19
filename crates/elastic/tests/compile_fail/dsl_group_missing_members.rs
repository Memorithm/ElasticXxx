use elastic::prelude::*;

elastic! {
    document broken_group {
        resource flight {
            class(configurational);
            allow(capacity);
        }
        group drone {
            depends(flight -> flight);
        }
    }
}

fn main() {}
