//! Generic compute-capability snapshots for kernel-realization planning.
//!
//! A [`CapabilitySnapshot`] is a deterministic, backend-neutral record of the
//! facts an execution boundary reports about itself. It contains no vendor,
//! product, or backend identifiers: hardware-specific knowledge enters the
//! Elastic model exclusively through adapters that translate their discovery
//! data into this shape.
//!
//! Honesty rules:
//!
//! - A capability that was not reported is represented as
//!   [`FeatureSupport::Unknown`], never as `false`. Treating "unknown" as
//!   "absent" silently disqualifies candidates; treating it as "present"
//!   fabricates support. Both are dishonest, so the distinction is explicit.
//! - The snapshot records declarations; it does not authenticate who made
//!   them. As with `elastic-core::CapabilitySet`, a trusted runtime must
//!   construct snapshots from authoritative discovery/configuration.
//! - Fingerprints are structural (see `elastic_eir::Fingerprint`) and valid
//!   inside one trust domain. They are cache keys and evidence anchors, not
//!   cryptographic identities.

use std::fmt;

use elastic_eir::Fingerprint;

/// Canonical namespace tag absorbed first by every fingerprint in this crate.
///
/// Changing any semantic of this crate's fingerprint inputs must change this
/// tag so that fingerprints from different schema generations can never be
/// compared by accident.
pub(crate) const CAPABILITY_SNAPSHOT_FINGERPRINT_DOMAIN: &str =
    "elastic-kernel/capability-snapshot/v1";

/// Whether an optional feature is reported present, reported absent, or not
/// reported at all.
///
/// This deliberately distinguishes *unknown* from *false*: a discovery layer
/// that cannot observe a feature must not be read as evidence that the feature
/// is missing, and planners must reject requirements on unknown features with
/// a dedicated reason instead of silently guessing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FeatureSupport {
    /// The boundary explicitly reported the feature as available.
    Known(bool),
    /// The boundary could not report on this feature.
    Unknown,
}

impl FeatureSupport {
    /// The known value, if this is not [`FeatureSupport::Unknown`].
    #[must_use]
    pub const fn known_value(self) -> Option<bool> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown => None,
        }
    }

    /// Canonical single-token text used in canonical records and fingerprints.
    #[must_use]
    pub const fn canonical_token(self) -> &'static str {
        match self {
            Self::Known(true) => "known-true",
            Self::Known(false) => "known-false",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for FeatureSupport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical_token())
    }
}

/// Workgroup geometry limits of an execution boundary.
///
/// Every field is a mandatory, positively-valued fact. Execution boundaries
/// without workgroups cannot produce meaningful values and must not
/// fabricate a snapshot of this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkgroupLimits {
    /// Maximum invocations along each workgroup axis `[x, y, z]`.
    pub max_invocations_per_axis: [u32; 3],
    /// Maximum total invocations within one workgroup.
    pub max_invocations_per_workgroup: u32,
    /// Maximum workgroups dispatchable along one grid axis.
    pub max_workgroups_per_axis: u32,
    /// Maximum workgroup-addressable storage in bytes.
    pub max_workgroup_storage_bytes: u64,
}

impl WorkgroupLimits {
    const MIN_POSITIVE: u32 = 1;

    fn validate(&self) -> Result<(), CapabilityError> {
        for (axis, invocations) in self.max_invocations_per_axis.iter().enumerate() {
            if *invocations < Self::MIN_POSITIVE {
                return Err(CapabilityError::NonPositiveLimit {
                    limit: LimitKind::WorkgroupAxisInvocations { axis },
                });
            }
        }
        if self.max_invocations_per_workgroup < Self::MIN_POSITIVE {
            return Err(CapabilityError::NonPositiveLimit {
                limit: LimitKind::WorkgroupInvocations,
            });
        }
        if self.max_workgroups_per_axis < Self::MIN_POSITIVE {
            return Err(CapabilityError::NonPositiveLimit {
                limit: LimitKind::DispatchAxisWorkgroups,
            });
        }
        if self.max_workgroup_storage_bytes == 0 {
            return Err(CapabilityError::NonPositiveLimit {
                limit: LimitKind::WorkgroupStorageBytes,
            });
        }
        Ok(())
    }
}

/// Binding-resource limits of an execution boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BindingLimits {
    /// Maximum number of distinct bind groups per dispatch.
    pub max_bind_groups: u32,
    /// Largest single storage-buffer binding in bytes.
    pub max_storage_buffer_binding_bytes: u64,
}

impl BindingLimits {
    fn validate(&self) -> Result<(), CapabilityError> {
        if self.max_bind_groups == 0 {
            return Err(CapabilityError::NonPositiveLimit {
                limit: LimitKind::BindGroups,
            });
        }
        if self.max_storage_buffer_binding_bytes == 0 {
            return Err(CapabilityError::NonPositiveLimit {
                limit: LimitKind::StorageBufferBindingBytes,
            });
        }
        Ok(())
    }
}

