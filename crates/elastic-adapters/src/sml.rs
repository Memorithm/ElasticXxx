//! Versioned SML-GENIUS elastic-weight plan adapter.
//!
//! SML owns page identity, learned-parameter semantics, importance and page
//! selection. ElasticXxx independently revalidates the published SML plan
//! contract and projects precision changes into the generic representation
//! transition model. This module does not physically move or re-encode weights.

use elastic_core::{
    CapabilitySet, RepresentationEpoch, RepresentationId, RepresentationState,
    RepresentationTransition, TransitionAttestations, TransitionError, TransitionMechanism,
};
use std::collections::BTreeSet;
use std::fmt;

/// SML plan contract qualified by this adapter.
pub const SML_ELASTIC_WEIGHT_PLAN_V1: &str = "sml.elastic-weight-plan@1.0.0";
/// Repository that owns the source contract.
pub const SML_ELASTIC_WEIGHT_SOURCE_REPOSITORY_V1: &str = "Memorithm/SML-GENIUS";
/// Exact merged SML revision reviewed for this adapter.
pub const SML_ELASTIC_WEIGHT_SOURCE_COMMIT_V1: &str = "3e04239862c9d14cb369227f4abbe84b5c7e1d5e";
/// Elastic representation contract version used for SML precision classes.
pub const SML_WEIGHT_REPRESENTATION_SCHEMA_V1: u32 = 1;
/// Boolean SML weight representation identity.
pub const SML_WEIGHT_BOOLEAN_REPRESENTATION_V1: &str = "sml.weight.boolean";
/// Ternary SML weight representation identity.
pub const SML_WEIGHT_TERNARY_REPRESENTATION_V1: &str = "sml.weight.ternary";
/// Four-bit residual SML weight representation identity.
pub const SML_WEIGHT_RESIDUAL4_REPRESENTATION_V1: &str = "sml.weight.residual4";

/// SML physical precision class from the qualified v1 contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SmlWeightPrecisionV1 {
    Boolean,
    Ternary,
    Residual4,
}

impl SmlWeightPrecisionV1 {
    /// Exact semantic bits per learned parameter.
    #[must_use]
    pub const fn bits_per_parameter(self) -> u8 {
        match self {
            Self::Boolean => 1,
            Self::Ternary => 2,
            Self::Residual4 => 4,
        }
    }

    /// Stable representation identifier text used by ElasticXxx.
    #[must_use]
    pub const fn representation_id_text(self) -> &'static str {
        match self {
            Self::Boolean => SML_WEIGHT_BOOLEAN_REPRESENTATION_V1,
            Self::Ternary => SML_WEIGHT_TERNARY_REPRESENTATION_V1,
            Self::Residual4 => SML_WEIGHT_RESIDUAL4_REPRESENTATION_V1,
        }
    }

    fn representation_id(self) -> Result<RepresentationId, SmlElasticWeightAdapterError> {
        RepresentationId::new(self.representation_id_text())
            .map_err(SmlElasticWeightAdapterError::Representation)
    }
}

/// SML exclusive residency tier from the qualified v1 contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SmlWeightResidencyV1 {
    Disk,
    Ram,
    Vram,
}

/// One raw page transition from an SML v1 elastic-weight plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmlElasticWeightTransitionV1 {
    pub page_id: u32,
    pub parameters: u64,
    pub from_representation_version: u64,
    pub to_representation_version: u64,
    pub from_precision: SmlWeightPrecisionV1,
    pub to_precision: SmlWeightPrecisionV1,
    pub from_residency: SmlWeightResidencyV1,
    pub to_residency: SmlWeightResidencyV1,
    pub from_active: bool,
    pub to_active: bool,
    pub target_payload_bytes: u64,
}

