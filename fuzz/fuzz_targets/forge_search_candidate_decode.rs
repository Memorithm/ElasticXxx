#![no_main]

use elastic_runtime::{ForgeSearchCandidateV1, MAX_FORGE_SEARCH_CANDIDATE_BYTES};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(candidate) = ForgeSearchCandidateV1::from_bounded_json(data) else {
        return;
    };

    candidate
        .validate()
        .expect("accepted Forge candidate must remain semantically valid");
    let encoded =
        serde_json::to_vec(&candidate).expect("accepted Forge candidate must remain serializable");
    if encoded.len() <= MAX_FORGE_SEARCH_CANDIDATE_BYTES {
        let decoded = ForgeSearchCandidateV1::from_bounded_json(&encoded)
            .expect("accepted Forge candidate must survive bounded round-trip");
        assert_eq!(candidate, decoded);
    }
});