/// Subgroup-execution support reported by an execution boundary.
///
/// When [`SubgroupSupport::supported`] is `false`, the size range must be
/// absent; a boundary that claims no subgroups cannot claim subgroup widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubgroupSupport {
    /// Whether subgroup operations are declared executable at all.
    pub supported: bool,
    /// Declared inclusive width range `[min, max]`, present only when
    /// supported.
    pub size_range: Option<(u32, u32)>,
}

impl SubgroupSupport {
    /// Support declaration without subgroup availability.
    #[must_use]
    pub const fn unsupported() -> Self {
        Self {
            supported: false,
            size_range: None,
        }
    }

    /// Support declaration with an inclusive width range.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityError::InvalidSubgroupRange`] unless
    /// `1 <= min <= max`.
    pub fn supported(min_size: u32, max_size: u32) -> Result<Self, CapabilityError> {
        if min_size == 0 || max_size < min_size {
            return Err(CapabilityError::InvalidSubgroupRange { min_size, max_size });
        }
        Ok(Self {
            supported: true,
            size_range: Some((min_size, max_size)),
        })
    }

    fn validate(&self) -> Result<(), CapabilityError> {
        match self.size_range {
            Some((min_size, max_size)) => {
                if !self.supported {
                    return Err(CapabilityError::RangeWithoutSupport);
                }
                if min_size == 0 || max_size < min_size {
                    return Err(CapabilityError::InvalidSubgroupRange { min_size, max_size });
                }
            }
            None if self.supported => {
                return Err(CapabilityError::MissingSubgroupRange);
            }
            None => {}
        }
        Ok(())
    }
}

/// Kinds of optional features this capability model can express.
///
/// The set is intentionally small: only features with either executable FLAT
/// variants or near-term roadmap obligations appear here. Extension happens by
/// adding variants together with their requirement semantics, not by
/// untyped strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    /// Native shader binary16 arithmetic.
    ShaderF16,
    /// Matrix-operation acceleration descriptors.
    ///
    /// Reporting this feature describes what the boundary declares. It does
    /// not imply that any candidate able to exploit it exists; a capability
    /// may exist without an executable candidate.
    MatrixOps,
}

impl Feature {
    /// Stable canonical name used in records, errors, and fingerprints.
    #[must_use]
    pub const fn canonical(self) -> &'static str {
        match self {
            Self::ShaderF16 => "shader-f16",
            Self::MatrixOps => "matrix-ops",
        }
    }
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical())
    }
}

/// One normalized capability observation of an execution boundary.
///
/// Construction validates internal consistency so that later stages
/// (admissibility checks, fingerprinting, planning) never observe an
/// impossible snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CapabilitySnapshot {
    /// Workgroup geometry limits.
    pub workgroup_limits: WorkgroupLimits,
    /// Binding-resource limits.
    pub binding_limits: BindingLimits,
    /// Subgroup execution support.
    pub subgroup_support: SubgroupSupport,
    /// Native shader-f16 support report.
    pub shader_f16: FeatureSupport,
    /// Matrix-operation acceleration report.
    pub matrix_ops: FeatureSupport,
}

impl CapabilitySnapshot {
    /// Validate and normalize a capability snapshot.
    ///
    /// # Errors
    ///
    /// Returns a [`CapabilityError`] when any limit is zero, when the subgroup
    /// declaration is internally inconsistent, or when an unsupported
    /// subgroup declaration carries a width range.
    pub fn new(snapshot: Self) -> Result<Self, CapabilityError> {
        snapshot.validate()?;
        Ok(snapshot)
    }

    /// Check internal consistency of an already-constructed snapshot.
    ///
    /// This is the same validation [`CapabilitySnapshot::new`] applies;
    /// it is public so adapters that assemble snapshots field by field can
    /// validate without reconstructing.
    ///
    /// # Errors
    ///
    /// See [`CapabilitySnapshot::new`].
    pub fn validate(&self) -> Result<(), CapabilityError> {
        self.workgroup_limits.validate()?;
        self.binding_limits.validate()?;
        self.subgroup_support.validate()?;
        Ok(())
    }

    /// Reported support for one [`Feature`].
    #[must_use]
    pub const fn feature_support(&self, feature: Feature) -> FeatureSupport {
        match feature {
            Feature::ShaderF16 => self.shader_f16,
            Feature::MatrixOps => self.matrix_ops,
        }
    }

    /// Deterministic structural fingerprint of this snapshot.
    ///
    /// Field order is fixed by this method, not by construction order, so the
    /// same logical capabilities always produce the same fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> Fingerprint {
        let mut fp = Fingerprint::EMPTY.text(CAPABILITY_SNAPSHOT_FINGERPRINT_DOMAIN);
        let limits = &self.workgroup_limits;
        for invocations in limits.max_invocations_per_axis {
            fp = fp.number(u64::from(invocations));
        }
        fp = fp.number(u64::from(limits.max_invocations_per_workgroup));
        fp = fp.number(u64::from(limits.max_workgroups_per_axis));
        fp = fp.number(limits.max_workgroup_storage_bytes);
        fp = fp.number(u64::from(self.binding_limits.max_bind_groups));
        fp = fp.number(self.binding_limits.max_storage_buffer_binding_bytes);
        fp = fp.number(u64::from(self.subgroup_support.supported));
        match self.subgroup_support.size_range {
            Some((min_size, max_size)) => {
                fp = fp.number(u64::from(min_size));
                fp = fp.number(u64::from(max_size));
            }
            None => {
                fp = fp.number(u64::MAX);
                fp = fp.number(u64::MAX);
            }
        }
        fp = fp.text(self.shader_f16.canonical_token());
        fp = fp.text(self.matrix_ops.canonical_token());
        fp
    }
}

