//! Structured adapter errors.
//!
//! Adapters are the last line of defense: these errors are raised at action
//! time when a proposal would violate bounds, step limits, or declared
//! invariants. Planners receive them; they cannot bypass them.

use std::fmt;

/// Failures raised by resource adapters immediately before acting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterError {
    /// A resource identifier was blank.
    BlankIdentifier,
    /// Protected usage would exceed the current commitment.
    UsageOverflow {
        /// Requested total usage.
        requested_total: u64,
        /// Currently committed bytes.
        committed: u64,
    },
    /// A budget bound was zero or otherwise degenerate.
    InvalidBounds {
        /// The rejected lower bound.
        min: u64,
        /// The rejected upper bound.
        max: u64,
    },
    /// The initial commitment was outside the declared bounds.
    InitialOutOfBounds {
        /// The rejected initial value.
        initial: u64,
        /// The declared lower bound.
        min: u64,
        /// The declared upper bound.
        max: u64,
    },
    /// A resize target was outside the declared bounds.
    TargetOutOfBounds {
        /// The requested target.
        target: u64,
        /// The declared lower bound.
        min: u64,
        /// The declared upper bound.
        max: u64,
    },
    /// A single resize exceeded the configured maximum step.
    StepLimitExceeded {
        /// Current value.
        from: u64,
        /// Requested target.
        to: u64,
        /// Configured maximum delta per action.
        max_step: u64,
    },
    /// Shrinking would destroy in-use contents, violating the
    /// `PreserveContents` invariant enforced by this adapter.
    WouldViolateContents {
        /// Requested target.
        target: u64,
        /// Bytes currently in use that must survive every transition.
        in_use: u64,
    },
    /// A concurrency width change would strand active holders.
    WouldStrandHolders {
        /// Requested new width.
        requested_width: usize,
        /// Holders currently active.
        active: usize,
    },
    /// More permits were acquired than the licensed width allows.
    PermitOverflow {
        /// Active holders including the failed acquisition.
        active: usize,
        /// Licensed width.
        width: usize,
    },
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlankIdentifier => write!(f, "resource identifier must not be blank"),
            Self::UsageOverflow {
                requested_total,
                committed,
            } => write!(
                f,
                "protected usage {requested_total} exceeds the commitment of {committed}"
            ),
            Self::InvalidBounds { min, max } => {
                write!(
                    f,
                    "invalid budget bounds: min {min} must be >= 1 and <= max {max}"
                )
            }
            Self::InitialOutOfBounds { initial, min, max } => write!(
                f,
                "initial commitment {initial} is outside declared bounds [{min}, {max}]"
            ),
            Self::TargetOutOfBounds { target, min, max } => write!(
                f,
                "resize target {target} is outside declared bounds [{min}, {max}]"
            ),
            Self::StepLimitExceeded { from, to, max_step } => write!(
                f,
                "resize {from} -> {to} exceeds the maximum step of {max_step}"
            ),
            Self::WouldViolateContents { target, in_use } => write!(
                f,
                "shrinking to {target} would destroy {in_use} in-use bytes; \
                 PreserveContents forbids losing data"
            ),
            Self::WouldStrandHolders {
                requested_width,
                active,
            } => write!(
                f,
                "new width {requested_width} would strand {active} active holders"
            ),
            Self::PermitOverflow { active, width } => {
                write!(f, "{active} holders exceed the licensed width of {width}")
            }
        }
    }
}

impl std::error::Error for AdapterError {}