impl SmlElasticWeightTransitionV1 {
    /// Project a precision change into ElasticXxx's generic representation model.
    ///
    /// Residency-only changes return Ok(None): residency is a separate resource
    /// axis and must not be disguised as a representation transition.
    ///
    /// SML does not authorize a materialization mechanism. The trusted caller
    /// must supply either Reencode or Recompute and then provide the
    /// corresponding ElasticXxx attestation during validation.
    pub fn representation_transition(
        self,
        mechanism: TransitionMechanism,
    ) -> Result<Option<RepresentationTransition>, SmlElasticWeightAdapterError> {
        if self.from_precision == self.to_precision {
            return Ok(None);
        }
        if mechanism == TransitionMechanism::Reinterpret {
            return Err(SmlElasticWeightAdapterError::ReinterpretPrecisionChange);
        }

        let from = RepresentationState::new(
            self.from_precision.representation_id()?,
            SML_WEIGHT_REPRESENTATION_SCHEMA_V1,
            RepresentationEpoch::new(self.from_representation_version),
        );
        let to = RepresentationState::new(
            self.to_precision.representation_id()?,
            SML_WEIGHT_REPRESENTATION_SCHEMA_V1,
            RepresentationEpoch::new(self.to_representation_version),
        );

        Ok(Some(RepresentationTransition {
            from,
            to,
            mechanism,
        }))
    }

    /// Project and validate a precision transition against trusted capabilities
    /// and transition-bound evidence.
    pub fn validate_representation_transition(
        self,
        mechanism: TransitionMechanism,
        capabilities: &CapabilitySet,
        attestations: TransitionAttestations,
    ) -> Result<Option<RepresentationTransition>, SmlElasticWeightAdapterError> {
        let Some(transition) = self.representation_transition(mechanism)? else {
            return Ok(None);
        };
        transition
            .validate(capabilities, attestations)
            .map_err(SmlElasticWeightAdapterError::Representation)?;
        Ok(Some(transition))
    }
}

/// Untrusted/native envelope matching the self-contained SML v1 plan fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmlElasticWeightPlanEnvelopeV1 {
    pub contract: String,
    pub source_commit: String,
    pub base_generation: u64,
    pub epoch: u64,
    pub ram_limit_bytes: u64,
    pub vram_limit_bytes: u64,
    pub max_active_parameters: u64,
    pub transitions: Vec<SmlElasticWeightTransitionV1>,
    pub ram_bytes: u64,
    pub vram_bytes: u64,
    pub active_parameters: u64,
    pub active_storage_bits: u128,
    pub bytes_moved_to_ram: u64,
    pub bytes_moved_to_vram: u64,
    pub precision_changes: u64,
}

/// Aggregate field independently recalculated by the ElasticXxx adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmlElasticWeightPlanFieldV1 {
    RamBytes,
    VramBytes,
    ActiveParameters,
    ActiveStorageBits,
    BytesMovedToRam,
    BytesMovedToVram,
    PrecisionChanges,
}

/// Validated SML v1 elastic-weight plan.
///
/// Construction is only possible through Self::validate, which independently
/// recomputes every aggregate accepted from SML.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmlElasticWeightPlanV1 {
    envelope: SmlElasticWeightPlanEnvelopeV1,
}