/// Category of an invalid limit observed during snapshot validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitKind {
    /// Invocations along one workgroup axis.
    WorkgroupAxisInvocations {
        /// Axis index (`0` = x, `1` = y, `2` = z).
        axis: usize,
    },
    /// Total invocations per workgroup.
    WorkgroupInvocations,
    /// Workgroups along one dispatch-grid axis.
    DispatchAxisWorkgroups,
    /// Workgroup-addressable storage bytes.
    WorkgroupStorageBytes,
    /// Distinct bind groups per dispatch.
    BindGroups,
    /// Storage-buffer binding bytes.
    StorageBufferBindingBytes,
}

/// Errors produced while constructing capability snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CapabilityError {
    /// A mandatory limit was zero or otherwise non-positive.
    NonPositiveLimit {
        /// Which limit was rejected.
        limit: LimitKind,
    },
    /// Subgroup widths were inconsistent (`min > max` or zero).
    InvalidSubgroupRange {
        /// Rejected minimum width.
        min_size: u32,
        /// Rejected maximum width.
        max_size: u32,
    },
    /// A subgroup width range was supplied while declaring subgroups
    /// unsupported.
    RangeWithoutSupport,
    /// Subgroups were declared supported without a width range.
    MissingSubgroupRange,
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonPositiveLimit { limit } => {
                write!(f, "capability limit {limit:?} must be positive")
            }
            Self::InvalidSubgroupRange { min_size, max_size } => write!(
                f,
                "subgroup size range [{min_size}, {max_size}] requires 1 <= min <= max"
            ),
            Self::RangeWithoutSupport => write!(
                f,
                "subgroup size range supplied while subgroups are declared unsupported"
            ),
            Self::MissingSubgroupRange => {
                write!(f, "subgroups declared supported without a size range")
            }
        }
    }
}

impl std::error::Error for CapabilityError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn portable_snapshot() -> CapabilitySnapshot {
        CapabilitySnapshot {
            workgroup_limits: WorkgroupLimits {
                max_invocations_per_axis: [64, 64, 64],
                max_invocations_per_workgroup: 256,
                max_workgroups_per_axis: 65535,
                max_workgroup_storage_bytes: 32_768,
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

    #[test]
    fn portable_snapshot_is_accepted() {
        assert_eq!(
            CapabilitySnapshot::new(portable_snapshot()),
            Ok(portable_snapshot())
        );
    }

    #[test]
    fn zero_limits_are_rejected() {
        let mut snapshot = portable_snapshot();
        snapshot.workgroup_limits.max_workgroup_storage_bytes = 0;
        assert_eq!(
            CapabilitySnapshot::new(snapshot),
            Err(CapabilityError::NonPositiveLimit {
                limit: LimitKind::WorkgroupStorageBytes,
            })
        );
    }

    #[test]
    fn subgroup_declaration_must_be_internally_consistent() {
        let mut snapshot = portable_snapshot();
        snapshot.subgroup_support = SubgroupSupport::unsupported();
        snapshot.subgroup_support.size_range = Some((8, 8));
        assert_eq!(
            CapabilitySnapshot::new(snapshot),
            Err(CapabilityError::RangeWithoutSupport)
        );

        let mut snapshot = portable_snapshot();
        snapshot.subgroup_support = SubgroupSupport {
            supported: true,
            size_range: None,
        };
        assert_eq!(
            CapabilitySnapshot::new(snapshot),
            Err(CapabilityError::MissingSubgroupRange)
        );

        assert!(SubgroupSupport::supported(4, 3).is_err());
        assert!(SubgroupSupport::supported(0, 4).is_err());
        assert!(SubgroupSupport::supported(8, 8).is_ok());
    }

    #[test]
    fn fingerprint_is_deterministic_and_sensitive() {
        let baseline = portable_snapshot();
        assert_eq!(baseline.fingerprint(), baseline.fingerprint());

        let mut larger_storage = baseline;
        larger_storage.workgroup_limits.max_workgroup_storage_bytes *= 2;
        assert_ne!(baseline.fingerprint(), larger_storage.fingerprint());

        let mut subgroup = portable_snapshot();
        subgroup.subgroup_support = SubgroupSupport::supported(4, 64).expect("valid range");
        assert_ne!(baseline.fingerprint(), subgroup.fingerprint());

        // Unknown must never alias Known(false); the two mean different
        // things to planners.
        let mut unknown_f16 = baseline;
        unknown_f16.shader_f16 = FeatureSupport::Unknown;
        assert_ne!(baseline.fingerprint(), unknown_f16.fingerprint());
    }
}
