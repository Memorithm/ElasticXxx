//! Example — closed-loop adaptation with a real planner strategy.
//!
//! A RAM budget under scripted synthetic pressure; the [`ThresholdPlanner`]
//! observes, proposes grounded candidates, and the adapter applies them —
//! refusing anything that would violate bounds, the step limit, or
//! `PreserveContents`. Demand is served through explicit bounded growth
//! steps.
//!
//! No OS probing, no randomness: the scenario is deterministic. Note how the
//! controller reacts to the *commitment* level (what was actually reserved),
//! which is the honest pressure signal for this adapter.

use elastic_adapters::{RamBudget, ThresholdPlanner};
use elastic_core::resource::ObservationSignalId;
use elastic_eir::TransitionPlanner;

fn mib(bytes: u64) -> String {
    format!("{:>7.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const MAX_STEP: u64 = 16 << 20;

    let mut budget = RamBudget::new(
        "inference-cache",
        256 << 20,
        1 << 20,
        128 << 20,
        16 << 20,
        Some(MAX_STEP),
    )?;
    let planner = ThresholdPlanner::new(0.25, 0.70, 0.5)?;
    let ir = budget.ir().clone();

    // Scripted demand curve: heavy pressure up front, then tapering.
    let demand: [u64; 6] = [200 << 20, 150 << 20, 120 << 20, 48 << 20, 12 << 20, 2 << 20];

    println!(
        "{:<8} {:>11} {:>11} {:>7}  decision",
        "phase", "demand", "committed", "util"
    );

    for (round, &needed) in demand.iter().enumerate() {
        let (_, max_bound) = budget.bounds();
        let served = needed.min(max_bound);

        // 1. PLAN first: react to the current committed level.
        let context = budget.observe();
        let utilization = context
            .get(ObservationSignalId::UTILIZATION)
            .unwrap_or(f64::NAN);
        let decision = match planner.propose_transition_with_context(&ir, &context) {
            elastic_eir::PlanOutcome::Candidate(candidate) => {
                match budget.apply(candidate.magnitude().unwrap_or_default()) {
                    Ok((from, to)) => format!("resize {from} -> {to}"),
                    Err(refusal) => format!("REFUSED ({refusal})"),
                }
            }
            other => other.to_string(),
        };

        println!(
            "r{:<7} {:>11} {:>11} {:>7.3}  {}",
            round,
            mib(served),
            mib(budget.committed()),
            utilization,
            decision
        );

        // 2. SERVE demand afterwards: release or grow in bounded steps,
        //    recording protected use.
        let current_use = budget.in_use();
        if current_use > served {
            budget.release_use(current_use - served)?;
        } else {
            // Growth respects the same step limit configured above.
            while budget.committed() < served {
                let remaining = served - budget.committed();
                let step_to = budget.committed() + MAX_STEP.min(remaining);
                budget.apply(step_to)?;
            }
            budget.record_use(served - current_use)?;
        }
    }

    println!(
        "\ninvariant check: in_use={} survived every transition",
        budget.in_use()
    );
    Ok(())
}
