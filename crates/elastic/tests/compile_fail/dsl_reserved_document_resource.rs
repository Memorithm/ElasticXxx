use elastic::prelude::*;

elastic! {
    document bad_name {
        resource document {
            class(shared);
            allow(capacity);
        }
    }
}

fn main() {}
