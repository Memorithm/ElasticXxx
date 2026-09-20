#![no_main]

use elastic_runtime::{EvidenceEnvelope, MAX_EVIDENCE_BYTES};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(envelope) = EvidenceEnvelope::from_slice(data) else {
        return;
    };

    envelope
        .validate()
        .expect("accepted evidence must remain semantically valid");
    let value = envelope
        .to_value()
        .expect("accepted evidence must remain representable as validated JSON");
    let encoded =
        serde_json::to_vec(&value).expect("validated evidence JSON serialization cannot fail");

    if encoded.len() <= MAX_EVIDENCE_BYTES {
        let decoded = EvidenceEnvelope::from_slice(&encoded)
            .expect("accepted bounded evidence must survive canonical round-trip");
        assert_eq!(envelope, decoded);
        decoded
            .summary()
            .expect("accepted evidence must retain a valid bounded summary");
    }
});