impl SmlElasticWeightPlanV1 {
    /// Validate source identity, every page transition, all aggregate counters
    /// and the declared resource ceilings.
    pub fn validate(
        envelope: SmlElasticWeightPlanEnvelopeV1,
    ) -> Result<Self, SmlElasticWeightAdapterError> {
        if envelope.contract != SML_ELASTIC_WEIGHT_PLAN_V1 {
            return Err(SmlElasticWeightAdapterError::Contract {
                expected: SML_ELASTIC_WEIGHT_PLAN_V1,
                actual: envelope.contract,
            });
        }
        if envelope.source_commit != SML_ELASTIC_WEIGHT_SOURCE_COMMIT_V1 {
            return Err(SmlElasticWeightAdapterError::SourceCommit {
                expected: SML_ELASTIC_WEIGHT_SOURCE_COMMIT_V1,
                actual: envelope.source_commit,
            });
        }

        let mut page_ids = BTreeSet::new();
        let mut ram_bytes = 0_u64;
        let mut vram_bytes = 0_u64;
        let mut active_parameters = 0_u64;
        let mut active_storage_bits = 0_u128;
        let mut bytes_moved_to_ram = 0_u64;
        let mut bytes_moved_to_vram = 0_u64;
        let mut precision_changes = 0_u64;

        for transition in &envelope.transitions {
            if transition.parameters == 0 {
                return Err(SmlElasticWeightAdapterError::ZeroParameters {
                    page_id: transition.page_id,
                });
            }
            if !page_ids.insert(transition.page_id) {
                return Err(SmlElasticWeightAdapterError::DuplicatePage {
                    page_id: transition.page_id,
                });
            }

            let expected_payload = payload_bytes(transition.parameters, transition.to_precision)?;
            if transition.target_payload_bytes != expected_payload {
                return Err(SmlElasticWeightAdapterError::PayloadBytes {
                    page_id: transition.page_id,
                    expected: expected_payload,
                    actual: transition.target_payload_bytes,
                });
            }

            let expected_version = if transition.from_precision != transition.to_precision {
                transition
                    .from_representation_version
                    .checked_add(1)
                    .ok_or(
                        SmlElasticWeightAdapterError::RepresentationVersionOverflow {
                            page_id: transition.page_id,
                        },
                    )?
            } else {
                transition.from_representation_version
            };
            if transition.to_representation_version != expected_version {
                return Err(SmlElasticWeightAdapterError::RepresentationVersion {
                    page_id: transition.page_id,
                    expected: expected_version,
                    actual: transition.to_representation_version,
                });
            }

            if transition.to_active && transition.to_residency == SmlWeightResidencyV1::Disk {
                return Err(SmlElasticWeightAdapterError::ActivePageOnDisk {
                    page_id: transition.page_id,
                });
            }

            match transition.to_residency {
                SmlWeightResidencyV1::Disk => {}
                SmlWeightResidencyV1::Ram => {
                    ram_bytes = checked_add(ram_bytes, transition.target_payload_bytes)?;
                }
                SmlWeightResidencyV1::Vram => {
                    vram_bytes = checked_add(vram_bytes, transition.target_payload_bytes)?;
                }
            }

            if transition.to_active {
                active_parameters = checked_add(active_parameters, transition.parameters)?;
                active_storage_bits = active_storage_bits
                    .checked_add(
                        transition.parameters as u128
                            * transition.to_precision.bits_per_parameter() as u128,
                    )
                    .ok_or(SmlElasticWeightAdapterError::ArithmeticOverflow)?;
            }

            if transition.from_precision != transition.to_precision {
                precision_changes = checked_add(precision_changes, 1)?;
            }
            if transition.from_residency != transition.to_residency {
                match transition.to_residency {
                    SmlWeightResidencyV1::Disk => {}
                    SmlWeightResidencyV1::Ram => {
                        bytes_moved_to_ram =
                            checked_add(bytes_moved_to_ram, transition.target_payload_bytes)?;
                    }
                    SmlWeightResidencyV1::Vram => {
                        bytes_moved_to_vram =
                            checked_add(bytes_moved_to_vram, transition.target_payload_bytes)?;
                    }
                }
            }
        }

        if ram_bytes > envelope.ram_limit_bytes {
            return Err(SmlElasticWeightAdapterError::RamBudget {
                actual: ram_bytes,
                maximum: envelope.ram_limit_bytes,
            });
        }
        if vram_bytes > envelope.vram_limit_bytes {
            return Err(SmlElasticWeightAdapterError::VramBudget {
                actual: vram_bytes,
                maximum: envelope.vram_limit_bytes,
            });
        }
        if active_parameters > envelope.max_active_parameters {
            return Err(SmlElasticWeightAdapterError::ActiveParameterBudget {
                actual: active_parameters,
                maximum: envelope.max_active_parameters,
            });
        }

        validate_field(
            SmlElasticWeightPlanFieldV1::RamBytes,
            ram_bytes as u128,
            envelope.ram_bytes as u128,
        )?;
        validate_field(
            SmlElasticWeightPlanFieldV1::VramBytes,
            vram_bytes as u128,
            envelope.vram_bytes as u128,
        )?;
        validate_field(
            SmlElasticWeightPlanFieldV1::ActiveParameters,
            active_parameters as u128,
            envelope.active_parameters as u128,
        )?;
        validate_field(
            SmlElasticWeightPlanFieldV1::ActiveStorageBits,
            active_storage_bits,
            envelope.active_storage_bits,
        )?;
        validate_field(
            SmlElasticWeightPlanFieldV1::BytesMovedToRam,
            bytes_moved_to_ram as u128,
            envelope.bytes_moved_to_ram as u128,
        )?;
        validate_field(
            SmlElasticWeightPlanFieldV1::BytesMovedToVram,
            bytes_moved_to_vram as u128,
            envelope.bytes_moved_to_vram as u128,
        )?;
        validate_field(
            SmlElasticWeightPlanFieldV1::PrecisionChanges,
            precision_changes as u128,
            envelope.precision_changes as u128,
        )?;

        Ok(Self { envelope })
    }

