//! Elastic multi-lane word representation for control-plane metadata.
//!
//! The width is a property of a contiguous plane/epoch, not a tag carried by
//! every hot descriptor. Storage is always expressed as native `u64` lanes;
//! the currently supported widths are 1, 2, 4, 8, 16 or 32 lanes
//! (64 through 2048 bits).
//!
//! This module owns structural representation rules only. It does not assign KV
//! semantics to individual bits, does not claim that widths above 128 bits are
//! native scalar integers, and does not authorize physical cache mutation.
//! Domain adapters remain responsible for proving that a width transition
//! preserves their semantic invariants.

use elastic_core::TransitionMechanism;
use std::fmt;

/// Versioned identity of the elastic multi-lane word contract.
pub const ELASTIC_WORD_WIDTH_V1: &str = "elastic.word-width@1.0.0";

/// Native backing-lane width used by the representation.
pub const ELASTIC_WORD_LANE_BITS: u16 = 64;

/// Smallest admitted plane width in native lanes.
pub const ELASTIC_WORD_MIN_LANES: u8 = 1;

/// Largest admitted plane width in native lanes.
pub const ELASTIC_WORD_MAX_LANES: u8 = 32;

/// Validated width of one logical word in a contiguous elastic plane.
///
/// The admitted values are powers of two in `1..=32`, giving the current
/// research envelope `64, 128, 256, 512, 1024, 2048` bits. Keeping the width
/// at plane/epoch scope avoids an enum/tag branch on every descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElasticWordWidthV1 {
    lanes: u8,
}

impl ElasticWordWidthV1 {
    /// Validate a width expressed in native `u64` lanes.
    pub fn from_lanes(lanes: u8) -> Result<Self, ElasticWordError> {
        if !(ELASTIC_WORD_MIN_LANES..=ELASTIC_WORD_MAX_LANES).contains(&lanes)
            || !lanes.is_power_of_two()
        {
            return Err(ElasticWordError::UnsupportedLaneCount { lanes });
        }
        Ok(Self { lanes })
    }

    /// Validate a width expressed in bits.
    pub fn from_bits(bits: u16) -> Result<Self, ElasticWordError> {
        if bits == 0 || bits % ELASTIC_WORD_LANE_BITS != 0 {
            return Err(ElasticWordError::UnsupportedBitWidth { bits });
        }

        let lanes = bits / ELASTIC_WORD_LANE_BITS;
        let lanes =
            u8::try_from(lanes).map_err(|_| ElasticWordError::UnsupportedBitWidth { bits })?;
        Self::from_lanes(lanes).map_err(|_| ElasticWordError::UnsupportedBitWidth { bits })
    }

    /// Number of native `u64` lanes in one logical word.
    #[must_use]
    pub const fn lanes(self) -> u8 {
        self.lanes
    }

    /// Width in bits.
    #[must_use]
    pub const fn bits(self) -> u16 {
        (self.lanes as u16) * ELASTIC_WORD_LANE_BITS
    }

    /// Width in bytes.
    #[must_use]
    pub const fn bytes(self) -> u16 {
        self.bits() / 8
    }
}

impl fmt::Display for ElasticWordWidthV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} bits ({}x u64)", self.bits(), self.lanes)
    }
}

/// Structural description of a width transition for one contiguous plane.
///
/// This is planning evidence only. A caller still has to validate domain
/// semantics and execute the transition through its trusted transaction path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElasticWordWidthTransitionV1 {
    from: ElasticWordWidthV1,
    to: ElasticWordWidthV1,
    mechanism: TransitionMechanism,
    word_count: usize,
}

impl ElasticWordWidthTransitionV1 {
    /// Source width.
    #[must_use]
    pub const fn from(self) -> ElasticWordWidthV1 {
        self.from
    }

    /// Target width.
    #[must_use]
    pub const fn to(self) -> ElasticWordWidthV1 {
        self.to
    }

    /// Declared representation transition mechanism.
    #[must_use]
    pub const fn mechanism(self) -> TransitionMechanism {
        self.mechanism
    }

