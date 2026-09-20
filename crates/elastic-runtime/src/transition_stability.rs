//! ELANG7 anti-thrashing admission for elastic transitions.
//!
//! This layer is deliberately orthogonal to transition legality. It can block a
//! structurally legal candidate because the control state is unstable, too soon
//! after a previous commit, or over a configured transition-rate budget. It can
//! never make an otherwise illegal transition legal.

use crate::{ObservationSnapshot, ObservationSource};
use elastic_core::resource::{DimensionId, ObservationSignalId};
use elastic_core::TransitionMechanism;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use std::time::{Duration, Instant};

/// Durable/report schema of the first transition-stability gate.
pub const TRANSITION_STABILITY_SCHEMA_V1: u16 = 1;
/// Hard bound for retained commit timestamps in one rate-limit window.
pub const MAX_TRANSITION_RATE_LIMIT_COMMITS: usize = 1024;

/// Direction in which a hysteresis trigger becomes active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HysteresisDirectionV1 {
    /// Trigger when the signal rises to or above the trigger threshold and
    /// re-arm only after it falls to or below the release threshold.
    Rising,
    /// Trigger when the signal falls to or below the trigger threshold and
    /// re-arm only after it rises to or above the release threshold.
    Falling,
}

impl HysteresisDirectionV1 {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rising => "rising",
            Self::Falling => "falling",
        }
    }
}

/// Source-bound, freshness-bound hysteresis contract.
#[derive(Clone, Debug, PartialEq)]
pub struct HysteresisPolicyV1 {
    source: ObservationSource,
    signal: ObservationSignalId,
    direction: HysteresisDirectionV1,
    trigger_threshold: f64,
    release_threshold: f64,
    max_age: Duration,
}

impl HysteresisPolicyV1 {
    /// Construct one strict hysteresis band.
    ///
    /// Rising bands require `release < trigger`; falling bands require
    /// `release > trigger`. Equal thresholds are rejected because they provide
    /// no dead band and therefore no hysteresis.
    pub fn new(
        source: ObservationSource,
        signal: ObservationSignalId,
        direction: HysteresisDirectionV1,
        trigger_threshold: f64,
        release_threshold: f64,
        max_age: Duration,
    ) -> Result<Self, TransitionStabilityError> {
        if !trigger_threshold.is_finite() || !release_threshold.is_finite() {
            return Err(TransitionStabilityError::NonFiniteHysteresisThreshold);
        }
        if max_age.is_zero() {
            return Err(TransitionStabilityError::ZeroHysteresisFreshness);
        }
        let ordered = match direction {
            HysteresisDirectionV1::Rising => release_threshold < trigger_threshold,
            HysteresisDirectionV1::Falling => release_threshold > trigger_threshold,
        };
        if !ordered {
            return Err(TransitionStabilityError::InvalidHysteresisBand {
                direction,
                trigger_threshold,
                release_threshold,
            });
        }
        Ok(Self {
            source,
            signal,
            direction,
            trigger_threshold,
            release_threshold,
            max_age,
        })
    }

    #[must_use]
    pub const fn source(&self) -> &ObservationSource {
        &self.source
    }

    #[must_use]
    pub const fn signal(&self) -> &ObservationSignalId {
        &self.signal
    }

    #[must_use]
    pub const fn direction(&self) -> HysteresisDirectionV1 {
        self.direction
    }

    #[must_use]
    pub const fn trigger_threshold(&self) -> f64 {
        self.trigger_threshold
    }

    #[must_use]
    pub const fn release_threshold(&self) -> f64 {
        self.release_threshold
    }

    #[must_use]
    pub const fn max_age(&self) -> Duration {
        self.max_age
    }

    fn trigger_satisfied(&self, value: f64) -> bool {
        match self.direction {
            HysteresisDirectionV1::Rising => value >= self.trigger_threshold,
            HysteresisDirectionV1::Falling => value <= self.trigger_threshold,
        }
    }

    fn release_satisfied(&self, value: f64) -> bool {
        match self.direction {
            HysteresisDirectionV1::Rising => value <= self.release_threshold,
            HysteresisDirectionV1::Falling => value >= self.release_threshold,
        }
    }
}

