#![no_main]

use elastic_runtime::DecisionTrace;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(trace) = DecisionTrace::from_bounded_json(data) else {
        return;
    };

    let encoded = trace
        .to_bounded_json()
        .expect("an accepted trace must re-encode within the public bounds");
    let decoded = DecisionTrace::from_bounded_json(encoded.as_bytes())
        .expect("a trace emitted by the encoder must decode again");
    assert_eq!(trace, decoded);
});
