#![no_main]

use elastic_kernel::{BooleanKernelDecisionTraceV1, MAX_BOOLEAN_KERNEL_DECISION_TRACE_BYTES};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(trace) = BooleanKernelDecisionTraceV1::from_bounded_json(data) else {
        return;
    };

    let encoded = trace
        .to_bounded_json()
        .expect("accepted kernel trace must re-encode within public bounds");
    assert!(encoded.len() <= MAX_BOOLEAN_KERNEL_DECISION_TRACE_BYTES);
    let decoded = BooleanKernelDecisionTraceV1::from_bounded_json(encoded.as_bytes())
        .expect("accepted kernel trace must survive bounded round-trip");
    assert_eq!(trace, decoded);
});
