#![no_main]

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;

use elastic_core::{IssuerId, RepresentationEpoch, RepresentationId, TransitionError};

const MAX_ID_LEN: usize = 256;

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let len = u.arbitrary::<u16>().unwrap_or(0) as usize;
    let payload = u.bytes(len.min(data.len())).unwrap_or(data);

    let text = String::from_utf8_lossy(payload).into_owned();
    let text_len = text.len();

    match RepresentationId::new(text.clone()) {
        Ok(id) => {
            assert_eq!(id.as_str(), text);
            assert!(id.as_str().len() <= MAX_ID_LEN);
            assert!(!id.as_str().trim().is_empty());
            assert!(!id.as_str().starts_with(char::is_whitespace));
            assert!(!id.as_str().ends_with(char::is_whitespace));
        }
        Err(error) => match error {
            TransitionError::EmptyRepresentationId => assert!(text.trim().is_empty()),
            TransitionError::UntrimmedIdentifier => {
                assert!(
                    text.starts_with(char::is_whitespace)
                        || text.ends_with(char::is_whitespace)
                );
                assert!(!text.trim().is_empty());
            }
            TransitionError::IdentifierTooLong { len } => {
                assert_eq!(len, text_len);
                assert!(len > MAX_ID_LEN);
            }
            other => panic!("unexpected identifier error: {other:?}"),
        },
    }

    match IssuerId::new(text.clone()) {
        Ok(issuer) => assert_eq!(issuer.as_str(), text),
        Err(TransitionError::EmptyIssuerId) => assert!(text.trim().is_empty()),
        Err(TransitionError::UntrimmedIdentifier) => {
            assert!(
                text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace)
            );
        }
        Err(TransitionError::IdentifierTooLong { len }) => {
            assert_eq!(len, text_len);
            assert!(len > MAX_ID_LEN);
        }
        Err(other) => panic!("unexpected issuer error: {other:?}"),
    }

    let raw = u.arbitrary::<u64>().unwrap_or(u64::MAX);
    let epoch = RepresentationEpoch::new(raw);
    if raw == u64::MAX {
        assert_eq!(epoch.next(), Err(TransitionError::EpochOverflow));
    } else {
        assert_eq!(epoch.next().map(RepresentationEpoch::get), Ok(raw + 1));
    }
    assert!(!epoch.lt(epoch));
});
