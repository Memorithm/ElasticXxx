//! Generic kernel candidate requirements and admissibility checking.
//!
//! Requirements describe what an execution boundary must provide so a
//! candidate realization can run. Admissibility is decided here, once,
//! generically. Domain adapters translate their concrete kernels into these
//! requirements; the Elastic layer never learns what a tile or a subgroup
//! *means* to attention.
//!
//! Every check distinguishes "the boundary reported the feature as missing"
//! from "the boundary could not report on the feature". Unknown features are
//! rejected with [`RejectionReason::FeatureUnknown`] instead of being treated
//! as supported or unsupported.

use std::fmt;

use crate::capability::{CapabilitySnapshot, Feature};

/// Canonical namespace tag for requirement fingerprints.
pub(crate) const REQUIREMENTS_FINGERPRINT_DOMAIN: &str = "elastic-kernel/requirements/v1";
/// Canonical namespace tag for dispatch-grid fingerprints.
pub(crate) const DISPATCH_GRID_FINGERPRINT_DOMAIN: &str = "elastic-kernel/dispatch-grid/v1";

/// How strongly a candidate depends on one optional [`Feature`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FeatureRequirement {
    /// The candidate runs without the feature.
    NotRequired,
    /// The candidate is only admissible when the feature is reported known
    /// and present.
    Required,
}

/// What a candidate realization needs from an execution boundary.
///
/// All values are mandatory facts about the candidate itself; there are no
/// hidden defaults that could silently loosen the contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KernelRequirements {
    /// Total invocations the candidate launches per workgroup.
    pub invocations_per_workgroup: u32,
    /// Invocations the candidate uses along each workgroup axis `[x, y, z]`.
    pub invocations_per_axis: [u32; 3],
    /// Workgroup-addressable storage the candidate stages, in bytes.
    ///
    /// This is the candidate's own static declaration of staged storage; it
    /// is compared against the boundary's workgroup-storage limit.
    pub workgroup_storage_bytes: u64,
    /// Distinct bind groups the candidate dispatches with.
    pub bind_groups: u32,
    /// Largest storage-buffer binding the candidate issues, in bytes.
    pub max_storage_buffer_binding_bytes: u64,
    /// Subgroup dependence of the candidate.
    ///
    /// `Some(min_width)` declares that the candidate executes subgroup
    /// operations and requires a usable width of at least `min_width`;
    /// `None` declares that the candidate never executes subgroup
    /// operations.
    pub subgroup_min_width: Option<u32>,
    /// Dependence on native shader-f16.
    pub shader_f16: FeatureRequirement,
    /// Dependence on matrix-operation acceleration.
    pub matrix_ops: FeatureRequirement,
}

/// Workload-dependent dispatch geometry for one candidate realization.
///
/// Unlike [`KernelRequirements`], this is not an intrinsic static property of
/// the kernel implementation: the grid usually depends on the current
/// workload shape. Keeping it separate avoids baking workload facts into a
/// candidate identity while still letting the generic Elastic layer validate
/// dispatch limits before execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DispatchGrid {
    /// Number of workgroups dispatched on each axis `[x, y, z]`.
    pub workgroups_per_axis: [u32; 3],
}

impl DispatchGrid {
    /// Create one explicit dispatch grid.
    #[must_use]
    pub const fn new(workgroups_per_axis: [u32; 3]) -> Self {
        Self {
            workgroups_per_axis,
        }
    }

    /// Decide whether this workload-dependent grid fits `snapshot`.
    ///
    /// A zero extent is legal here: some execution APIs use zero-work grids
    /// to represent an intentional no-op. Domain adapters remain responsible
    /// for deciding whether that is semantically valid for their workload.
    ///
    /// # Errors
    ///
    /// Returns [`RejectionReason::DispatchGridExceeded`] for the first axis
    /// whose workgroup count exceeds the boundary limit.
    pub fn check_against(&self, snapshot: &CapabilitySnapshot) -> Result<(), RejectionReason> {
        let available = snapshot.workgroup_limits.max_workgroups_per_axis;
        for (axis, &required) in self.workgroups_per_axis.iter().enumerate() {
            if required > available {
                return Err(RejectionReason::DispatchGridExceeded {
                    axis,
                    required_workgroups: required,
                    available_workgroups: available,
                });
            }
        }
        Ok(())
    }

