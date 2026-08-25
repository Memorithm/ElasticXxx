//! General typed model for elastic resources.
//!
//! This module is the first layer of the Elastic *Rust Surface Model*: a
//! resource-agnostic vocabulary with which ordinary Rust code declares
//!
//! - what the resource logically is ([`LogicalResourceId`],
//!   [`ResourceClassId`]);
//! - which properties may change ([`DimensionId`], via
//!   [`ResourceSpecBuilder::allow`]);
//! - which transitions are admissible ([`AdmissibleTransition`]);
//! - which properties must remain true ([`Invariant`], via
//!   [`ResourceSpecBuilder::preserve`]) — constraints, never objectives;
//! - what the runtime may try to improve ([`ObjectiveId`], via
//!   [`ResourceSpecBuilder::optimize`]);
//! - which trusted capabilities are required ([`CapabilityRequirement`]);
//! - which observations may inform adaptation
//!   ([`ObservationSignalId`]).
//!
//! # Extensibility balance
//!
//! Built-in semantics are enum variants, so core validation never compares
//! semantic strings. The sets are nevertheless open: every term type offers a
//! validated `custom` constructor, letting downstream/resource-specific crates
//! add dimensions, objectives, classes, and signals without changing this
//! crate and without a closed-world "all future hardware" enum.
//!
//! # What declarations do not do
//!
//! A declaration is intent. It does not execute transitions, does not prove
//! capabilities (requirements are recorded; the trusted runtime discovers the
//! actual capability snapshot), and does not plan. Planning interfaces live in
//! later layers and may return candidates, no candidate, insufficient
//! evidence, or unsupported outcomes.
//!
//! # Determinism
//!
//! All unordered collections normalize to sorted order at build time;
//! objectives intentionally preserve declared priority order. Equal
//! declarations therefore compare equal and iterate identically regardless of
//! construction order.

mod bridge;
pub mod error;
mod invariant;
mod spec;
mod terms;
mod transition;

pub use bridge::{DeclarationError, RepresentationalDeclaration};

pub use error::{ResourceSpecError, TermKind};
pub use invariant::{Invariant, InvariantKind};
pub use spec::{ResourceSpec, ResourceSpecBuilder};
pub use terms::{
    BuiltinDimension, BuiltinObjective, BuiltinObservationSignal, BuiltinResourceClass, ContractId,
    DimensionId, LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId,
};
pub use transition::{AdmissibleTransition, CapabilityRequirement};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_terms_order_by_declaration_then_customs_lexicographically() {
        let mut terms = [
            DimensionId::custom("zeta").unwrap(),
            DimensionId::CAPACITY,
            DimensionId::custom("alpha").unwrap(),
            DimensionId::ENERGY,
        ];
        terms.sort();
        assert_eq!(
            terms.iter().map(DimensionId::as_str).collect::<Vec<_>>(),
            vec!["capacity", "energy", "alpha", "zeta"]
        );
    }

    #[test]
    fn custom_terms_reject_blank_text() {
        for blank in ["", "   "] {
            assert_eq!(
                DimensionId::custom(blank),
                Err(ResourceSpecError::InvalidCustomTerm {
                    term_kind: TermKind::Dimension
                })
            );
            assert_eq!(
                ObjectiveId::custom(blank),
                Err(ResourceSpecError::InvalidCustomTerm {
                    term_kind: TermKind::Objective
                })
            );
            assert_eq!(
                ResourceClassId::custom(blank),
                Err(ResourceSpecError::InvalidCustomTerm {
                    term_kind: TermKind::ResourceClass
                })
            );
            assert_eq!(
                ObservationSignalId::custom(blank),
                Err(ResourceSpecError::InvalidCustomTerm {
                    term_kind: TermKind::ObservationSignal
                })
            );
            assert_eq!(
                ContractId::new(blank),
                Err(ResourceSpecError::InvalidCustomTerm {
                    term_kind: TermKind::Contract
                })
            );
            assert_eq!(
                LogicalResourceId::new(blank),
                Err(ResourceSpecError::EmptyResourceId)
            );
        }
    }

    #[test]
    fn custom_terms_do_not_shadow_built_ins() {
        let impostor = DimensionId::custom("capacity").unwrap();
        assert_eq!(impostor.builtin_part(), None);
        assert_ne!(impostor, DimensionId::CAPACITY);
        // Customs always sort after built-ins even with identical text.
        assert!(DimensionId::CAPACITY < impostor);
    }
}