    /// Number of logical words covered by this transition.
    #[must_use]
    pub const fn word_count(self) -> usize {
        self.word_count
    }

    /// Whether the target uses more lanes per word.
    #[must_use]
    pub const fn is_expansion(self) -> bool {
        self.to.lanes > self.from.lanes
    }

    /// Whether the target uses fewer lanes per word.
    #[must_use]
    pub const fn is_contraction(self) -> bool {
        self.to.lanes < self.from.lanes
    }
}

/// Contiguous multi-lane backing plane.
///
/// The width is stored once for the whole plane. The hot payload remains a flat
/// `Vec<u64>` with no per-word enum or width tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElasticWordPlaneV1 {
    width: ElasticWordWidthV1,
    lanes: Vec<u64>,
}

impl ElasticWordPlaneV1 {
    /// Build a plane from an already-materialized flat lane buffer.
    pub fn new(width: ElasticWordWidthV1, lanes: Vec<u64>) -> Result<Self, ElasticWordError> {
        let lanes_per_word = usize::from(width.lanes());
        if lanes.len() % lanes_per_word != 0 {
            return Err(ElasticWordError::MisalignedStorageLength {
                lane_count: lanes.len(),
                lanes_per_word: width.lanes(),
            });
        }
        Ok(Self { width, lanes })
    }

    /// Allocate a zero-filled plane.
    pub fn zeroed(width: ElasticWordWidthV1, word_count: usize) -> Result<Self, ElasticWordError> {
        let lane_count = word_count
            .checked_mul(usize::from(width.lanes()))
            .ok_or(ElasticWordError::StorageLengthOverflow)?;
        Ok(Self {
            width,
            lanes: vec![0; lane_count],
        })
    }

    /// Current plane width.
    #[must_use]
    pub const fn width(&self) -> ElasticWordWidthV1 {
        self.width
    }

    /// Number of logical words in the plane.
    #[must_use]
    pub fn word_count(&self) -> usize {
        self.lanes.len() / usize::from(self.width.lanes())
    }

    /// Flat native-lane storage.
    #[must_use]
    pub fn as_lanes(&self) -> &[u64] {
        &self.lanes
    }

    /// Mutable flat native-lane storage.
    #[must_use]
    pub fn as_lanes_mut(&mut self) -> &mut [u64] {
        &mut self.lanes
    }

    /// Borrow one logical word as its native lanes.
    pub fn word(&self, index: usize) -> Result<&[u64], ElasticWordError> {
        let lanes_per_word = usize::from(self.width.lanes());
        let start =
            index
                .checked_mul(lanes_per_word)
                .ok_or(ElasticWordError::WordIndexOutOfBounds {
                    index,
                    word_count: self.word_count(),
                })?;
        let end =
            start
                .checked_add(lanes_per_word)
                .ok_or(ElasticWordError::WordIndexOutOfBounds {
                    index,
                    word_count: self.word_count(),
                })?;
        self.lanes
            .get(start..end)
            .ok_or(ElasticWordError::WordIndexOutOfBounds {
                index,
                word_count: self.word_count(),
            })
    }

    /// Plan a structural width transition.
    ///
    /// Width changes cannot be represented as a byte reinterpretation because
    /// they change the materialized stride of every logical word.
    pub fn plan_width_transition(
        &self,
        target: ElasticWordWidthV1,
        mechanism: TransitionMechanism,
    ) -> Result<ElasticWordWidthTransitionV1, ElasticWordError> {
        if target != self.width && matches!(mechanism, TransitionMechanism::Reinterpret) {
            return Err(ElasticWordError::WidthChangeRequiresMaterialization {
                from_bits: self.width.bits(),
                to_bits: target.bits(),
            });
        }

        Ok(ElasticWordWidthTransitionV1 {
            from: self.width,
            to: target,
            mechanism,
            word_count: self.word_count(),
        })
    }

