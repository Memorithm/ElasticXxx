#![no_main]

use elastic_runtime::ConstrainedDecisionTrace;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(trace) = ConstrainedDecisionTrace::from_bounded_json(data) else {
        return;
    };

    let encoded = trace
        .to_bounded_json()
        .expect("accepted constrained trace must re-encode within public bounds");
    let decoded = ConstrainedDecisionTrace::from_bounded_json(encoded.as_bytes())
        .expect("accepted constrained trace must survive bounded round-trip");
    assert_eq!(trace, decoded);
});
