//! RAM budget adapter: a real, in-process memory capacity resource.
//!
//! The budget materializes as an actual zeroed allocation
//! ([`RamBudget::apply`] really reserves and releases memory), so growth and
//! shrink are physical actions with observable cost — while staying portable,
//! safe (`#![forbid(unsafe_code)]`), and free of OS discovery. The hosting
//! limit is operator-supplied configuration, not a probe of the machine.
//!
//! Invariant enforcement at action time: the budget tracks bytes handed to
//! the application ([`RamBudget::record_use`]/[`RamBudget::release_use`]);
//! any resize that would drop committed space below in-use bytes is refused
//! because the declaration preserves contents. Planners cannot override the
//! refusal — validation runs immediately before the effect.

use crate::error::AdapterError;
use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId, ResourceSpec,
};
use elastic_core::TransitionMechanism;
use elastic_eir::{lower, EirResource, PlanningContext, TransitionCandidate};

/// A validated RAM-budget declaration plus its live materialization.
///
/// Effects are real: [`RamBudget::apply`] resizes an actual allocation.
#[derive(Debug)]
pub struct RamBudget {
    spec: ResourceSpec,
    ir: EirResource,
    bounds: (u64, u64),
    max_step: Option<u64>,
    host_total: u64,
    buffer: Vec<u8>,
    in_use: u64,
}

impl RamBudget {
    /// Construct a budget from explicit operator configuration.
    ///
    /// `host_total` models the trusted discovery result for how much memory
    /// this process may claim; bounds and the initial commitment are
    /// validated against it before any allocation happens.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::BlankIdentifier`],
    /// [`AdapterError::InvalidBounds`], or
    /// [`AdapterError::InitialOutOfBounds`] for degenerate configuration.
    pub fn new(
        id: &str,
        host_total: u64,
        min: u64,
        max: u64,
        initial: u64,
        max_step: Option<u64>,
    ) -> Result<Self, AdapterError> {
        if min == 0 || min > max || max > host_total {
            return Err(AdapterError::InvalidBounds { min, max });
        }
        if initial < min || initial > max {
            return Err(AdapterError::InitialOutOfBounds { initial, min, max });
        }

        let spec = ResourceSpec::builder(
            ResourceClassId::CAPACITY_RESOURCE,
            LogicalResourceId::new(id).map_err(|_| AdapterError::BlankIdentifier)?,
        )
        .allow(DimensionId::CAPACITY)
        .preserve(Invariant::new(InvariantKind::PreserveContents))
        .optimize(ObjectiveId::MEMORY_FOOTPRINT)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reinterpret,
            DimensionId::CAPACITY,
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
            bounds: (min, max),
            max_step,
            host_total,
            buffer: vec![0_u8; usize::try_from(initial).unwrap_or(usize::MAX)],
            in_use: 0,
        })
    }

    /// The validated declaration backing this adapter.
    #[must_use]
    pub const fn spec(&self) -> &ResourceSpec {
        &self.spec
    }

    /// The normalized IR node for this budget.
    #[must_use]
    pub const fn ir(&self) -> &EirResource {
        &self.ir
    }

    /// Declared `(min, max)` bounds.
    #[must_use]
    pub const fn bounds(&self) -> (u64, u64) {
        self.bounds
    }

    /// Currently committed bytes (the real allocation size).
    #[must_use]
    pub fn committed(&self) -> u64 {
        self.buffer.len() as u64
    }

    /// Bytes handed to the application and protected by `PreserveContents`.
    #[must_use]
    pub const fn in_use(&self) -> u64 {
        self.in_use
    }

    /// Hand `bytes` to the application (grows protected usage).
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::UsageOverflow`] when protected usage would
    /// exceed the current commitment.
    pub fn record_use(&mut self, bytes: u64) -> Result<(), AdapterError> {
        let new_use = self.in_use.saturating_add(bytes);
        if new_use > self.committed() {
            return Err(AdapterError::UsageOverflow {
                requested_total: new_use,
                committed: self.committed(),
            });
        }
        self.in_use = new_use;
        Ok(())
    }

    /// Return `bytes` from the application to the adapter.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::WouldViolateContents`] when releasing more
    /// than was recorded.
    pub fn release_use(&mut self, bytes: u64) -> Result<(), AdapterError> {
        if bytes > self.in_use {
            return Err(AdapterError::WouldViolateContents {
                target: self.in_use.saturating_sub(bytes),
                in_use: self.in_use,
            });
        }
        self.in_use -= bytes;
        Ok(())
    }

    /// Current observations for planners.
    ///
    /// - `free-capacity`: host total minus committed bytes;
    /// - `utilization`: committed divided by host total, in `0.0..=1.0`.
    #[must_use]
    pub fn observe(&self) -> PlanningContext {
        let committed = self.committed();
        let free = self.host_total.saturating_sub(committed);
        let utilization = if self.host_total == 0 {
            0.0
        } else {
            committed as f64 / self.host_total as f64
        };
        PlanningContext::new()
            .observe(ObservationSignalId::FREE_CAPACITY, free as f64)
            .observe(ObservationSignalId::UTILIZATION, utilization)
    }

    /// Validate a resize proposal without acting.
    ///
    /// Checks bounds, the content-preservation invariant, and the step limit
    /// — exactly what [`RamBudget::apply`] will enforce, so planners can
    /// pre-check honestly.
    ///
    /// # Errors
    ///
    /// See [`AdapterError`]: out-of-bounds targets, destruction of in-use
    /// contents, and oversized steps are rejected here.
    pub fn validate_resize(&self, target: u64) -> Result<u64, AdapterError> {
        let (min, max) = self.bounds;
        if target < min || target > max || target > self.host_total {
            return Err(AdapterError::TargetOutOfBounds { target, min, max });
        }
        if target < self.in_use {
            return Err(AdapterError::WouldViolateContents {
                target,
                in_use: self.in_use,
            });
        }
        if let Some(max_step) = self.max_step {
            let from = self.committed();
            if target.abs_diff(from) > max_step {
                return Err(AdapterError::StepLimitExceeded {
                    from,
                    to: target,
                    max_step,
                });
            }
        }
        Ok(target)
    }

    /// Execute a resize after full re-validation.
    ///
    /// This is the physical action: the underlying allocation really grows or
    /// shrinks. Growing preserves contents (new space is zeroed); shrinking
    /// below recorded usage is impossible.
    ///
    /// # Errors
    ///
    /// Identical conditions to [`RamBudget::validate_resize`]; the effect
    /// happens only after validation passes.
    pub fn apply(&mut self, target: u64) -> Result<(u64, u64), AdapterError> {
        let target = self.validate_resize(target)?;
        let from = self.committed();
        self.buffer.resize(target as usize, 0);
        Ok((from, self.committed()))
    }

    /// Build an advisory candidate proposal for this adapter's single
    /// admitted transition (`reinterpret@capacity`).
    ///
    /// The magnitude is intent; [`RamBudget::apply`] re-validates it against
    /// bounds, usage, and step limits regardless of what planners propose.
    #[must_use]
    pub fn candidate(&self, target: u64) -> Option<TransitionCandidate> {
        let admitted = self.ir.transitions().iter().find(|admitted| {
            admitted.transition().mechanism() == TransitionMechanism::Reinterpret
                && admitted.transition().dimension() == &DimensionId::CAPACITY
        })?;
        Some(TransitionCandidate::from_admitted(admitted).with_magnitude(target))
    }
}
