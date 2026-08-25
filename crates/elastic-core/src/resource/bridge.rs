//! Bridge between the general resource model and the representation layer.
//!
//! A representational resource is a **specialization** of the general model:
//! the declaration says that the `Representation` dimension is elastic, which
//! representation contracts are allowed, and which transition mechanisms are
//! admitted; execution still goes through the existing
//! [`crate::representation`] machinery — [`TargetContract`],
//! [`RepresentationTransition::validate`], and [`VersionFrontier`] — so no
//! invariant check is bypassed.
//!
//! The bridge performs *planning metadata* and *structural validation* only.
//! It never executes a physical re-encoding; physical action remains with the
//! trusted adapter boundary.

use super::spec::ResourceSpec;
use super::terms::DimensionId;
use crate::frontier::{FrontierError, VersionFrontier};
use crate::representation::{
    RepresentationId, RepresentationState, TargetContract, TransitionError, TransitionMechanism,
};
use std::collections::BTreeSet;
use std::fmt;

/// Errors raised while constructing or using a representational declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclarationError {
    /// The `Representation` dimension was not declared elastic, so this
    /// declaration has nothing representational to specialize.
    RepresentationNotElastic,
    /// No target representation contract was declared as allowed.
    NoAllowedContracts,
    /// The requested representation contract is not declared allowed.
    UnsupportedRepresentation {
        /// The rejected contract identity.
        id: RepresentationId,
        /// The requested schema version.
        schema_version: u32,
    },
    /// The requested mechanism is not admitted along the representation
    /// dimension.
    MechanismNotAdmitted {
        /// The rejected mechanism.
        mechanism: TransitionMechanism,
    },
    /// Core transition validation failed (epoch policy, capabilities,
    /// attestations).
    Core(TransitionError),
    /// Frontier control flow failed.
    Frontier(FrontierError),
}

impl fmt::Display for DeclarationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RepresentationNotElastic => write!(
                f,
                "the representation dimension must be declared elastic for a representational resource"
            ),
            Self::NoAllowedContracts => write!(
                f,
                "a representational declaration must allow at least one representation contract"
            ),
            Self::UnsupportedRepresentation { id, schema_version } => write!(
                f,
                "representation {} v{} is not declared as an allowed target",
                id.as_str(),
                schema_version
            ),
            Self::MechanismNotAdmitted { mechanism } => write!(
                f,
                "mechanism {} is not admitted along the representation dimension",
                mechanism_name(*mechanism)
            ),
            Self::Core(error) => write!(f, "{error}"),
            Self::Frontier(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DeclarationError {}

impl From<FrontierError> for DeclarationError {
    fn from(value: FrontierError) -> Self {
        match value {
            FrontierError::Core(core) => Self::Core(core),
            other => Self::Frontier(other),
        }
    }
}

fn mechanism_name(mechanism: TransitionMechanism) -> &'static str {
    match mechanism {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
    }
}

/// A validated representational-resource specialization of
/// [`ResourceSpec`].
///
/// Pairs the general declaration (which dimensions are elastic, which
/// mechanisms are admitted, which capabilities are required) with the explicit
/// set of representation contracts the logical resource may materialize.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationalDeclaration {
    spec: ResourceSpec,
    /// Every `(representation, schema version)` pair the logical resource may
    /// materialize, mirroring the [`crate::CapabilitySet`] keying so multiple
    /// versions of one representation can be declared independently.
    allowed_contracts: BTreeSet<(RepresentationId, u32)>,
}

impl RepresentationalDeclaration {
    /// Specialize a general spec into a representational declaration.
    ///
    /// # Errors
    ///
    /// Returns [`DeclarationError::RepresentationNotElastic`] when the spec
    /// does not declare the `Representation` dimension elastic, and
    /// [`DeclarationError::NoAllowedContracts`] when no target contract is
    /// listed.
    pub fn new(
        spec: ResourceSpec,
        allowed_contracts: impl IntoIterator<Item = (RepresentationId, u32)>,
    ) -> Result<Self, DeclarationError> {
        if !spec.is_elastic(&DimensionId::REPRESENTATION) {
            return Err(DeclarationError::RepresentationNotElastic);
        }
        let mut contracts = BTreeSet::new();
        for (id, schema_version) in allowed_contracts {
            contracts.insert((id, schema_version));
        }
        if contracts.is_empty() {
            return Err(DeclarationError::NoAllowedContracts);
        }
        Ok(Self {
            spec,
            allowed_contracts: contracts,
        })
    }

    /// The underlying general declaration.
    #[must_use]
    pub const fn spec(&self) -> &ResourceSpec {
        &self.spec
    }

    /// Iterate allowed contracts as `(identity, schema version)` pairs.
    pub fn allowed_contracts(&self) -> impl Iterator<Item = (&RepresentationId, u32)> {
        self.allowed_contracts.iter().map(|(id, v)| (id, *v))
    }

    /// Whether this exact materialization is a declared allowed contract.
    #[must_use]
    pub fn supports(&self, state: &RepresentationState) -> bool {
        self.allowed_contracts
            .contains(&(state.id.clone(), state.schema_version))
    }

    /// Whether `mechanism` is admitted along the representation dimension.
    #[must_use]
    pub fn admits_mechanism(&self, mechanism: TransitionMechanism) -> bool {
        self.spec.admits(mechanism, &DimensionId::REPRESENTATION)
    }

    /// Derive the admissible target state for a move to `target`.
    ///
    /// Checks that the contract is allowed and the mechanism admitted, then
    /// delegates epoch policy entirely to
    /// [`RepresentationState::derive_target`].
    ///
    /// # Errors
    ///
    /// Returns structured errors for unsupported contracts, unadmitted
    /// mechanisms, and core epoch-policy failures.
    pub fn derive_target(
        &self,
        current: &RepresentationState,
        target: &RepresentationId,
        schema_version: u32,
        mechanism: TransitionMechanism,
    ) -> Result<RepresentationState, DeclarationError> {
        if !self
            .allowed_contracts
            .contains(&(target.clone(), schema_version))
        {
            return Err(DeclarationError::UnsupportedRepresentation {
                id: target.clone(),
                schema_version,
            });
        }
        if !self.admits_mechanism(mechanism) {
            return Err(DeclarationError::MechanismNotAdmitted { mechanism });
        }
        let contract = TargetContract::New {
            id: target.clone(),
            schema_version,
        };
        current
            .derive_target(contract, mechanism)
            .map_err(DeclarationError::Core)
    }

    /// Propose a transition on `frontier` from its committed state toward
    /// `target`, using this declaration's admission rules.
    ///
    /// Staging does not validate against capabilities; call
    /// [`VersionFrontier::commit`] with a trusted [`crate::CapabilitySet`] and
    /// attestations afterwards. Rollback leaves the committed state untouched.
    ///
    /// # Errors
    ///
    /// Returns structured errors from the declaration checks or the frontier.
    pub fn propose_on(
        &self,
        frontier: &mut VersionFrontier,
        target: &RepresentationId,
        schema_version: u32,
        mechanism: TransitionMechanism,
    ) -> Result<RepresentationState, DeclarationError> {
        let target_state =
            self.derive_target(frontier.committed(), target, schema_version, mechanism)?;
        frontier.propose(target_state.clone(), mechanism)?;
        Ok(target_state)
    }
}