    /// Reference repacker using low-lane preservation and zero extension.
    ///
    /// This helper is deliberately stricter than a domain-specific codec:
    /// expansion zero-fills new high lanes; contraction is accepted only when
    /// every discarded high lane is already zero. It is useful as a
    /// deterministic structural oracle, but does not prove that an arbitrary KV
    /// or model representation may be narrowed safely.
    pub fn reference_repack_zero_extended(
        &self,
        target: ElasticWordWidthV1,
    ) -> Result<Self, ElasticWordError> {
        if target == self.width {
            return Ok(self.clone());
        }

        let source_lanes = usize::from(self.width.lanes());
        let target_lanes = usize::from(target.lanes());
        let word_count = self.word_count();
        let target_lane_count = word_count
            .checked_mul(target_lanes)
            .ok_or(ElasticWordError::StorageLengthOverflow)?;
        let mut output = vec![0_u64; target_lane_count];

        for word_index in 0..word_count {
            let source_start = word_index * source_lanes;
            let target_start = word_index * target_lanes;
            let common = source_lanes.min(target_lanes);

            output[target_start..target_start + common]
                .copy_from_slice(&self.lanes[source_start..source_start + common]);

            if target_lanes < source_lanes {
                if let Some((relative_lane, value)) = self.lanes
                    [source_start + target_lanes..source_start + source_lanes]
                    .iter()
                    .copied()
                    .enumerate()
                    .find(|(_, value)| *value != 0)
                {
                    return Err(ElasticWordError::NarrowingWouldDiscardData {
                        word_index,
                        lane_index: target_lanes + relative_lane,
                        value,
                    });
                }
            }
        }

        Self::new(target, output)
    }
}

/// Fail-closed errors for elastic multi-lane word representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElasticWordError {
    /// Lane count is outside the admitted power-of-two envelope.
    UnsupportedLaneCount { lanes: u8 },
    /// Bit width is not one of the admitted lane-aligned widths.
    UnsupportedBitWidth { bits: u16 },
    /// Flat storage length is not divisible by the selected plane width.
    MisalignedStorageLength {
        lane_count: usize,
        lanes_per_word: u8,
    },
    /// Requested allocation or repack size overflowed `usize`.
    StorageLengthOverflow,
    /// Logical word index is outside the plane.
    WordIndexOutOfBounds { index: usize, word_count: usize },
    /// A stride-changing width transition was incorrectly requested as reinterpretation.
    WidthChangeRequiresMaterialization { from_bits: u16, to_bits: u16 },
    /// Reference contraction would drop a non-zero high lane.
    NarrowingWouldDiscardData {
        word_index: usize,
        lane_index: usize,
        value: u64,
    },
}

impl fmt::Display for ElasticWordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedLaneCount { lanes } => write!(
                f,
                "elastic word width requires a power-of-two lane count in {ELASTIC_WORD_MIN_LANES}..={ELASTIC_WORD_MAX_LANES}, observed {lanes}"
            ),
            Self::UnsupportedBitWidth { bits } => write!(
                f,
                "elastic word width requires one of 64, 128, 256, 512, 1024 or 2048 bits, observed {bits}"
            ),
            Self::MisalignedStorageLength {
                lane_count,
                lanes_per_word,
            } => write!(
                f,
                "elastic word plane has {lane_count} u64 lanes, not divisible by {lanes_per_word} lanes/word"
            ),
            Self::StorageLengthOverflow => {
                f.write_str("elastic word storage length overflowed usize")
            }
            Self::WordIndexOutOfBounds { index, word_count } => write!(
                f,
                "elastic word index {index} is out of bounds for {word_count} words"
            ),
            Self::WidthChangeRequiresMaterialization { from_bits, to_bits } => write!(
                f,
                "elastic word width change {from_bits}->{to_bits} bits changes physical stride and requires reencode/recompute rather than reinterpret"
            ),
            Self::NarrowingWouldDiscardData {
                word_index,
                lane_index,
                value,
            } => write!(
                f,
                "elastic word narrowing would discard non-zero word {word_index} lane {lane_index} value {value:#x}"
            ),
        }
    }
}