/// Maximum committed transition count in one rolling monotonic-time window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionRateLimitV1 {
    maximum_commits: usize,
    window: Duration,
}

impl TransitionRateLimitV1 {
    pub fn new(maximum_commits: usize, window: Duration) -> Result<Self, TransitionStabilityError> {
        if maximum_commits == 0 || maximum_commits > MAX_TRANSITION_RATE_LIMIT_COMMITS {
            return Err(TransitionStabilityError::InvalidRateLimitCount {
                maximum_commits,
                maximum: MAX_TRANSITION_RATE_LIMIT_COMMITS,
            });
        }
        if window.is_zero() {
            return Err(TransitionStabilityError::ZeroRateLimitWindow);
        }
        Ok(Self {
            maximum_commits,
            window,
        })
    }

    #[must_use]
    pub const fn maximum_commits(self) -> usize {
        self.maximum_commits
    }

    #[must_use]
    pub const fn window(self) -> Duration {
        self.window
    }
}

/// Immutable anti-thrashing policy for one exact transition class.
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionStabilityPolicyV1 {
    mechanism: TransitionMechanism,
    dimension: DimensionId,
    hysteresis: Option<HysteresisPolicyV1>,
    cooldown: Option<Duration>,
    rate_limit: Option<TransitionRateLimitV1>,
}

impl TransitionStabilityPolicyV1 {
    pub fn new(
        mechanism: TransitionMechanism,
        dimension: DimensionId,
        hysteresis: Option<HysteresisPolicyV1>,
        cooldown: Option<Duration>,
        rate_limit: Option<TransitionRateLimitV1>,
    ) -> Result<Self, TransitionStabilityError> {
        if hysteresis.is_none() && cooldown.is_none() && rate_limit.is_none() {
            return Err(TransitionStabilityError::EmptyPolicy);
        }
        if cooldown.is_some_and(|duration| duration.is_zero()) {
            return Err(TransitionStabilityError::ZeroCooldown);
        }
        Ok(Self {
            mechanism,
            dimension,
            hysteresis,
            cooldown,
            rate_limit,
        })
    }

    #[must_use]
    pub const fn mechanism(&self) -> TransitionMechanism {
        self.mechanism
    }

    #[must_use]
    pub const fn dimension(&self) -> &DimensionId {
        &self.dimension
    }

    #[must_use]
    pub const fn hysteresis(&self) -> Option<&HysteresisPolicyV1> {
        self.hysteresis.as_ref()
    }

    #[must_use]
    pub const fn cooldown(&self) -> Option<Duration> {
        self.cooldown
    }

    #[must_use]
    pub const fn rate_limit(&self) -> Option<TransitionRateLimitV1> {
        self.rate_limit
    }
}

/// Fail-closed admission status. Only `Eligible` creates a permit.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TransitionStabilityStatusV1 {
    Eligible,
    InsufficientEvidence,
    HysteresisTriggerNotReached,
    HysteresisAwaitingRelease,
    CooldownActive,
    RateLimited,
    ClockRegression,
}

/// Auditable anti-thrashing decision. Absolute monotonic instants are not
/// serialized because they have no stable meaning across processes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TransitionStabilityReportV1 {
    pub schema_version: u16,
    pub status: TransitionStabilityStatusV1,
    pub reason: String,
    pub mechanism: String,
    pub dimension: String,
    pub hysteresis_armed: bool,
    pub observed_value: Option<f64>,
    pub cooldown_remaining_milliseconds: u64,
    pub commits_in_rate_window: usize,
    pub rate_limit_maximum_commits: Option<usize>,
    pub rate_limit_window_milliseconds: Option<u64>,
    pub generation: u64,
}

/// Single-use anti-thrashing admission token.
///
/// It is intentionally not `Clone`. Consuming it in `record_commit` makes the
/// control-state handoff explicit. It is not an actuation capability: callers
/// must still perform ordinary trusted validation before physical effect.
#[derive(Debug)]
pub struct TransitionStabilityPermitV1 {
    generation: u64,
    checked_at: Instant,
    evidence_observed_at: Option<Instant>,
    mechanism: TransitionMechanism,
    dimension: DimensionId,
}

