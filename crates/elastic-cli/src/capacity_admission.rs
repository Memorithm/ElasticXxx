//! Strict process boundary over the public capacity admission controller.

use elastic::{CapacityAdmissionControllerV1, CapacityAdmissionRequestV1};
use std::error::Error;
use std::io::{self, Read};

pub fn run() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    io::stdin().take(16_385).read_to_end(&mut input)?;
    if input.len() > 16_384 {
        return Err("capacity admission input exceeds 16 KiB".into());
    }
    let request: CapacityAdmissionRequestV1 = serde_json::from_slice(&input)?;
    request.validate()?;
    let mut controller = CapacityAdmissionControllerV1::new(
        "capacity-admission",
        request.max_concurrency,
        request.max_concurrency,
    )?;
    let report = controller.admit(request)?;
    println!("{}", serde_json::to_string(&report)?);
    if report.committed == Some(true) {
        Ok(())
    } else {
        Err(report.reason.into())
    }
}
