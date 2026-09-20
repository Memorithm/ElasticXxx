#![no_main]

use elastic_runtime::{OperatorConfig, MAX_OPERATOR_CONFIG_BYTES};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(config) = OperatorConfig::from_bounded_json(data) else {
        return;
    };

    let encoded =
        serde_json::to_vec(&config).expect("an accepted operator config must remain serializable");

    // The decoder's input bound is authoritative. Canonical JSON may only be
    // re-fed when it still fits that same public byte budget.
    if encoded.len() <= MAX_OPERATOR_CONFIG_BYTES {
        let decoded = OperatorConfig::from_bounded_json(&encoded)
            .expect("an accepted bounded operator config must survive canonical round-trip");
        assert_eq!(config, decoded);
    }

    config
        .validate()
        .expect("bounded operator decoding only returns semantically valid configurations");
});