    /// Deterministic structural fingerprint of this dispatch grid.
    #[must_use]
    pub fn fingerprint(&self) -> elastic_eir::Fingerprint {
        let mut fp = elastic_eir::Fingerprint::EMPTY.text(DISPATCH_GRID_FINGERPRINT_DOMAIN);
        for workgroups in self.workgroups_per_axis {
            fp = fp.number(u64::from(workgroups));
        }
        fp
    }
}

impl KernelRequirements {
    /// Validate the internal consistency of the requirement record.
    ///
    /// # Errors
    ///
    /// Returns [`RequirementsError::ZeroResourceNeed`] when any mandatory
    /// resource need is zero, [`RequirementsError::AxisExceedsWorkgroup`]
    /// when an axis invocation count exceeds the declared per-workgroup
    /// total, and [`RequirementsError::InvalidSubgroupMinimum`] when a
    /// subgroup minimum width is zero.
    pub fn validate(&self) -> Result<(), RequirementsError> {
        if self.invocations_per_workgroup == 0
            || self.workgroup_storage_bytes == 0
            || self.bind_groups == 0
        {
            return Err(RequirementsError::ZeroResourceNeed);
        }
        for (axis, invocations) in self.invocations_per_axis.iter().enumerate() {
            if *invocations > self.invocations_per_workgroup {
                return Err(RequirementsError::AxisExceedsWorkgroup {
                    axis,
                    axis_invocations: *invocations,
                    invocations_per_workgroup: self.invocations_per_workgroup,
                });
            }
        }
        let declared_axis_sum = self
            .invocations_per_axis
            .iter()
            .try_fold(1u64, |product, &invocations| {
                product.checked_mul(u64::from(invocations))
            })
            .ok_or(RequirementsError::AxisProductOverflow)?;
        if declared_axis_sum > u64::from(self.invocations_per_workgroup) {
            return Err(RequirementsError::AxisExceedsWorkgroup {
                axis: 0,
                axis_invocations: self.invocations_per_axis[0],
                invocations_per_workgroup: self.invocations_per_workgroup,
            });
        }
        if let Some(min_width) = self.subgroup_min_width {
            if min_width == 0 {
                return Err(RequirementsError::InvalidSubgroupMinimum);
            }
        }
        Ok(())
    }

    /// Decide whether this candidate's requirements hold on `snapshot`.
    ///
    /// Field evaluation order is fixed by this method, so identical inputs
    /// always produce identical rejection reasons.
    ///
    /// # Errors
    ///
    /// Returns the first rejection reason in evaluation order when the
    /// candidate cannot run on the snapshot.
    pub fn check_against(&self, snapshot: &CapabilitySnapshot) -> Result<(), RejectionReason> {
        let limits = &snapshot.workgroup_limits;
        if self.invocations_per_workgroup > limits.max_invocations_per_workgroup {
            return Err(RejectionReason::InvocationsPerWorkgroupExceeded {
                required: self.invocations_per_workgroup,
                available: limits.max_invocations_per_workgroup,
            });
        }
        for (axis, (&required, &available)) in self
            .invocations_per_axis
            .iter()
            .zip(limits.max_invocations_per_axis.iter())
            .enumerate()
        {
            if required > available {
                return Err(RejectionReason::AxisSizeExceeded {
                    axis,
                    required,
                    available,
                });
            }
        }
        if self.workgroup_storage_bytes > limits.max_workgroup_storage_bytes {
            return Err(RejectionReason::WorkgroupStorageExceeded {
                required_bytes: self.workgroup_storage_bytes,
                available_bytes: limits.max_workgroup_storage_bytes,
            });
        }
        if self.bind_groups > snapshot.binding_limits.max_bind_groups {
            return Err(RejectionReason::BindGroupsExceeded {
                required: self.bind_groups,
                available: snapshot.binding_limits.max_bind_groups,
            });
        }
        if self.max_storage_buffer_binding_bytes
            > snapshot.binding_limits.max_storage_buffer_binding_bytes
        {
            return Err(RejectionReason::StorageBufferBindingExceeded {
                required_bytes: self.max_storage_buffer_binding_bytes,
                available_bytes: snapshot.binding_limits.max_storage_buffer_binding_bytes,
            });
        }
        if let Some(min_width) = self.subgroup_min_width {
            let support = snapshot.subgroup_support;
            if !support.supported {
                return Err(RejectionReason::SubgroupUnsupported);
            }
            match support.size_range {
                Some((_, max_size)) if max_size >= min_width => {}
                Some((_, max_size)) => {
                    return Err(RejectionReason::SubgroupWidthInsufficient {
                        required_min_width: min_width,
                        available_max_width: max_size,
                    });
                }
                // Snapshot validation guarantees a range when supported; this
                // arm is unreachable for validated snapshots and stays honest
                // rather than assuming.
                None => {
                    return Err(RejectionReason::SubgroupUnsupported);
                }
            }
        }
        self.check_feature(Feature::ShaderF16, self.shader_f16, snapshot)?;
        self.check_feature(Feature::MatrixOps, self.matrix_ops, snapshot)?;
        Ok(())
    }

