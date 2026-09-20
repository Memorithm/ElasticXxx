#![no_main]

use elastic_runtime::{GuardConfigV1, MAX_GUARD_CONFIG_BYTES};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(config) = GuardConfigV1::from_bounded_json(data) else {
        return;
    };

    config
        .validate()
        .expect("bounded guard decoding only returns semantically valid configurations");

    let encoded = config
        .to_bounded_json()
        .expect("an accepted guard config must re-encode within the public bounds");
    assert!(encoded.len() <= MAX_GUARD_CONFIG_BYTES);

    let decoded = GuardConfigV1::from_bounded_json(encoded.as_bytes())
        .expect("an accepted guard config must survive canonical round-trip");
    assert_eq!(config, decoded);

    decoded
        .lower()
        .expect("an accepted guard config must lower deterministically");
});
