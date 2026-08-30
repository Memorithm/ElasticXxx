//! Concurrency permits adapter: a real CPU-side width resource.
//!
//! Models a licensed execution width (how many workers may run
//! concurrently) as an explicit permit ledger. Changing the width is a real
//! adaptation with real constraints: the adapter refuses any width change
//! that would strand active holders, mirroring how a thread-pool resize must
//! wait for in-flight work — expressed here as a structured invariant
//! violation instead of OS-specific draining logic.
//!
//! The declaration admits `reinterpret@concurrency` and requires the matching
//! capability, so planner proposals flow through the same grounded-candidate
//! contract as every other resource.

use crate::error::AdapterError;
use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId, ResourceSpec,
};
use elastic_core::TransitionMechanism;
use elastic_eir::{lower, EirResource, PlanningContext, TransitionCandidate};

/// A licensed concurrency width with an active-holder ledger.
#[derive(Debug)]
pub struct ConcurrencyPermits {
    spec: ResourceSpec,
    ir: EirResource,
    max_width: usize,
    width: usize,
    active: usize,
}

impl ConcurrencyPermits {
    /// Construct a permit pool with an initial licensed width.
    ///
    /// `max_width` models the trusted discovery of how much parallelism this
    /// process may use.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::BlankIdentifier`] for blank ids and
    /// [`AdapterError::TargetOutOfBounds`] when `initial_width` is zero or
    /// above `max_width` (a width of at least one is required to make
    /// progress).
    pub fn new(id: &str, max_width: usize, initial_width: usize) -> Result<Self, AdapterError> {
        if initial_width == 0 || initial_width > max_width {
            return Err(AdapterError::TargetOutOfBounds {
                target: initial_width as u64,
                min: 1,
                max: max_width as u64,
            });
        }
        let spec = ResourceSpec::builder(
            ResourceClassId::SHARED,
            LogicalResourceId::new(id).map_err(|_| AdapterError::BlankIdentifier)?,
        )
        .allow(DimensionId::CONCURRENCY)
        .preserve(Invariant::new(InvariantKind::PreserveIdentity))
        .optimize(ObjectiveId::THROUGHPUT)
        .optimize(ObjectiveId::STABILITY)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CONCURRENCY,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CONCURRENCY,
        ))
        .build()
        .map_err(|_| AdapterError::BlankIdentifier)?;
        let document = lower(&spec).map_err(|_| AdapterError::BlankIdentifier)?;
        let ir = document
            .resource(id)
            .ok_or(AdapterError::BlankIdentifier)?
            .clone();
        Ok(Self {
            spec,
            ir,
            max_width,
            width: initial_width,
            active: 0,
        })
    }

    /// The validated declaration backing this adapter.
    #[must_use]
    pub const fn spec(&self) -> &ResourceSpec {
        &self.spec
    }

    /// The normalized IR node for this permit pool.
    #[must_use]
    pub const fn ir(&self) -> &EirResource {
        &self.ir
    }

    /// Maximum concurrency width allowed by the trusted configuration.
    #[must_use]
    pub const fn max_width(&self) -> usize {
        self.max_width
    }

    /// The currently licensed width.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// The number of active holders.
    #[must_use]
    pub const fn active(&self) -> usize {
        self.active
    }

    /// Acquire one permit.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::PermitOverflow`] when the licensed width is
    /// exhausted.
    pub fn acquire(&mut self) -> Result<(), AdapterError> {
        if self.active >= self.width {
            return Err(AdapterError::PermitOverflow {
                active: self.active + 1,
                width: self.width,
            });
        }
        self.active += 1;
        Ok(())
    }

    /// Release one permit.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::PermitOverflow`] when releasing an unacquired
    /// permit.
    pub fn release(&mut self) -> Result<(), AdapterError> {
        self.active = self
            .active
            .checked_sub(1)
            .ok_or(AdapterError::PermitOverflow {
                active: 0,
                width: self.width,
            })?;
        Ok(())
    }

    /// Current observations: `utilization` is `active / width`.
    #[must_use]
    pub fn observe(&self) -> PlanningContext {
        let utilization = if self.width == 0 {
            0.0
        } else {
            self.active as f64 / self.width as f64
        };
        PlanningContext::new().observe(ObservationSignalId::UTILIZATION, utilization)
    }

    /// Validate a width change without acting.
    ///
    /// # Errors
    ///
    /// Rejects widths that are zero, above the trusted maximum, or below the
    /// number of active holders (which would strand them).
    pub fn validate_resize(&self, target: usize) -> Result<usize, AdapterError> {
        if target == 0 || target > self.max_width {
            return Err(AdapterError::TargetOutOfBounds {
                target: target as u64,
                min: 1,
                max: self.max_width as u64,
            });
        }
        if target < self.active {
            return Err(AdapterError::WouldStrandHolders {
                requested_width: target,
                active: self.active,
            });
        }
        Ok(target)
    }

    /// Execute a width change after re-validation.
    ///
    /// # Errors
    ///
    /// Identical conditions to [`ConcurrencyPermits::validate_resize`].
    pub fn apply(&mut self, target: usize) -> Result<(usize, usize), AdapterError> {
        let target = self.validate_resize(target)?;
        let from = self.width;
        self.width = target;
        Ok((from, self.width))
    }

    /// Build an advisory candidate proposal (`reinterpret@concurrency`).
    #[must_use]
    pub fn candidate(&self, target_width: usize) -> Option<TransitionCandidate> {
        let admitted = self.ir.transitions().iter().find(|admitted| {
            admitted.transition().mechanism() == TransitionMechanism::Reinterpret
                && admitted.transition().dimension() == &DimensionId::CONCURRENCY
        })?;
        Some(TransitionCandidate::from_admitted(admitted).with_magnitude(target_width as u64))
    }
}
