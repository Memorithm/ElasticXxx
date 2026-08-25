//! Structural fingerprints over canonically ordered EIR content.
//!
//! FNV-1a absorption with explicit field framing, mirroring the fingerprint
//! discipline already established in `elastic-core`. Every field is framed as
//! `[tag][length][payload]`:
//!
//! - text: tag `b's'`, payload length as `u64` little-endian, UTF-8 bytes;
//! - numbers: tag `b'n'`, fixed 8-byte little-endian value.
//!
//! The length prefix makes field boundaries unambiguous, so different
//! field sequences cannot collide by concatenation (e.g. `["a/b", ""]` vs
//! `["a", "/b"]`). These are **structural fingerprints**: stable within a
//! process and across processes for identical content, suitable for equality
//! checks, caching, and tests inside one trust domain. They are not
//! cryptographic, not collision-resistant by design, and must never
//! authenticate anything across trust domains.

use std::fmt;

/// A non-cryptographic structural fingerprint of EIR content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(u64);

impl Fingerprint {
    /// Empty-fingerprint seed.
    pub const EMPTY: Self = Self(0xcbf2_9ce4_8422_2325);

    /// Absorb one text field, framed as `[b's'][len u64 LE][bytes]`.
    #[must_use]
    pub fn text(mut self, text: &str) -> Self {
        self = self.field_tag(b's');
        self = self.fixed(&u64::to_le_bytes(text.len() as u64));
        for byte in text.as_bytes() {
            self.absorb_byte(*byte);
        }
        self
    }

    /// Absorb one unsigned integer field, framed as `[b'n'][value u64 LE]`.
    #[must_use]
    pub fn number(mut self, value: u64) -> Self {
        self = self.field_tag(b'n');
        self.fixed(&value.to_le_bytes())
    }

    fn field_tag(mut self, tag: u8) -> Self {
        self.absorb_byte(tag);
        self
    }

    fn fixed(mut self, bytes: &[u8; 8]) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_framing_is_unambiguous() {
        // Naive separator schemes collide on these pairs; length framing must
        // not.
        let split = Fingerprint::EMPTY.text("a/b").text("");
        let joined = Fingerprint::EMPTY.text("a").text("/b");
        assert_ne!(split, joined);

        // Number payloads do not alias with equal-length text.
        let number = Fingerprint::EMPTY.number(0x2f62_2f61);
        let text = Fingerprint::EMPTY.text("/b/a");
        assert_ne!(number, text);

        // Empty text is a distinct, well-framed field, not a no-op.
        assert_ne!(Fingerprint::EMPTY.text(""), Fingerprint::EMPTY);
        assert_ne!(
            Fingerprint::EMPTY.text("").text("x"),
            Fingerprint::EMPTY.text("x")
        );
    }
}