    fn check_feature(
        &self,
        feature: Feature,
        requirement: FeatureRequirement,
        snapshot: &CapabilitySnapshot,
    ) -> Result<(), RejectionReason> {
        if requirement != FeatureRequirement::Required {
            return Ok(());
        }
        match snapshot.feature_support(feature).known_value() {
            Some(true) => Ok(()),
            Some(false) => Err(RejectionReason::FeatureUnsupported { feature }),
            None => Err(RejectionReason::FeatureUnknown { feature }),
        }
    }

    /// Deterministic structural fingerprint of the requirement record.
    #[must_use]
    pub fn fingerprint(&self) -> elastic_eir::Fingerprint {
        let mut fp = elastic_eir::Fingerprint::EMPTY.text(REQUIREMENTS_FINGERPRINT_DOMAIN);
        fp = fp.number(u64::from(self.invocations_per_workgroup));
        for invocations in self.invocations_per_axis {
            fp = fp.number(u64::from(invocations));
        }
        fp = fp.number(self.workgroup_storage_bytes);
        fp = fp.number(u64::from(self.bind_groups));
        fp = fp.number(self.max_storage_buffer_binding_bytes);
        fp = match self.subgroup_min_width {
            Some(min_width) => fp.number(u64::from(min_width)),
            None => fp.number(u64::MAX),
        };
        fp = fp.text(feature_token(self.shader_f16));
        fp.text(feature_token(self.matrix_ops))
    }
}

fn feature_token(requirement: FeatureRequirement) -> &'static str {
    match requirement {
        FeatureRequirement::NotRequired => "not-required",
        FeatureRequirement::Required => "required",
    }
}

/// Errors produced while validating requirement records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RequirementsError {
    /// A mandatory resource need was zero.
    ZeroResourceNeed,
    /// An axis invocation count exceeded the declared per-workgroup total.
    AxisExceedsWorkgroup {
        /// Rejected axis index.
        axis: usize,
        /// Axis invocation declaration.
        axis_invocations: u32,
        /// Declared total invocations per workgroup.
        invocations_per_workgroup: u32,
    },
    /// The product of axis invocations overflowed during validation.
    AxisProductOverflow,
    /// A subgroup minimum width was zero.
    InvalidSubgroupMinimum,
}

impl fmt::Display for RequirementsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroResourceNeed => {
                write!(f, "resource needs must be positive")
            }
            Self::AxisExceedsWorkgroup {
                axis,
                axis_invocations,
                invocations_per_workgroup,
            } => write!(
                f,
                "axis {axis} declares {axis_invocations} invocations, exceeding the {invocations_per_workgroup} invocations declared per workgroup"
            ),
            Self::AxisProductOverflow => {
                write!(f, "axis invocation product overflows during validation")
            }
            Self::InvalidSubgroupMinimum => {
                write!(f, "subgroup minimum width must be positive")
            }
        }
    }
}

impl std::error::Error for RequirementsError {}

/// Why a candidate realization was rejected for one capability snapshot.
///
/// Reasons are explicit enough to appear verbatim in auditable selection
/// evidence. Unknown features are distinct from unsupported features.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RejectionReason {
    /// Candidate invocations per workgroup exceed the device limit.
    InvocationsPerWorkgroupExceeded {
        /// Required invocations.
        required: u32,
        /// Device limit.
        available: u32,
    },
    /// Candidate invocations along one axis exceed the device limit.
    AxisSizeExceeded {
        /// Axis index (`0` = x, `1` = y, `2` = z).
        axis: usize,
        /// Required invocations.
        required: u32,
        /// Device limit.
        available: u32,
    },
    /// Workload-dependent dispatch workgroups exceed the per-axis boundary
    /// limit.
    DispatchGridExceeded {
        /// Axis index (`0` = x, `1` = y, `2` = z).
        axis: usize,
        /// Workgroups the workload requires on this axis.
        required_workgroups: u32,
        /// Workgroups the boundary permits on this axis.
        available_workgroups: u32,
    },
    /// Staged workgroup storage exceeds the device limit.
    WorkgroupStorageExceeded {
        /// Required bytes.
        required_bytes: u64,
        /// Device limit in bytes.
        available_bytes: u64,
    },
    /// Bind-group count exceeds the device limit.
    BindGroupsExceeded {
        /// Required bind groups.
        required: u32,
        /// Device limit.
        available: u32,
    },
    /// Storage-buffer binding size exceeds the device limit.
    StorageBufferBindingExceeded {
        /// Required bytes.
        required_bytes: u64,
        /// Device limit in bytes.
        available_bytes: u64,
    },
    /// The candidate executes subgroup operations but the boundary does not
    /// report subgroup support.
    SubgroupUnsupported,
    /// The boundary supports subgroups but not at the candidate's minimum
    /// usable width.
    SubgroupWidthInsufficient {
        /// Minimum usable width the candidate declared.
        required_min_width: u32,
        /// Largest width the boundary reported.
        available_max_width: u32,
    },
    /// The boundary explicitly reported the feature as absent.
    FeatureUnsupported {
        /// Missing feature.
        feature: Feature,
    },
    /// The boundary could not report on the feature. This is deliberately
    /// distinct from [`RejectionReason::FeatureUnsupported`]: unknown is not
    /// evidence of absence.
    FeatureUnknown {
        /// Unreported feature.
        feature: Feature,
    },
}

