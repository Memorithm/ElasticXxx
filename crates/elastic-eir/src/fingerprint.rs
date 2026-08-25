//! Structural fingerprints over canonically ordered EIR content.
//!
//! FNV-1a 64-bit absorption with explicit separators, mirroring the
//! fingerprint discipline already established in `elastic-core`. These are
//! **structural fingerprints**: stable within a process and across processes
//! for identical content, suitable for equality checks, caching, and tests.
//! They are not cryptographic, not collision-resistant by design, and must
//! never authenticate anything across trust domains.

use std::fmt;

/// A non-cryptographic structural fingerprint of EIR content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(u64);

impl Fingerprint {
    /// Empty-fingerprint seed.
    pub const EMPTY: Self = Self(0xcbf2_9ce4_8422_2325);

    /// Absorb one string field.
    #[must_use]
    pub fn text(mut self, text: &str) -> Self {
        self = self.separator();
        for byte in text.as_bytes() {
            self.absorb_byte(*byte);
        }
        self
    }

    /// Absorb one unsigned integer field.
    #[must_use]
    pub fn number(mut self, value: u64) -> Self {
        self = self.separator();
        self.absorb_byte(value as u8);
        self.absorb_byte((value >> 8) as u8);
        self.absorb_byte((value >> 16) as u8);
        self.absorb_byte((value >> 24) as u8);
        self.absorb_byte((value >> 32) as u8);
        self.absorb_byte((value >> 40) as u8);
        self.absorb_byte((value >> 48) as u8);
        self.absorb_byte((value >> 56) as u8);
        self
    }

    fn separator(self) -> Self {
        self.text_field(b"/")
    }

    fn text_field(mut self, bytes: &[u8]) -> Self {
        for byte in bytes {
            self.absorb_byte(*byte);
        }
        self
    }

    fn absorb_byte(&mut self, byte: u8) {
        self.0 ^= u64::from(byte);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }

    /// Raw fingerprint bits (diagnostics only).
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fp:{:016x}", self.0)
    }
}
