#![no_main]

use elastic_runtime::{
    ModelExecutionControllerContractsV1, MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_BYTES,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(contracts) = ModelExecutionControllerContractsV1::from_bounded_json(data) else {
        return;
    };

    let encoded = contracts
        .to_bounded_json()
        .expect("accepted model-execution contracts must re-encode within public bounds");
    assert!(encoded.len() <= MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_BYTES);

    let decoded = ModelExecutionControllerContractsV1::from_bounded_json(encoded.as_bytes())
        .expect("accepted model-execution contracts must survive bounded round-trip");
    assert_eq!(contracts, decoded);
});
