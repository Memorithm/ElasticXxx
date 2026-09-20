#![no_main]

use elastic_runtime::{
    BooleanBatchDeviceDecisionTraceV1, MAX_BOOLEAN_BATCH_DEVICE_TRACE_BYTES,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(trace) = BooleanBatchDeviceDecisionTraceV1::from_bounded_json(data) else {
        return;
    };

    let encoded = trace
        .to_bounded_json()
        .expect("accepted batch/device trace must re-encode within public bounds");
    assert!(encoded.len() <= MAX_BOOLEAN_BATCH_DEVICE_TRACE_BYTES);
    let decoded = BooleanBatchDeviceDecisionTraceV1::from_bounded_json(encoded.as_bytes())
        .expect("accepted batch/device trace must survive bounded round-trip");
    assert_eq!(trace, decoded);
});
