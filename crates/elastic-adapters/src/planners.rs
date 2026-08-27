//! Real planning strategies for capacity resources.
//!
//! These are honest control laws — deterministic, explainable, and
//! conservative — not optimizers. Both implement the EIR
//! [`TransitionPlanner`] contract over a [`PlanningContext`]:
//!
//! - [`ThresholdPlanner`]: reactive relative band control (grow above the
//!   high watermark, shrink below the low one, resize by a fraction of the
//!   current commitment);
//! - [`HeadroomPlanner`]: absolute headroom regulation with a deadband.
//!
//! Shared contract with the core trait:
//!
//! 1. candidates only restate admissions of the resource under planning,
//!    verified via [`TransitionCandidate::is_declared_in`];
//! 2. missing evidence yields [`PlanOutcome::InsufficientEvidence`], an
//!    inadmissible request yields [`PlanOutcome::Unsupported`], and a
//!    satisfied resource yields [`PlanOutcome::NoCandidate`];
//! 3. identical inputs produce identical outputs (pure functions — no
//!    interior state, no clocks, no randomness).
//!
//! Magnitudes are advisory: adapters re-validate every proposal against
//! bounds, step limits, and invariants at action time.

use crate::ram::{committed_bytes_signal, host_total_bytes_signal};
use elastic_core::resource::{DimensionId, ObservationSignalId};
use elastic_core::TransitionMechanism;
use elastic_eir::{
    AdmittedTransition, EirResource, PlanOutcome, PlanningContext, TransitionCandidate,
    TransitionPlanner,
};

/// Shared lookup: the admitted capacity transition of the resource under
/// planning, or an honest outcome explaining why this strategy cannot apply.
fn capacity_admission(resource: &EirResource) -> Result<AdmittedTransition, PlanOutcome> {
    let Some(admitted) = resource.transitions().iter().find(|admitted| {
        admitted.transition().mechanism() == TransitionMechanism::Reinterpret
            && admitted.transition().dimension() == &DimensionId::CAPACITY
    }) else {
        // A different dimension vocabulary may still be plannable by other
        // strategies; this controller simply does not apply.
        return Err(PlanOutcome::Unsupported);
    };
    if !admitted.capability_grounded() {
        return Err(PlanOutcome::InsufficientEvidence {
            detail: "capacity transition lacks a required capability".to_owned(),
        });
    }
    Ok(admitted.clone())
}

/// Reactive threshold controller (relative band + fractional step).
///
/// Decision table given `utilization = committed / host_total`:
///
/// 1. `utilization >= high_watermark` → grow to
///    `committed * (1 + step_fraction)`;
/// 2. `utilization <= low_watermark` → shrink to
///    `committed * (1 - step_fraction)`;
/// 3. inside the band → [`PlanOutcome::NoCandidate`] (stability is a valid,
///    explicit answer).
///
/// Targets are rounded up to whole units with a floor of one; the adapter's
/// bounds and step limits still bound what can actually happen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThresholdPlanner {
    high_watermark: f64,
    low_watermark: f64,
    step_fraction: f64,
}

impl ThresholdPlanner {
    /// Construct a threshold controller.
    ///
    /// # Errors
    ///
    /// Returns [`PlannerConfigError`] unless
    /// `0.0 <= low <= high <= 1.0` and `0.0 < step_fraction <= 1.0`.
    pub fn new(
        low_watermark: f64,
        high_watermark: f64,
        step_fraction: f64,
    ) -> Result<Self, PlannerConfigError> {
        if !(low_watermark.is_finite())
            || !(high_watermark.is_finite())
            || !(step_fraction.is_finite())
            || low_watermark < 0.0
            || low_watermark > high_watermark
            || high_watermark > 1.0
            || step_fraction <= 0.0
            || step_fraction > 1.0
        {
            return Err(PlannerConfigError::InvalidWatermarks {
                low_watermark,
                high_watermark,
                step_fraction,
            });
        }
        Ok(Self {
            high_watermark,
            low_watermark,
            step_fraction,
        })
    }

    fn target_from(&self, committed: u64, grow: bool) -> u64 {
        let fraction = if grow {
            1.0 + self.step_fraction
        } else {
            1.0 - self.step_fraction
        };
        let target = (committed as f64 * fraction).ceil();
        if target < 1.0 {
            1
        } else {
            target as u64
        }
    }
}

/// Configuration failures for strategy constructors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlannerConfigError {
    /// Watermarks must satisfy `0 <= low <= high <= 1`; the step fraction
    /// must lie in `(0, 1]`.
    InvalidWatermarks {
        low_watermark: f64,
        high_watermark: f64,
        step_fraction: f64,
    },
    /// The headroom target or deadband was negative or non-finite.
    InvalidHeadroom {
        headroom_fraction: f64,
        deadband_fraction: f64,
    },
}

impl std::fmt::Display for PlannerConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWatermarks { low_watermark, high_watermark, step_fraction } => write!(
                f,
                "invalid thresholds: low {low_watermark} high {high_watermark} step {step_fraction}; \
                 require 0 <= low <= high <= 1 and 0 < step <= 1"
            ),
            Self::InvalidHeadroom { headroom_fraction, deadband_fraction } => write!(
                f,
                "invalid headroom config: target {headroom_fraction} deadband {deadband_fraction}; \
                 require finite values with 0 <= deadband <= target <= 1"
            ),
        }
    }
}