impl fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvocationsPerWorkgroupExceeded { required, available } => write!(
                f,
                "requires {required} invocations per workgroup, boundary allows {available}"
            ),
            Self::AxisSizeExceeded {
                axis,
                required,
                available,
            } => write!(
                f,
                "requires {required} invocations on axis {axis}, boundary allows {available}"
            ),
            Self::DispatchGridExceeded {
                axis,
                required_workgroups,
                available_workgroups,
            } => write!(
                f,
                "requires {required_workgroups} dispatch workgroups on axis {axis}, boundary allows {available_workgroups}"
            ),
            Self::WorkgroupStorageExceeded {
                required_bytes,
                available_bytes,
            } => write!(
                f,
                "requires {required_bytes} bytes of workgroup storage, boundary allows {available_bytes}"
            ),
            Self::BindGroupsExceeded { required, available } => write!(
                f,
                "requires {required} bind groups, boundary allows {available}"
            ),
            Self::StorageBufferBindingExceeded {
                required_bytes,
                available_bytes,
            } => write!(
                f,
                "requires a {required_bytes}-byte storage binding, boundary allows {available_bytes}"
            ),
            Self::SubgroupUnsupported => write!(
                f,
                "executes subgroup operations but the boundary reports no subgroup support"
            ),
            Self::SubgroupWidthInsufficient {
                required_min_width,
                available_max_width,
            } => write!(
                f,
                "needs subgroup width >= {required_min_width}, boundary reports maximum {available_max_width}"
            ),
            Self::FeatureUnsupported { feature } => {
                write!(f, "feature {feature} is reported unavailable")
            }
            Self::FeatureUnknown { feature } => {
                write!(f, "feature {feature} was not reported (unknown, not assumed absent)")
            }
        }
    }
}

impl std::error::Error for RejectionReason {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{BindingLimits, FeatureSupport, SubgroupSupport, WorkgroupLimits};

    fn portable_snapshot() -> CapabilitySnapshot {
        CapabilitySnapshot {
            workgroup_limits: WorkgroupLimits {
                max_invocations_per_axis: [1024, 1024, 64],
                max_invocations_per_workgroup: 1024,
                max_workgroups_per_axis: 65535,
                max_workgroup_storage_bytes: 48 << 10,
            },
            binding_limits: BindingLimits {
                max_bind_groups: 8,
                max_storage_buffer_binding_bytes: 128 << 20,
            },
            subgroup_support: SubgroupSupport::unsupported(),
            shader_f16: FeatureSupport::Known(false),
            matrix_ops: FeatureSupport::Unknown,
        }
    }

    fn portable_requirements() -> KernelRequirements {
        KernelRequirements {
            invocations_per_workgroup: 64,
            invocations_per_axis: [64, 1, 1],
            workgroup_storage_bytes: 24_576,
            bind_groups: 4,
            max_storage_buffer_binding_bytes: 64 << 20,
            subgroup_min_width: None,
            shader_f16: FeatureRequirement::NotRequired,
            matrix_ops: FeatureRequirement::NotRequired,
        }
    }

    #[test]
    fn portable_candidate_is_admissible_on_portable_boundary() {
        assert_eq!(
            portable_requirements().check_against(&portable_snapshot()),
            Ok(())
        );
    }

