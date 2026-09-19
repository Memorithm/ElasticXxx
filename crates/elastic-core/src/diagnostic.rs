//! Stable diagnostic identifiers shared by the Elastic language and tooling.
//!
//! Codes are part of the developer-facing compatibility surface. Human text
//! may improve over time; a code keeps the same semantic category once shipped.

use core::fmt;

/// Schema version of the stable Elastic diagnostic-code registry.
pub const ELASTIC_DIAGNOSTIC_SCHEMA_V1: u16 = 1;

/// Stable diagnostic categories emitted by the embedded language/tooling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ElasticDiagnosticCode {
    /// A language declaration references a resource/target that does not exist.
    LanguageUnknownReference,
    /// A declaration/alias collides with another declaration in the same scope.
    LanguageDuplicateDeclaration,
    /// A guard/constraint uses a predicate alias that was never declared.
    LanguageUndeclaredPredicate,
    /// A required field of a language declaration is absent.
    LanguageMissingRequiredField,
    /// The declaration shape is unsupported or malformed.
    LanguageMalformedDeclaration,
    /// A guard can never evaluate explicitly `True` under strong-Kleene semantics.
    AnalysisDeadGuard,
    /// A guard evaluates explicitly `True` for every strong-Kleene assignment.
    AnalysisTautologicalGuard,
    /// Two guard expressions are exactly equivalent including `Unknown`.
    AnalysisEquivalentGuards,
    /// One guard's explicit-True eligibility implies another guard is explicitly True.
    AnalysisGuardImplication,
    /// Two guards can never both evaluate explicitly `True`.
    AnalysisMutuallyExclusiveGuards,
}

impl ElasticDiagnosticCode {
    /// Stable machine-readable code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LanguageUnknownReference => "ELX-LANG-0001",
            Self::LanguageDuplicateDeclaration => "ELX-LANG-0002",
            Self::LanguageUndeclaredPredicate => "ELX-LANG-0003",
            Self::LanguageMissingRequiredField => "ELX-LANG-0004",
            Self::LanguageMalformedDeclaration => "ELX-LANG-0005",
            Self::AnalysisDeadGuard => "ELX-ANALYZE-0001",
            Self::AnalysisTautologicalGuard => "ELX-ANALYZE-0002",
            Self::AnalysisEquivalentGuards => "ELX-ANALYZE-0003",
            Self::AnalysisGuardImplication => "ELX-ANALYZE-0004",
            Self::AnalysisMutuallyExclusiveGuards => "ELX-ANALYZE-0005",
        }
    }
}

impl fmt::Display for ElasticDiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn stable_codes_are_unique_and_well_formed() {
        let codes = [
            ElasticDiagnosticCode::LanguageUnknownReference,
            ElasticDiagnosticCode::LanguageDuplicateDeclaration,
            ElasticDiagnosticCode::LanguageUndeclaredPredicate,
            ElasticDiagnosticCode::LanguageMissingRequiredField,
            ElasticDiagnosticCode::LanguageMalformedDeclaration,
            ElasticDiagnosticCode::AnalysisDeadGuard,
            ElasticDiagnosticCode::AnalysisTautologicalGuard,
            ElasticDiagnosticCode::AnalysisEquivalentGuards,
            ElasticDiagnosticCode::AnalysisGuardImplication,
            ElasticDiagnosticCode::AnalysisMutuallyExclusiveGuards,
        ];
        let unique = codes
            .iter()
            .map(|code| code.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), codes.len());
        assert!(unique
            .iter()
            .all(|code| code.starts_with("ELX-") && code.len() <= 64));
    }
}

#[cfg(test)]
mod macro_contract_tests {
    use super::*;

    #[test]
    fn proc_macro_private_codes_match_public_registry() {
        let macros = include_str!("../../elastic-macros/src/lib.rs");
        let public = [
            ElasticDiagnosticCode::LanguageUnknownReference,
            ElasticDiagnosticCode::LanguageDuplicateDeclaration,
            ElasticDiagnosticCode::LanguageUndeclaredPredicate,
            ElasticDiagnosticCode::LanguageMissingRequiredField,
            ElasticDiagnosticCode::LanguageMalformedDeclaration,
        ];
        for code in public {
            let needle = format!("=> \"{}\"", code.as_str());
            assert!(
                macros.contains(&needle),
                "proc-macro diagnostic registry drifted: missing {}",
                code.as_str()
            );
        }
    }
}
