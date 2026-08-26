#![forbid(unsafe_code)]

use elastic_core::{IssuerId, RepresentationId, TransitionError};

const MAX_ID_LEN: usize = 256;

#[test]
fn representation_id_enforces_trimmed_bounded_contract() {
    assert_eq!(
        RepresentationId::new(" kv.int4"),
        Err(TransitionError::UntrimmedIdentifier)
    );
    assert_eq!(
        RepresentationId::new("kv.int4 "),
        Err(TransitionError::UntrimmedIdentifier)
    );

    let exact = "r".repeat(MAX_ID_LEN);
    assert_eq!(
        RepresentationId::new(exact.clone())
            .expect("the exact byte limit must remain admissible")
            .as_str(),
        exact
    );

    let overlong = "r".repeat(MAX_ID_LEN + 1);
    assert_eq!(
        RepresentationId::new(overlong),
        Err(TransitionError::IdentifierTooLong {
            len: MAX_ID_LEN + 1,
        })
    );

    assert_eq!(
        RepresentationId::new("kv int4")
            .expect("interior whitespace is part of the identifier")
            .as_str(),
        "kv int4"
    );
}

#[test]
fn issuer_id_uses_the_same_trimmed_bounded_contract() {
    assert_eq!(
        IssuerId::new(" validator"),
        Err(TransitionError::UntrimmedIdentifier)
    );
    assert_eq!(
        IssuerId::new("validator "),
        Err(TransitionError::UntrimmedIdentifier)
    );

    let exact = "i".repeat(MAX_ID_LEN);
    assert_eq!(
        IssuerId::new(exact.clone())
            .expect("the exact byte limit must remain admissible")
            .as_str(),
        exact
    );

    let overlong = "i".repeat(MAX_ID_LEN + 1);
    assert_eq!(
        IssuerId::new(overlong),
        Err(TransitionError::IdentifierTooLong {
            len: MAX_ID_LEN + 1,
        })
    );
}