    /// Qualified SML source commit carried by this validated plan.
    #[must_use]
    pub fn source_commit(&self) -> &str {
        &self.envelope.source_commit
    }

    /// SML planner generation on which the plan was based.
    #[must_use]
    pub const fn base_generation(&self) -> u64 {
        self.envelope.base_generation
    }

    /// Caller-owned SML planning epoch.
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.envelope.epoch
    }

    /// Validated page transitions.
    #[must_use]
    pub fn transitions(&self) -> &[SmlElasticWeightTransitionV1] {
        &self.envelope.transitions
    }

    /// Validated target RAM payload bytes.
    #[must_use]
    pub const fn ram_bytes(&self) -> u64 {
        self.envelope.ram_bytes
    }

    /// Validated target VRAM payload bytes.
    #[must_use]
    pub const fn vram_bytes(&self) -> u64 {
        self.envelope.vram_bytes
    }

    /// Validated active learned parameter count.
    #[must_use]
    pub const fn active_parameters(&self) -> u64 {
        self.envelope.active_parameters
    }

    /// Validated active semantic storage bits.
    #[must_use]
    pub const fn active_storage_bits(&self) -> u128 {
        self.envelope.active_storage_bits
    }

    /// Number of validated precision-class changes.
    #[must_use]
    pub const fn precision_changes(&self) -> u64 {
        self.envelope.precision_changes
    }
}

fn payload_bytes(
    parameters: u64,
    precision: SmlWeightPrecisionV1,
) -> Result<u64, SmlElasticWeightAdapterError> {
    let bits = (parameters as u128)
        .checked_mul(precision.bits_per_parameter() as u128)
        .ok_or(SmlElasticWeightAdapterError::ArithmeticOverflow)?;
    u64::try_from(bits.div_ceil(8)).map_err(|_| SmlElasticWeightAdapterError::ArithmeticOverflow)
}

fn checked_add(left: u64, right: u64) -> Result<u64, SmlElasticWeightAdapterError> {
    left.checked_add(right)
        .ok_or(SmlElasticWeightAdapterError::ArithmeticOverflow)
}

fn validate_field(
    field: SmlElasticWeightPlanFieldV1,
    expected: u128,
    actual: u128,
) -> Result<(), SmlElasticWeightAdapterError> {
    if expected != actual {
        return Err(SmlElasticWeightAdapterError::Accounting {
            field,
            expected,
            actual,
        });
    }
    Ok(())
}

/// Fail-closed SML elastic-weight boundary validation error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmlElasticWeightAdapterError {
    Contract {
        expected: &'static str,
        actual: String,
    },
    SourceCommit {
        expected: &'static str,
        actual: String,
    },
    ZeroParameters {
        page_id: u32,
    },
    DuplicatePage {
        page_id: u32,
    },
    ActivePageOnDisk {
        page_id: u32,
    },
    PayloadBytes {
        page_id: u32,
        expected: u64,
        actual: u64,
    },
    RepresentationVersionOverflow {
        page_id: u32,
    },
    RepresentationVersion {
        page_id: u32,
        expected: u64,
        actual: u64,
    },
    RamBudget {
        actual: u64,
        maximum: u64,
    },
    VramBudget {
        actual: u64,
        maximum: u64,
    },
    ActiveParameterBudget {
        actual: u64,
        maximum: u64,
    },
    Accounting {
        field: SmlElasticWeightPlanFieldV1,
        expected: u128,
        actual: u128,
    },
    ReinterpretPrecisionChange,
    ArithmeticOverflow,
    Representation(TransitionError),
}

impl fmt::Display for SmlElasticWeightAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SmlElasticWeightAdapterError {}

impl From<TransitionError> for SmlElasticWeightAdapterError {
    fn from(value: TransitionError) -> Self {
        Self::Representation(value)
    }
}