    #[test]
    fn dispatch_grid_accepts_exact_limit_on_every_axis() {
        let grid = DispatchGrid::new([65_535, 65_535, 65_535]);
        assert_eq!(grid.check_against(&portable_snapshot()), Ok(()));
    }

    #[test]
    fn dispatch_grid_rejects_first_axis_over_limit_with_typed_reason() {
        let grid = DispatchGrid::new([65_535, 65_536, 1]);
        assert_eq!(
            grid.check_against(&portable_snapshot()),
            Err(RejectionReason::DispatchGridExceeded {
                axis: 1,
                required_workgroups: 65_536,
                available_workgroups: 65_535,
            })
        );
    }

    #[test]
    fn dispatch_grid_fingerprint_is_deterministic_and_geometry_sensitive() {
        let baseline = DispatchGrid::new([128, 2, 1]);
        assert_eq!(baseline.fingerprint(), baseline.fingerprint());
        assert_ne!(
            baseline.fingerprint(),
            DispatchGrid::new([129, 2, 1]).fingerprint()
        );
        assert_ne!(
            baseline.fingerprint(),
            DispatchGrid::new([128, 1, 2]).fingerprint()
        );
    }

    #[test]
    fn oversized_workgroup_storage_is_rejected_with_measured_reason() {
        let mut requirements = portable_requirements();
        requirements.workgroup_storage_bytes = 96 << 10;
        assert_eq!(
            requirements.check_against(&portable_snapshot()),
            Err(RejectionReason::WorkgroupStorageExceeded {
                required_bytes: 96 << 10,
                available_bytes: 48 << 10,
            })
        );
    }

    #[test]
    fn subgroup_dependent_candidate_is_rejected_without_subgroups() {
        let mut requirements = portable_requirements();
        requirements.subgroup_min_width = Some(4);
        assert_eq!(
            requirements.check_against(&portable_snapshot()),
            Err(RejectionReason::SubgroupUnsupported)
        );
    }

    #[test]
    fn subgroup_width_must_be_covered_by_the_boundary_range() {
        let mut snapshot = portable_snapshot();
        snapshot.subgroup_support = SubgroupSupport::supported(8, 16).expect("valid range");
        let mut requirements = portable_requirements();
        requirements.subgroup_min_width = Some(32);
        assert_eq!(
            requirements.check_against(&snapshot),
            Err(RejectionReason::SubgroupWidthInsufficient {
                required_min_width: 32,
                available_max_width: 16,
            })
        );
        requirements.subgroup_min_width = Some(16);
        assert_eq!(requirements.check_against(&snapshot), Ok(()));
    }

    #[test]
    fn unknown_feature_is_not_treated_as_supported_or_absent() {
        let mut requirements = portable_requirements();
        requirements.shader_f16 = FeatureRequirement::Required;
        // Snapshot reports Known(false).
        assert_eq!(
            requirements.check_against(&portable_snapshot()),
            Err(RejectionReason::FeatureUnsupported {
                feature: Feature::ShaderF16,
            })
        );
        // Snapshot reports nothing.
        let mut snapshot = portable_snapshot();
        snapshot.shader_f16 = crate::capability::FeatureSupport::Unknown;
        assert_eq!(
            requirements.check_against(&snapshot),
            Err(RejectionReason::FeatureUnknown {
                feature: Feature::ShaderF16,
            })
        );
    }

    #[test]
    fn inconsistent_requirement_records_are_rejected_at_validation() {
        let mut requirements = portable_requirements();
        requirements.invocations_per_workgroup = 0;
        assert_eq!(
            requirements.validate(),
            Err(RequirementsError::ZeroResourceNeed)
        );

        let mut requirements = portable_requirements();
        requirements.invocations_per_axis = [128, 1, 1];
        requirements.invocations_per_workgroup = 64;
        assert_eq!(
            requirements.validate(),
            Err(RequirementsError::AxisExceedsWorkgroup {
                axis: 0,
                axis_invocations: 128,
                invocations_per_workgroup: 64,
            })
        );

        let mut requirements = portable_requirements();
        requirements.subgroup_min_width = Some(0);
        assert_eq!(
            requirements.validate(),
            Err(RequirementsError::InvalidSubgroupMinimum)
        );
    }

    #[test]
    fn requirement_fingerprint_is_deterministic_and_sensitive() {
        let baseline = portable_requirements();
        assert_eq!(baseline.fingerprint(), baseline.fingerprint());
        let mut subgroup = baseline;
        subgroup.subgroup_min_width = Some(4);
        assert_ne!(baseline.fingerprint(), subgroup.fingerprint());
    }
}
