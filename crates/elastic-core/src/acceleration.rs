//! Runtime diagnostics for architecture-specific Boolean acceleration gates.
//!
//! Detection is descriptive only. A detected CPU feature does not change
//! Boolean semantics, authorize validation/actuation, or automatically enable
//! an implementation path. Specialized execution requires separate measured
//! qualification and must always retain the portable path as fallback.

/// Architecture family relevant to optional Boolean acceleration probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanCpuArchitecture {
    /// 64-bit Arm.
    Aarch64,
    /// 64-bit x86.
    X86_64,
    /// Any architecture without a dedicated diagnostic profile here.
    Other,
}

/// Runtime/compile-target CPU features relevant to future Boolean kernels.
///
/// All fields are diagnostics. `elastic-core` currently keeps the portable
/// evaluator authoritative regardless of these values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BooleanCpuFeatures {
    architecture: BooleanCpuArchitecture,
    compile_time_neon: bool,
    runtime_neon: bool,
    runtime_sve: bool,
    runtime_sve2: bool,
    runtime_popcnt: bool,
    runtime_avx2: bool,
}

impl BooleanCpuFeatures {
    /// Detect the current process CPU features without changing execution paths.
    #[must_use]
    pub fn detect() -> Self {
        let mut features = Self {
            architecture: BooleanCpuArchitecture::Other,
            compile_time_neon: false,
            runtime_neon: false,
            runtime_sve: false,
            runtime_sve2: false,
            runtime_popcnt: false,
            runtime_avx2: false,
        };

        #[cfg(target_arch = "aarch64")]
        {
            features.architecture = BooleanCpuArchitecture::Aarch64;
            features.compile_time_neon = cfg!(target_feature = "neon");
            features.runtime_neon = std::arch::is_aarch64_feature_detected!("neon");
            features.runtime_sve = std::arch::is_aarch64_feature_detected!("sve");
            features.runtime_sve2 = std::arch::is_aarch64_feature_detected!("sve2");
        }

        #[cfg(target_arch = "x86_64")]
        {
            features.architecture = BooleanCpuArchitecture::X86_64;
            features.runtime_popcnt = std::arch::is_x86_feature_detected!("popcnt");
            features.runtime_avx2 = std::arch::is_x86_feature_detected!("avx2");
        }

        features
    }

    /// Architecture family of the current build.
    #[must_use]
    pub const fn architecture(self) -> BooleanCpuArchitecture {
        self.architecture
    }

    /// Whether the compiler target already assumes Arm NEON.
    #[must_use]
    pub const fn compile_time_neon(self) -> bool {
        self.compile_time_neon
    }

    /// Whether the running AArch64 CPU reports NEON/AdvSIMD.
    #[must_use]
    pub const fn runtime_neon(self) -> bool {
        self.runtime_neon
    }

    /// Whether the running AArch64 CPU reports SVE.
    #[must_use]
    pub const fn runtime_sve(self) -> bool {
        self.runtime_sve
    }

    /// Whether the running AArch64 CPU reports SVE2.
    #[must_use]
    pub const fn runtime_sve2(self) -> bool {
        self.runtime_sve2
    }

    /// Whether the running x86-64 CPU reports POPCNT.
    #[must_use]
    pub const fn runtime_popcnt(self) -> bool {
        self.runtime_popcnt
    }

    /// Whether the running x86-64 CPU reports AVX2.
    #[must_use]
    pub const fn runtime_avx2(self) -> bool {
        self.runtime_avx2
    }

    /// Whether this process has any detected vector-capable architecture signal
    /// that may justify a separately benchmarked specialized probe.
    ///
    /// This is intentionally not an execution-policy decision.
    #[must_use]
    pub const fn has_vector_probe_candidate(self) -> bool {
        self.runtime_neon || self.runtime_sve || self.runtime_sve2 || self.runtime_avx2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_is_self_consistent_for_the_compiled_architecture() {
        let features = BooleanCpuFeatures::detect();

        #[cfg(target_arch = "aarch64")]
        {
            assert_eq!(features.architecture(), BooleanCpuArchitecture::Aarch64);
            assert!(!features.runtime_popcnt());
            assert!(!features.runtime_avx2());
            if features.runtime_sve2() {
                assert!(features.runtime_sve());
            }
        }

        #[cfg(target_arch = "x86_64")]
        {
            assert_eq!(features.architecture(), BooleanCpuArchitecture::X86_64);
            assert!(!features.compile_time_neon());
            assert!(!features.runtime_neon());
            assert!(!features.runtime_sve());
            assert!(!features.runtime_sve2());
        }
    }

    #[test]
    fn detection_is_diagnostic_only_and_stable_within_process() {
        assert_eq!(BooleanCpuFeatures::detect(), BooleanCpuFeatures::detect());
    }
}