impl std::error::Error for PlannerConfigError {}

impl TransitionPlanner for ThresholdPlanner {
    fn propose_transition(&self, _resource: &EirResource) -> PlanOutcome {
        PlanOutcome::InsufficientEvidence {
            detail: "threshold controller requires observation context; call \
                     propose_transition_with_context"
                .to_owned(),
        }
    }

    fn propose_transition_with_context(
        &self,
        resource: &EirResource,
        context: &PlanningContext,
    ) -> PlanOutcome {
        let Some(utilization) = context.get(ObservationSignalId::UTILIZATION) else {
            return PlanOutcome::InsufficientEvidence {
                detail: "missing utilization observation".to_owned(),
            };
        };
        let admitted = match capacity_admission(resource) {
            Ok(admitted) => admitted,
            Err(outcome) => return outcome,
        };

        let Some(magnitude) = context
            .get(committed_bytes_signal())
            .map(|value| value as u64)
        else {
            return PlanOutcome::InsufficientEvidence {
                detail: "missing committed-bytes observation".to_owned(),
            };
        };

        if utilization >= self.high_watermark {
            let candidate = TransitionCandidate::from_admitted(&admitted)
                .with_magnitude(self.target_from(magnitude, true));
            return finish(candidate, resource);
        }
        if utilization <= self.low_watermark && magnitude > 1 {
            let candidate = TransitionCandidate::from_admitted(&admitted)
                .with_magnitude(self.target_from(magnitude, false));
            return finish(candidate, resource);
        }
        PlanOutcome::NoCandidate
    }
}

/// Absolute-headroom regulator: keep free capacity near a fixed fraction of
/// the host total, ignoring moves smaller than the deadband.
///
/// Where [`ThresholdPlanner`] reacts to utilization bands with relative
/// steps, this strategy regulates toward an absolute target
/// `committed_target = host_total * (1 - headroom)`; proposals are suppressed
/// while `|committed - target| <= host_total * deadband`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadroomPlanner {
    headroom_fraction: f64,
    deadband_fraction: f64,
}

impl HeadroomPlanner {
    /// Construct the regulator.
    ///
    /// # Errors
    ///
    /// Returns [`PlannerConfigError::InvalidHeadroom`] unless both values are
    /// finite and `0.0 <= deadband_fraction <= headroom_fraction <= 1.0`.
    pub fn new(headroom_fraction: f64, deadband_fraction: f64) -> Result<Self, PlannerConfigError> {
        if !headroom_fraction.is_finite()
            || !deadband_fraction.is_finite()
            || deadband_fraction < 0.0
            || deadband_fraction > headroom_fraction
            || headroom_fraction > 1.0
        {
            return Err(PlannerConfigError::InvalidHeadroom {
                headroom_fraction,
                deadband_fraction,
            });
        }
        Ok(Self {
            headroom_fraction,
            deadband_fraction,
        })
    }
}

impl TransitionPlanner for HeadroomPlanner {
    fn propose_transition(&self, _resource: &EirResource) -> PlanOutcome {
        PlanOutcome::InsufficientEvidence {
            detail: "headroom regulator requires observation context; call \
                     propose_transition_with_context"
                .to_owned(),
        }
    }

    fn propose_transition_with_context(
        &self,
        resource: &EirResource,
        context: &PlanningContext,
    ) -> PlanOutcome {
        let Some(free) = context.get(ObservationSignalId::FREE_CAPACITY) else {
            return PlanOutcome::InsufficientEvidence {
                detail: "missing free-capacity observation".to_owned(),
            };
        };
        let Some(committed) = context
            .get(committed_bytes_signal())
            .map(|value| value as u64)
        else {
            return PlanOutcome::InsufficientEvidence {
                detail: "missing committed-bytes observation".to_owned(),
            };
        };
        let Some(host_total) = context.get(host_total_bytes_signal()) else {
            return PlanOutcome::InsufficientEvidence {
                detail: "missing host-total-bytes observation".to_owned(),
            };
        };

        let admitted = match capacity_admission(resource) {
            Ok(admitted) => admitted,
            Err(outcome) => return outcome,
        };

        let target_free = host_total * self.headroom_fraction;
        let delta = free - target_free;
        if delta.abs() <= host_total * self.deadband_fraction {
            return PlanOutcome::NoCandidate;
        }

        // Moving by the signed gap steers free space onto the headroom line:
        // positive gaps grow the commitment, negative ones shrink it.
        let raw_target = committed as f64 + delta;
        let target = raw_target.ceil().max(1.0) as u64;
        let candidate = TransitionCandidate::from_admitted(&admitted).with_magnitude(target);
        finish(candidate, resource)
    }
}

/// Verify declaredness before publishing the candidate.
fn finish(candidate: TransitionCandidate, resource: &EirResource) -> PlanOutcome {
    if candidate.is_declared_in(resource) {
        PlanOutcome::Candidate(candidate)
    } else {
        PlanOutcome::NoCandidate
    }
}