impl TransitionStabilityPermitV1 {
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn mechanism(&self) -> TransitionMechanism {
        self.mechanism
    }

    #[must_use]
    pub const fn dimension(&self) -> &DimensionId {
        &self.dimension
    }
}

/// Stateful transition-stability admission gate.
#[derive(Debug)]
pub struct TransitionStabilityGateV1 {
    policy: TransitionStabilityPolicyV1,
    generation: u64,
    hysteresis_armed: bool,
    last_commit: Option<Instant>,
    commit_history: VecDeque<Instant>,
}

impl TransitionStabilityGateV1 {
    #[must_use]
    pub fn new(policy: TransitionStabilityPolicyV1) -> Self {
        Self {
            policy,
            generation: 0,
            hysteresis_armed: true,
            last_commit: None,
            commit_history: VecDeque::new(),
        }
    }

    #[must_use]
    pub const fn policy(&self) -> &TransitionStabilityPolicyV1 {
        &self.policy
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Evaluate current stability evidence and return a single-use permit only
    /// when every configured anti-thrashing condition is satisfied.
    pub fn check(
        &mut self,
        observations: &ObservationSnapshot,
        now: Instant,
    ) -> (
        TransitionStabilityReportV1,
        Option<TransitionStabilityPermitV1>,
    ) {
        let mut observed_value = None;
        let mut evidence_observed_at = None;
        if self.prune_rate_history(now).is_err() {
            return (
                self.report(
                    TransitionStabilityStatusV1::ClockRegression,
                    "monotonic time regressed behind retained commit history",
                    observed_value,
                    0,
                ),
                None,
            );
        }

        if let Some(hysteresis) = &self.policy.hysteresis {
            let matching = observations
                .iter()
                .filter(|observation| {
                    observation.signal() == hysteresis.signal()
                        && observation.source() == hysteresis.source()
                })
                .collect::<Vec<_>>();
            let Some(observation) = (matching.len() == 1).then(|| matching[0]) else {
                return (
                    self.report(
                        TransitionStabilityStatusV1::InsufficientEvidence,
                        "hysteresis observation missing or ambiguous for exact source/signal",
                        observed_value,
                        0,
                    ),
                    None,
                );
            };
            let Some(age) = now.checked_duration_since(*observation.timestamp()) else {
                return (
                    self.report(
                        TransitionStabilityStatusV1::ClockRegression,
                        "hysteresis observation timestamp is in the future",
                        observed_value,
                        0,
                    ),
                    None,
                );
            };
            if !observation.is_valid()
                || !observation.value().is_finite()
                || age > hysteresis.max_age()
            {
                return (
                    self.report(
                        TransitionStabilityStatusV1::InsufficientEvidence,
                        "hysteresis observation is unsupported, non-finite, or stale",
                        observed_value,
                        0,
                    ),
                    None,
                );
            }
            let value = observation.value();
            observed_value = Some(value);
            evidence_observed_at = Some(*observation.timestamp());
            if !self.hysteresis_armed {
                if hysteresis.release_satisfied(value) {
                    self.hysteresis_armed = true;
                } else {
                    return (self.report(TransitionStabilityStatusV1::HysteresisAwaitingRelease, "hysteresis has not crossed its release threshold since the previous commit", observed_value, 0), None);
                }
            }
            if !hysteresis.trigger_satisfied(value) {
                return (
                    self.report(
                        TransitionStabilityStatusV1::HysteresisTriggerNotReached,
                        "hysteresis trigger threshold is not reached",
                        observed_value,
                        0,
                    ),
                    None,
                );
            }
        }

        if let Some(cooldown) = self.policy.cooldown {
            if let Some(last_commit) = self.last_commit {
                let Some(elapsed) = now.checked_duration_since(last_commit) else {
                    return (
                        self.report(
                            TransitionStabilityStatusV1::ClockRegression,
                            "monotonic time regressed behind the last commit",
                            observed_value,
                            0,
                        ),
                        None,
                    );
                };
                if elapsed < cooldown {
                    let remaining = cooldown.saturating_sub(elapsed);
                    return (
                        self.report(
                            TransitionStabilityStatusV1::CooldownActive,
                            "minimum post-commit cooldown is still active",
                            observed_value,
                            duration_millis_u64(remaining),
                        ),
                        None,
                    );
                }
            }
        }

        if let Some(rate_limit) = self.policy.rate_limit {
            if self.commit_history.len() >= rate_limit.maximum_commits() {
                return (
                    self.report(
                        TransitionStabilityStatusV1::RateLimited,
                        "rolling transition-rate budget is exhausted",
                        observed_value,
                        0,
                    ),
                    None,
                );
            }
        }

        let report = self.report(
            TransitionStabilityStatusV1::Eligible,
            "all configured anti-thrashing conditions are satisfied",
            observed_value,
            0,
        );
        let permit = TransitionStabilityPermitV1 {
            generation: self.generation,
            checked_at: now,
            evidence_observed_at,
            mechanism: self.policy.mechanism,
            dimension: self.policy.dimension.clone(),
        };
        (report, Some(permit))
    }

    /// Record a transition only after the caller has actually committed it.
    ///
    /// A stale permit fails closed. Recording a commit advances the gate
    /// generation and disarms hysteresis until its release threshold is seen.
    pub fn record_commit(
        &mut self,
        permit: TransitionStabilityPermitV1,
        committed_at: Instant,
    ) -> Result<(), TransitionStabilityError> {
        if permit.generation != self.generation
            || permit.mechanism != self.policy.mechanism
            || permit.dimension != self.policy.dimension
        {
            return Err(TransitionStabilityError::StalePermit {
                permit_generation: permit.generation,
                current_generation: self.generation,
            });
        }
        if committed_at
            .checked_duration_since(permit.checked_at)
            .is_none()
            || self
                .last_commit
                .is_some_and(|last| committed_at.checked_duration_since(last).is_none())
        {
            return Err(TransitionStabilityError::ClockRegression);
        }
        if let Some(hysteresis) = &self.policy.hysteresis {
            let observed_at = permit
                .evidence_observed_at
                .ok_or(TransitionStabilityError::CommitOutsideAdmission)?;
            let age = committed_at
                .checked_duration_since(observed_at)
                .ok_or(TransitionStabilityError::ClockRegression)?;
            if age > hysteresis.max_age() {
                return Err(TransitionStabilityError::CommitOutsideAdmission);
            }
        }
        self.prune_rate_history(committed_at)?;
        if let Some(cooldown) = self.policy.cooldown {
            if let Some(last) = self.last_commit {
                let elapsed = committed_at
                    .checked_duration_since(last)
                    .ok_or(TransitionStabilityError::ClockRegression)?;
                if elapsed < cooldown {
                    return Err(TransitionStabilityError::CommitOutsideAdmission);
                }
            }
        }
        if let Some(rate_limit) = self.policy.rate_limit {
            if self.commit_history.len() >= rate_limit.maximum_commits() {
                return Err(TransitionStabilityError::CommitOutsideAdmission);
            }
        }
        self.last_commit = Some(committed_at);
        if self.policy.rate_limit.is_some() {
            self.commit_history.push_back(committed_at);
        }
        if self.policy.hysteresis.is_some() {
            self.hysteresis_armed = false;
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(TransitionStabilityError::GenerationOverflow)?;
        Ok(())
    }

    fn prune_rate_history(&mut self, now: Instant) -> Result<(), TransitionStabilityError> {
        let Some(rate_limit) = self.policy.rate_limit else {
            return Ok(());
        };
        while let Some(first) = self.commit_history.front().copied() {
            let age = now
                .checked_duration_since(first)
                .ok_or(TransitionStabilityError::ClockRegression)?;
            if age >= rate_limit.window() {
                self.commit_history.pop_front();
            } else {
                break;
            }
        }
        Ok(())
    }

    fn report(
        &self,
        status: TransitionStabilityStatusV1,
        reason: &str,
        observed_value: Option<f64>,
        cooldown_remaining_milliseconds: u64,
    ) -> TransitionStabilityReportV1 {
        TransitionStabilityReportV1 {
            schema_version: TRANSITION_STABILITY_SCHEMA_V1,
            status,
            reason: reason.to_owned(),
            mechanism: mechanism_name(self.policy.mechanism).to_owned(),
            dimension: self.policy.dimension.as_str().to_owned(),
            hysteresis_armed: self.hysteresis_armed,
            observed_value,
            cooldown_remaining_milliseconds,
            commits_in_rate_window: self.commit_history.len(),
            rate_limit_maximum_commits: self.policy.rate_limit.map(|rate| rate.maximum_commits()),
            rate_limit_window_milliseconds: self
                .policy
                .rate_limit
                .map(|rate| duration_millis_u64(rate.window())),
            generation: self.generation,
        }
    }
}

fn duration_millis_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

const fn mechanism_name(mechanism: TransitionMechanism) -> &'static str {
    match mechanism {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
    }
}

/// Construction or state-update failure for transition stability control.
#[derive(Clone, Debug, PartialEq)]
pub enum TransitionStabilityError {
    EmptyPolicy,
    ZeroCooldown,
    NonFiniteHysteresisThreshold,
    ZeroHysteresisFreshness,
    InvalidHysteresisBand {
        direction: HysteresisDirectionV1,
        trigger_threshold: f64,
        release_threshold: f64,
    },
    InvalidRateLimitCount {
        maximum_commits: usize,
        maximum: usize,
    },
    ZeroRateLimitWindow,
    StalePermit {
        permit_generation: u64,
        current_generation: u64,
    },
    CommitOutsideAdmission,
    ClockRegression,
    GenerationOverflow,
}

impl fmt::Display for TransitionStabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPolicy => f.write_str("transition stability policy must enable hysteresis, cooldown, or rate limiting"),
            Self::ZeroCooldown => f.write_str("transition stability cooldown must be non-zero"),
            Self::NonFiniteHysteresisThreshold => f.write_str("hysteresis thresholds must be finite"),
            Self::ZeroHysteresisFreshness => f.write_str("hysteresis observation freshness bound must be non-zero"),
            Self::InvalidHysteresisBand { direction, trigger_threshold, release_threshold } => write!(
                f,
                "invalid {} hysteresis band: trigger={trigger_threshold}, release={release_threshold}",
                direction.as_str()
            ),
            Self::InvalidRateLimitCount { maximum_commits, maximum } => write!(
                f,
                "transition rate limit count {maximum_commits} must be in 1..={maximum}"
            ),
            Self::ZeroRateLimitWindow => f.write_str("transition rate-limit window must be non-zero"),
            Self::StalePermit { permit_generation, current_generation } => write!(
                f,
                "transition stability permit generation {permit_generation} is stale; current generation is {current_generation}"
            ),
            Self::CommitOutsideAdmission => f.write_str("transition commit no longer satisfies cooldown/rate admission"),
            Self::ClockRegression => f.write_str("monotonic transition-stability time regressed"),
            Self::GenerationOverflow => f.write_str("transition-stability generation overflow"),
        }
    }
}