impl std::error::Error for ElasticWordError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admitted_widths_cover_64_through_2048_bits() {
        let widths = [64_u16, 128, 256, 512, 1024, 2048];
        for (index, bits) in widths.into_iter().enumerate() {
            let width = ElasticWordWidthV1::from_bits(bits).unwrap();
            assert_eq!(width.bits(), bits);
            assert_eq!(usize::from(width.lanes()), 1_usize << index);
            assert_eq!(width.bytes(), bits / 8);
        }
    }

    #[test]
    fn invalid_widths_fail_closed() {
        for bits in [0_u16, 32, 192, 384, 4096] {
            assert!(matches!(
                ElasticWordWidthV1::from_bits(bits),
                Err(ElasticWordError::UnsupportedBitWidth { bits: observed }) if observed == bits
            ));
        }
        for lanes in [0_u8, 3, 6, 64] {
            assert!(matches!(
                ElasticWordWidthV1::from_lanes(lanes),
                Err(ElasticWordError::UnsupportedLaneCount { lanes: observed }) if observed == lanes
            ));
        }
    }

    #[test]
    fn plane_keeps_width_once_and_words_contiguous() {
        let width = ElasticWordWidthV1::from_bits(256).unwrap();
        let plane = ElasticWordPlaneV1::new(width, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();

        assert_eq!(plane.word_count(), 2);
        assert_eq!(plane.word(0).unwrap(), &[1, 2, 3, 4]);
        assert_eq!(plane.word(1).unwrap(), &[5, 6, 7, 8]);
        assert_eq!(plane.as_lanes(), &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn misaligned_plane_is_rejected() {
        let width = ElasticWordWidthV1::from_bits(256).unwrap();
        assert!(matches!(
            ElasticWordPlaneV1::new(width, vec![1, 2, 3]),
            Err(ElasticWordError::MisalignedStorageLength {
                lane_count: 3,
                lanes_per_word: 4
            })
        ));
    }

    #[test]
    fn expansion_reference_repack_zero_extends_each_word() {
        let w64 = ElasticWordWidthV1::from_bits(64).unwrap();
        let w256 = ElasticWordWidthV1::from_bits(256).unwrap();
        let plane = ElasticWordPlaneV1::new(w64, vec![0x11, 0x22]).unwrap();

        let expanded = plane.reference_repack_zero_extended(w256).unwrap();
        assert_eq!(expanded.word_count(), 2);
        assert_eq!(expanded.word(0).unwrap(), &[0x11, 0, 0, 0]);
        assert_eq!(expanded.word(1).unwrap(), &[0x22, 0, 0, 0]);
    }

    #[test]
    fn contraction_reference_repack_accepts_only_zero_high_lanes() {
        let w256 = ElasticWordWidthV1::from_bits(256).unwrap();
        let w64 = ElasticWordWidthV1::from_bits(64).unwrap();
        let plane = ElasticWordPlaneV1::new(w256, vec![0x11, 0, 0, 0, 0x22, 0, 0, 0]).unwrap();

        let narrowed = plane.reference_repack_zero_extended(w64).unwrap();
        assert_eq!(narrowed.as_lanes(), &[0x11, 0x22]);

        let lossy = ElasticWordPlaneV1::new(w256, vec![0x11, 0x99, 0, 0]).unwrap();
        assert!(matches!(
            lossy.reference_repack_zero_extended(w64),
            Err(ElasticWordError::NarrowingWouldDiscardData {
                word_index: 0,
                lane_index: 1,
                value: 0x99
            })
        ));
    }

    #[test]
    fn width_change_cannot_be_reinterpreted() {
        let w128 = ElasticWordWidthV1::from_bits(128).unwrap();
        let w512 = ElasticWordWidthV1::from_bits(512).unwrap();
        let plane = ElasticWordPlaneV1::zeroed(w128, 4).unwrap();

        assert!(matches!(
            plane.plan_width_transition(w512, TransitionMechanism::Reinterpret),
            Err(ElasticWordError::WidthChangeRequiresMaterialization {
                from_bits: 128,
                to_bits: 512
            })
        ));

        let plan = plane
            .plan_width_transition(w512, TransitionMechanism::Reencode)
            .unwrap();
        assert!(plan.is_expansion());
        assert!(!plan.is_contraction());
        assert_eq!(plan.word_count(), 4);
    }
}
