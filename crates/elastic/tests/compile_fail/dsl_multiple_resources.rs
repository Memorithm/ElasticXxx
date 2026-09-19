use elastic::prelude::*;

elastic! {
    resource first {
        class(stateful);
        allow(capacity);
    }
    resource second {
        class(stateful);
        allow(capacity);
    }
}

fn main() {}