impl std::error::Error for TransitionStabilityError {}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::resource::ObservationSignalId;
    use std::time::Duration;

    fn snapshot(
        source: &ObservationSource,
        signal: ObservationSignalId,
        value: f64,
        at: Instant,
    ) -> ObservationSnapshot {
        ObservationSnapshot::new(
            at,
            vec![crate::Observation::from_source(
                source.clone(),
                signal,
                value,
                at,
            )],
        )
    }

    fn rising_policy() -> TransitionStabilityPolicyV1 {
        TransitionStabilityPolicyV1::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
            Some(
                HysteresisPolicyV1::new(
                    ObservationSource::runtime("anti-thrash-test"),
                    ObservationSignalId::UTILIZATION,
                    HysteresisDirectionV1::Rising,
                    0.80,
                    0.60,
                    Duration::from_secs(2),
                )
                .unwrap(),
            ),
            Some(Duration::from_secs(10)),
            Some(TransitionRateLimitV1::new(2, Duration::from_secs(60)).unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn rising_hysteresis_requires_release_after_commit() {
        let source = ObservationSource::runtime("anti-thrash-test");
        let start = Instant::now();
        let mut gate = TransitionStabilityGateV1::new(rising_policy());

        let (low, _) = gate.check(
            &snapshot(&source, ObservationSignalId::UTILIZATION, 0.79, start),
            start,
        );
        assert_eq!(
            low.status,
            TransitionStabilityStatusV1::HysteresisTriggerNotReached
        );

        let trigger_at = start + Duration::from_secs(1);
        let (eligible, permit) = gate.check(
            &snapshot(&source, ObservationSignalId::UTILIZATION, 0.90, trigger_at),
            trigger_at,
        );
        assert_eq!(eligible.status, TransitionStabilityStatusV1::Eligible);
        gate.record_commit(permit.unwrap(), trigger_at).unwrap();

        let later = start + Duration::from_secs(12);
        let (blocked, _) = gate.check(
            &snapshot(&source, ObservationSignalId::UTILIZATION, 0.90, later),
            later,
        );
        assert_eq!(
            blocked.status,
            TransitionStabilityStatusV1::HysteresisAwaitingRelease
        );

        let release_at = start + Duration::from_secs(13);
        let (release, _) = gate.check(
            &snapshot(&source, ObservationSignalId::UTILIZATION, 0.55, release_at),
            release_at,
        );
        assert_eq!(
            release.status,
            TransitionStabilityStatusV1::HysteresisTriggerNotReached
        );
        assert!(release.hysteresis_armed);
    }

    #[test]
    fn cooldown_and_rate_limit_are_independent_of_hysteresis() {
        let policy = TransitionStabilityPolicyV1::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
            None,
            Some(Duration::from_secs(10)),
            Some(TransitionRateLimitV1::new(2, Duration::from_secs(60)).unwrap()),
        )
        .unwrap();
        let mut gate = TransitionStabilityGateV1::new(policy);
        let empty = ObservationSnapshot::new(Instant::now(), vec![]);
        let start = empty.timestamp;

        let (_, first) = gate.check(&empty, start);
        gate.record_commit(first.unwrap(), start).unwrap();
        let (cooldown, _) = gate.check(&empty, start + Duration::from_secs(5));
        assert_eq!(cooldown.status, TransitionStabilityStatusV1::CooldownActive);

        let second_at = start + Duration::from_secs(11);
        let (_, second) = gate.check(&empty, second_at);
        gate.record_commit(second.unwrap(), second_at).unwrap();
        let (limited, _) = gate.check(&empty, start + Duration::from_secs(22));
        assert_eq!(limited.status, TransitionStabilityStatusV1::RateLimited);

        let after_window = start + Duration::from_secs(61);
        let (eligible, permit) = gate.check(&empty, after_window);
        assert_eq!(eligible.status, TransitionStabilityStatusV1::Eligible);
        assert!(permit.is_some());
    }

    #[test]
    fn wrong_source_stale_or_unsupported_hysteresis_evidence_fails_closed() {
        let expected = ObservationSource::runtime("anti-thrash-test");
        let now = Instant::now();
        let mut gate = TransitionStabilityGateV1::new(rising_policy());

        let wrong = ObservationSnapshot::new(
            now,
            vec![crate::Observation::from_source(
                ObservationSource::runtime("wrong"),
                ObservationSignalId::UTILIZATION,
                0.9,
                now,
            )],
        );
        assert_eq!(
            gate.check(&wrong, now).0.status,
            TransitionStabilityStatusV1::InsufficientEvidence
        );

        let old = now.checked_sub(Duration::from_secs(3)).unwrap();
        assert_eq!(
            gate.check(
                &snapshot(&expected, ObservationSignalId::UTILIZATION, 0.9, old),
                now,
            )
            .0
            .status,
            TransitionStabilityStatusV1::InsufficientEvidence
        );

        let unsupported = ObservationSnapshot::new(
            now,
            vec![crate::Observation::unsupported_from_source(
                expected,
                ObservationSignalId::UTILIZATION,
                now,
                "not available",
            )],
        );
        assert_eq!(
            gate.check(&unsupported, now).0.status,
            TransitionStabilityStatusV1::InsufficientEvidence
        );
    }

    #[test]
    fn hysteresis_permit_expires_with_its_source_observation() {
        let source = ObservationSource::runtime("anti-thrash-test");
        let start = Instant::now();
        let mut gate = TransitionStabilityGateV1::new(rising_policy());
        let (_, permit) = gate.check(
            &snapshot(&source, ObservationSignalId::UTILIZATION, 0.9, start),
            start,
        );
        assert!(matches!(
            gate.record_commit(permit.unwrap(), start + Duration::from_secs(3)),
            Err(TransitionStabilityError::CommitOutsideAdmission)
        ));
        assert_eq!(gate.generation(), 0);
    }

    #[test]
    fn stale_permit_cannot_be_reused_after_another_commit() {
        let policy = TransitionStabilityPolicyV1::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
            None,
            None,
            Some(TransitionRateLimitV1::new(4, Duration::from_secs(60)).unwrap()),
        )
        .unwrap();
        let mut gate = TransitionStabilityGateV1::new(policy);
        let now = Instant::now();
        let empty = ObservationSnapshot::new(now, vec![]);
        let (_, first) = gate.check(&empty, now);
        let (_, stale) = gate.check(&empty, now);
        gate.record_commit(first.unwrap(), now).unwrap();
        assert!(matches!(
            gate.record_commit(stale.unwrap(), now),
            Err(TransitionStabilityError::StalePermit { .. })
        ));
    }

    #[test]
    fn falling_hysteresis_uses_inverse_trigger_and_release_order() {
        let source = ObservationSource::runtime("falling");
        let policy = TransitionStabilityPolicyV1::new(
            TransitionMechanism::Reinterpret,
            DimensionId::ENERGY,
            Some(
                HysteresisPolicyV1::new(
                    source.clone(),
                    ObservationSignalId::THERMAL_MARGIN,
                    HysteresisDirectionV1::Falling,
                    5.0,
                    8.0,
                    Duration::from_secs(1),
                )
                .unwrap(),
            ),
            None,
            None,
        )
        .unwrap();
        let mut gate = TransitionStabilityGateV1::new(policy);
        let now = Instant::now();
        let (eligible, permit) = gate.check(
            &snapshot(&source, ObservationSignalId::THERMAL_MARGIN, 4.0, now),
            now,
        );
        assert_eq!(eligible.status, TransitionStabilityStatusV1::Eligible);
        gate.record_commit(permit.unwrap(), now).unwrap();
        let later = now + Duration::from_millis(100);
        let (waiting, _) = gate.check(
            &snapshot(&source, ObservationSignalId::THERMAL_MARGIN, 6.0, later),
            later,
        );
        assert_eq!(
            waiting.status,
            TransitionStabilityStatusV1::HysteresisAwaitingRelease
        );
        let release = now + Duration::from_millis(200);
        let (rearmed, _) = gate.check(
            &snapshot(&source, ObservationSignalId::THERMAL_MARGIN, 9.0, release),
            release,
        );
        assert_eq!(
            rearmed.status,
            TransitionStabilityStatusV1::HysteresisTriggerNotReached
        );
    }

    #[test]
    fn invalid_or_empty_policies_fail_closed() {
        assert!(matches!(
            TransitionStabilityPolicyV1::new(
                TransitionMechanism::Reinterpret,
                DimensionId::CAPACITY,
                None,
                None,
                None,
            ),
            Err(TransitionStabilityError::EmptyPolicy)
        ));
        assert!(HysteresisPolicyV1::new(
            ObservationSource::runtime("x"),
            ObservationSignalId::UTILIZATION,
            HysteresisDirectionV1::Rising,
            0.5,
            0.5,
            Duration::from_secs(1),
        )
        .is_err());
        assert!(TransitionRateLimitV1::new(0, Duration::from_secs(1)).is_err());
    }
}
