//! Verification result types.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerificationResult {
    Pass,
    Fail { detail: String },
    Inconclusive { detail: String },
}

impl VerificationResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }
}
