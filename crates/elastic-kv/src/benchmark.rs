//! Versioned fixed-candidate benchmark protocol for representation accounting.
//!
//! This module is deliberately a benchmark harness, not an allocator.  It
//! supplies one deterministic synthetic corpus and a closed candidate list so
//! that later elastic allocation work cannot change the denominator or choose
//! a baseline after seeing its own results.  The encoders are small reference
//! implementations whose only purpose is to make payload, metadata, padding,
//! and reconstruction accounting executable.

use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::fmt::Write;

/// Stable identifier of this benchmark protocol.
pub const BENCHMARK_PROTOCOL_VERSION: &str = "elastic-bit-allocation-v1";

/// Seed used by [`SyntheticCorpus::deterministic`].
pub const SYNTHETIC_CORPUS_SEED: u64 = 0x454c_4153_5449_4301;

/// Number of blocks in the frozen synthetic corpus.
pub const SYNTHETIC_BLOCK_COUNT: usize = 16;

/// Number of rows in each frozen synthetic block.
pub const SYNTHETIC_BLOCK_ROWS: usize = 4;

/// Number of columns in each frozen synthetic block.
pub const SYNTHETIC_BLOCK_COLUMNS: usize = 8;

/// Serialized streams are rounded to complete bytes.
pub const SERIALIZED_ALIGNMENT_BITS: u64 = 8;

/// Resident allocations are rounded to this named 256-bit unit.
pub const RESIDENT_ALIGNMENT_BITS: u64 = 256;
const CODEBOOK: [f32; 8] = [
    -1.0,
    -0.714_285_7,
    -0.428_571_43,
    -0.142_857_15,
    0.142_857_15,
    0.428_571_43,
    0.714_285_7,
    1.0,
];

/// Fixed candidates evaluated by Stage A.
pub const FIXED_CANDIDATES: &[CandidateId] = &[
    CandidateId::DenseF32,
    CandidateId::DenseBf16,
    CandidateId::DenseF16,
    CandidateId::FixedInt4,
    CandidateId::FixedInt2,
    CandidateId::SparseF16,
    CandidateId::LowRankF16,
    CandidateId::VectorCodebook8,
    CandidateId::Int4ResidualF16,
];

/// Representation family used by one fixed candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RepresentationFamily {
    /// Dense floating-point storage.
    Dense,
    /// Fixed-width scalar quantization.
    FixedQuantized,
    /// Explicit values plus indices for omitted values.
    Sparse,
    /// A fixed rank-one factorization.
    LowRank,
    /// Fixed vector quantization with a shared codebook.
    Codebook,
    /// A fixed 4-bit base plus selected half-precision residuals.
    HeterogeneousResidual,
}

impl RepresentationFamily {
    /// Stable protocol spelling.
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Dense => "dense",
            Self::FixedQuantized => "fixed-quantized",
            Self::Sparse => "sparse",
            Self::LowRank => "low-rank",
            Self::Codebook => "codebook",
            Self::HeterogeneousResidual => "heterogeneous-residual",
        }
    }
}

/// One representation candidate in the closed Stage-A set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateId {
    /// Dense IEEE binary32 values.
    DenseF32,
    /// Dense brain floating-point 16 values.
    DenseBf16,
    /// Dense IEEE binary16 values.
    DenseF16,
    /// Per-block symmetric fixed 4-bit quantization.
    FixedInt4,
    /// Per-block symmetric fixed 2-bit quantization.
    FixedInt2,
    /// Values above a fixed half-maximum threshold, stored as F16 with indices.
    SparseF16,
    /// Fixed rank-one factors, stored as F16.
    LowRankF16,
    /// Eight-entry fixed scalar codebook with 3-bit indices.
    VectorCodebook8,
    /// Fixed Int4 base with F16 residuals selected by a fixed error threshold.
    Int4ResidualF16,
}

impl CandidateId {
    /// Stable protocol spelling.
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::DenseF32 => "dense-f32",
            Self::DenseBf16 => "dense-bf16",
            Self::DenseF16 => "dense-f16",
            Self::FixedInt4 => "fixed-int4",
            Self::FixedInt2 => "fixed-int2",
            Self::SparseF16 => "sparse-f16",
            Self::LowRankF16 => "low-rank-f16",
            Self::VectorCodebook8 => "vector-codebook-8",
            Self::Int4ResidualF16 => "int4-residual-f16",
        }
    }

    /// Family represented by this candidate.
    pub const fn family(self) -> RepresentationFamily {
        match self {
            Self::DenseF32 | Self::DenseBf16 | Self::DenseF16 => RepresentationFamily::Dense,
            Self::FixedInt4 | Self::FixedInt2 => RepresentationFamily::FixedQuantized,
            Self::SparseF16 => RepresentationFamily::Sparse,
            Self::LowRankF16 => RepresentationFamily::LowRank,
            Self::VectorCodebook8 => RepresentationFamily::Codebook,
            Self::Int4ResidualF16 => RepresentationFamily::HeterogeneousResidual,
        }
    }
}

/// Errors raised while constructing or evaluating the frozen protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BenchmarkError {
    /// A corpus must contain at least one block.
    EmptyCorpus,
    /// A block does not match the declared rectangular shape.
    InvalidBlockShape {
        /// Block ordinal.
        block: usize,
        /// Expected number of values.
        expected: usize,
        /// Actual number of values.
        actual: usize,
    },
    /// A corpus value is not finite and cannot be a canonical logical value.
    NonFiniteValue {
        /// Block ordinal.
        block: usize,
        /// Value ordinal within the block.
        value: usize,
    },
    /// A corpus block shape cannot have zero rows or columns.
    InvalidBlockDimensions,
    /// A checked bit-count operation overflowed.
    ArithmeticOverflow,
}

impl fmt::Display for BenchmarkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCorpus => formatter.write_str("benchmark corpus must not be empty"),
            Self::InvalidBlockShape {
                block,
                expected,
                actual,
            } => write!(
                formatter,
                "benchmark block {block} has {actual} values; expected {expected}"
            ),
            Self::NonFiniteValue { block, value } => {
                write!(
                    formatter,
                    "benchmark block {block} value {value} is not finite"
                )
            }
            Self::InvalidBlockDimensions => {
                formatter.write_str("benchmark block dimensions must be non-zero")
            }
            Self::ArithmeticOverflow => formatter.write_str("benchmark bit count overflowed"),
        }
    }
}

impl Error for BenchmarkError {}

/// Deterministic tensor/block corpus used by the frozen Stage-A protocol.
#[derive(Clone, Debug, PartialEq)]
pub struct SyntheticCorpus {
    seed: u64,
    rows: usize,
    columns: usize,
    blocks: Vec<Vec<f32>>,
}

impl SyntheticCorpus {
    /// Validate and construct a corpus from explicit blocks.
    pub fn new(
        seed: u64,
        rows: usize,
        columns: usize,
        blocks: Vec<Vec<f32>>,
    ) -> Result<Self, BenchmarkError> {
        if blocks.is_empty() {
            return Err(BenchmarkError::EmptyCorpus);
        }
        if rows == 0 || columns == 0 {
            return Err(BenchmarkError::InvalidBlockDimensions);
        }
        let expected = rows
            .checked_mul(columns)
            .ok_or(BenchmarkError::ArithmeticOverflow)?;
        for (block, values) in blocks.iter().enumerate() {
            if values.len() != expected {
                return Err(BenchmarkError::InvalidBlockShape {
                    block,
                    expected,
                    actual: values.len(),
                });
            }
            for (value, sample) in values.iter().enumerate() {
                if !sample.is_finite() {
                    return Err(BenchmarkError::NonFiniteValue { block, value });
                }
            }
        }
        Ok(Self {
            seed,
            rows,
            columns,
            blocks,
        })
    }

    /// Build the protocol's frozen synthetic corpus.
    #[must_use]
    pub fn deterministic() -> Self {
        let mut state = SYNTHETIC_CORPUS_SEED;
        let mut blocks = Vec::with_capacity(SYNTHETIC_BLOCK_COUNT);
        for block in 0..SYNTHETIC_BLOCK_COUNT {
            let mut values = Vec::with_capacity(SYNTHETIC_BLOCK_ROWS * SYNTHETIC_BLOCK_COLUMNS);
            for value in 0..(SYNTHETIC_BLOCK_ROWS * SYNTHETIC_BLOCK_COLUMNS) {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let sample = ((state >> 42) & 0x7ff) as f32 / 1024.0 - 1.0;
                let pattern = (((block * 7 + value * 3) % 17) as f32 - 8.0) / 64.0;
                let value = if (block + value) % 11 == 0 {
                    0.0
                } else {
                    (sample * 0.75 + pattern).clamp(-1.0, 1.0)
                };
                values.push(value);
            }
            blocks.push(values);
        }
        Self::new(
            SYNTHETIC_CORPUS_SEED,
            SYNTHETIC_BLOCK_ROWS,
            SYNTHETIC_BLOCK_COLUMNS,
            blocks,
        )
        .expect("frozen synthetic corpus is valid")
    }

    /// Seed recorded in the result schema.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Number of rows in each block.
    #[must_use]
    pub const fn rows(&self) -> usize {
        self.rows
    }

    /// Number of columns in each block.
    #[must_use]
    pub const fn columns(&self) -> usize {
        self.columns
    }

    /// Corpus blocks in canonical ordinal order.
    #[must_use]
    pub fn blocks(&self) -> &[Vec<f32>] {
        &self.blocks
    }

    /// Canonical logical-value denominator for bits-per-value metrics.
    #[must_use]
    pub fn logical_value_count(&self) -> u64 {
        self.blocks
            .iter()
            .map(|block| u64::try_from(block.len()).expect("validated block length fits in u64"))
            .sum()
    }
}

/// Reconstruction error metrics for one block.
#[derive(Clone, Debug, PartialEq)]
pub struct ReconstructionMetrics {
    /// Sum of squared errors against canonical logical values.
    pub sum_squared_error: f64,
    /// Maximum absolute error against canonical logical values.
    pub max_absolute_error: f64,
}

impl ReconstructionMetrics {
    /// Mean squared error against canonical logical values.
    #[must_use]
    pub fn mean_squared_error(&self, logical_value_count: usize) -> f64 {
        self.sum_squared_error / logical_value_count as f64
    }
}

/// Exact storage accounting and reconstruction metrics for one block.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockBenchmarkResult {
    /// Block ordinal in the frozen corpus.
    pub block: usize,
    /// Number of canonical logical values in the block.
    pub logical_value_count: u64,
    /// Encoded data bits, including packed indices or factors.
    pub raw_payload_bits: u64,
    /// Scale, index, codebook, mask, and other representation metadata bits.
    pub metadata_bits: u64,
    /// Padding between the payload/metadata stream and serialized byte boundary.
    pub serialized_padding_bits: u64,
    /// Padding between serialized bytes and the named resident allocation unit.
    pub resident_padding_bits: u64,
    /// Exact serialized bits, including serialized padding.
    pub serialized_bits: u64,
    /// Exact resident bits, including resident padding.
    pub resident_bits: u64,
    /// Reconstruction metrics for this block.
    pub reconstruction: ReconstructionMetrics,
}

/// Fixed-candidate result over the complete corpus.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchmarkResult {
    /// Protocol identifier.
    pub protocol_version: &'static str,
    /// Corpus seed.
    pub corpus_seed: u64,
    /// Candidate identifier.
    pub candidate: CandidateId,
    /// Candidate family.
    pub family: RepresentationFamily,
    /// Canonical logical-value denominator.
    pub logical_value_count: u64,
    /// Sum of encoded data bits across blocks.
    pub raw_payload_bits: u64,
    /// Sum of all representation metadata bits across blocks.
    pub metadata_bits: u64,
    /// Sum of serialized alignment padding across blocks.
    pub serialized_padding_bits: u64,
    /// Sum of resident alignment padding across blocks.
    pub resident_padding_bits: u64,
    /// Exact serialized bits across the corpus.
    pub serialized_bits: u64,
    /// Exact resident bits across the corpus.
    pub resident_bits: u64,
    /// Per-block accounting and reconstruction metrics.
    pub blocks: Vec<BlockBenchmarkResult>,
}

impl BenchmarkResult {
    /// Exact serialized bits per canonical logical value.
    #[must_use]
    pub fn serialized_bits_per_value(&self) -> f64 {
        self.serialized_bits as f64 / self.logical_value_count as f64
    }

    /// Exact resident bits per canonical logical value.
    #[must_use]
    pub fn resident_bits_per_value(&self) -> f64 {
        self.resident_bits as f64 / self.logical_value_count as f64
    }

    /// Stable line-oriented record suitable for a checked-in result artifact.
    #[must_use]
    pub fn canonical_record(&self) -> String {
        let mut record = format!(
            "protocol={};seed={};candidate={};family={};logical_values={};raw_payload_bits={};metadata_bits={};serialized_padding_bits={};resident_padding_bits={};serialized_bits={};resident_bits={};serialized_bits_per_value={:.12};resident_bits_per_value={:.12}",
            self.protocol_version,
            self.corpus_seed,
            self.candidate.canonical_name(),
            self.family.canonical_name(),
            self.logical_value_count,
            self.raw_payload_bits,
            self.metadata_bits,
            self.serialized_padding_bits,
            self.resident_padding_bits,
            self.serialized_bits,
            self.resident_bits,
            self.serialized_bits_per_value(),
            self.resident_bits_per_value(),
        );
        for block in &self.blocks {
            let _ = write!(
                record,
                ";block{}:logical_values={},raw_payload_bits={},metadata_bits={},serialized_padding_bits={},resident_padding_bits={},serialized_bits={},resident_bits={},mse={:.12e},max_absolute_error={:.12e}",
                block.block,
                block.logical_value_count,
                block.raw_payload_bits,
                block.metadata_bits,
                block.serialized_padding_bits,
                block.resident_padding_bits,
                block.serialized_bits,
                block.resident_bits,
                block
                    .reconstruction
                    .mean_squared_error(block.logical_value_count as usize),
                block.reconstruction.max_absolute_error,
            );
        }
        record
    }
}

/// Evaluate every candidate in the frozen list, without search or allocation.
pub fn run_fixed_baseline(
    corpus: &SyntheticCorpus,
) -> Result<Vec<BenchmarkResult>, BenchmarkError> {
    FIXED_CANDIDATES
        .iter()
        .copied()
        .map(|candidate| evaluate_candidate(corpus, candidate))
        .collect()
}

fn evaluate_candidate(
    corpus: &SyntheticCorpus,
    candidate: CandidateId,
) -> Result<BenchmarkResult, BenchmarkError> {
    let mut blocks = Vec::with_capacity(corpus.blocks.len());
    for (block, source) in corpus.blocks.iter().enumerate() {
        let (reconstructed, payload_bits, metadata_bits) =
            encode_block(source, corpus.rows, corpus.columns, candidate)?;
        blocks.push(finalize_block(
            block,
            source,
            &reconstructed,
            payload_bits,
            metadata_bits,
        )?);
    }

    let mut result = BenchmarkResult {
        protocol_version: BENCHMARK_PROTOCOL_VERSION,
        corpus_seed: corpus.seed,
        candidate,
        family: candidate.family(),
        logical_value_count: 0,
        raw_payload_bits: 0,
        metadata_bits: 0,
        serialized_padding_bits: 0,
        resident_padding_bits: 0,
        serialized_bits: 0,
        resident_bits: 0,
        blocks,
    };
    for block in &result.blocks {
        result.logical_value_count =
            checked_add(result.logical_value_count, block.logical_value_count)?;
        result.raw_payload_bits = checked_add(result.raw_payload_bits, block.raw_payload_bits)?;
        result.metadata_bits = checked_add(result.metadata_bits, block.metadata_bits)?;
        result.serialized_padding_bits = checked_add(
            result.serialized_padding_bits,
            block.serialized_padding_bits,
        )?;
        result.resident_padding_bits =
            checked_add(result.resident_padding_bits, block.resident_padding_bits)?;
        result.serialized_bits = checked_add(result.serialized_bits, block.serialized_bits)?;
        result.resident_bits = checked_add(result.resident_bits, block.resident_bits)?;
    }
    Ok(result)
}

fn encode_block(
    source: &[f32],
    rows: usize,
    columns: usize,
    candidate: CandidateId,
) -> Result<(Vec<f32>, u64, u64), BenchmarkError> {
    match candidate {
        CandidateId::DenseF32 => Ok((source.to_vec(), bits(source.len(), 32)?, 0)),
        CandidateId::DenseBf16 => {
            let reconstructed = source
                .iter()
                .map(|value| decode_bf16(encode_bf16(*value)))
                .collect();
            Ok((reconstructed, bits(source.len(), 16)?, 0))
        }
        CandidateId::DenseF16 => {
            let reconstructed = source
                .iter()
                .map(|value| decode_f16(encode_f16(*value)))
                .collect();
            Ok((reconstructed, bits(source.len(), 16)?, 0))
        }
        CandidateId::FixedInt4 => quantized_block(source, 4),
        CandidateId::FixedInt2 => quantized_block(source, 2),
        CandidateId::SparseF16 => sparse_block(source),
        CandidateId::LowRankF16 => low_rank_block(source, rows, columns),
        CandidateId::VectorCodebook8 => codebook_block(source),
        CandidateId::Int4ResidualF16 => residual_block(source),
    }
}

fn finalize_block(
    block: usize,
    source: &[f32],
    reconstructed: &[f32],
    raw_payload_bits: u64,
    metadata_bits: u64,
) -> Result<BlockBenchmarkResult, BenchmarkError> {
    let logical_value_count =
        u64::try_from(source.len()).map_err(|_| BenchmarkError::ArithmeticOverflow)?;
    let payload_and_metadata = raw_payload_bits
        .checked_add(metadata_bits)
        .ok_or(BenchmarkError::ArithmeticOverflow)?;
    let (serialized_bits, serialized_padding_bits) =
        aligned_bits(payload_and_metadata, SERIALIZED_ALIGNMENT_BITS)?;
    let (resident_bits, resident_padding_bits) =
        aligned_bits(serialized_bits, RESIDENT_ALIGNMENT_BITS)?;
    let reconstruction = reconstruction_metrics(source, reconstructed);
    Ok(BlockBenchmarkResult {
        block,
        logical_value_count,
        raw_payload_bits,
        metadata_bits,
        serialized_padding_bits,
        resident_padding_bits,
        serialized_bits,
        resident_bits,
        reconstruction,
    })
}

fn quantized_block(source: &[f32], bit_width: u64) -> Result<(Vec<f32>, u64, u64), BenchmarkError> {
    let scale = maximum_abs(source);
    let levels = ((1_u64 << (bit_width - 1)) - 1) as f32;
    let step = if scale == 0.0 { 1.0 } else { scale / levels };
    let reconstructed = source
        .iter()
        .map(|value| {
            let quantized = (*value / step).round().clamp(-levels, levels);
            quantized * step
        })
        .collect();
    Ok((reconstructed, bits(source.len(), bit_width)?, 32))
}

fn sparse_block(source: &[f32]) -> Result<(Vec<f32>, u64, u64), BenchmarkError> {
    let scale = maximum_abs(source);
    let threshold = scale * 0.5;
    let index_bits =
        ceil_log2(u64::try_from(source.len()).map_err(|_| BenchmarkError::ArithmeticOverflow)?)
            .max(1);
    let count_bits = ceil_log2(
        u64::try_from(source.len())
            .map_err(|_| BenchmarkError::ArithmeticOverflow)?
            .checked_add(1)
            .ok_or(BenchmarkError::ArithmeticOverflow)?,
    );
    let selected: Vec<usize> = source
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (value.abs() >= threshold && *value != 0.0).then_some(index))
        .collect();
    let mut reconstructed = vec![0.0; source.len()];
    for &index in &selected {
        reconstructed[index] = decode_f16(encode_f16(source[index]));
    }
    let selected_count =
        u64::try_from(selected.len()).map_err(|_| BenchmarkError::ArithmeticOverflow)?;
    let payload_bits = bits(selected.len(), 16)?;
    let metadata_bits = selected_count
        .checked_mul(index_bits)
        .and_then(|value| value.checked_add(count_bits))
        .ok_or(BenchmarkError::ArithmeticOverflow)?;
    Ok((reconstructed, payload_bits, metadata_bits))
}

fn low_rank_block(
    source: &[f32],
    rows: usize,
    columns: usize,
) -> Result<(Vec<f32>, u64, u64), BenchmarkError> {
    let mut pivot = (0, 0);
    let mut pivot_abs = 0.0_f32;
    for row in 0..rows {
        for column in 0..columns {
            let value = source[row * columns + column].abs();
            if value > pivot_abs {
                pivot = (row, column);
                pivot_abs = value;
            }
        }
    }
    let pivot_value = source[pivot.0 * columns + pivot.1];
    let mut reconstructed = vec![0.0; source.len()];
    if pivot_value != 0.0 {
        let left: Vec<f32> = (0..rows)
            .map(|row| decode_f16(encode_f16(source[row * columns + pivot.1])))
            .collect();
        let right: Vec<f32> = (0..columns)
            .map(|column| decode_f16(encode_f16(source[pivot.0 * columns + column] / pivot_value)))
            .collect();
        for row in 0..rows {
            for column in 0..columns {
                reconstructed[row * columns + column] = left[row] * right[column];
            }
        }
    }
    let payload_bits = bits(
        rows.checked_add(columns)
            .ok_or(BenchmarkError::ArithmeticOverflow)?,
        16,
    )?;
    let row_bits = ceil_log2(u64::try_from(rows).map_err(|_| BenchmarkError::ArithmeticOverflow)?);
    let column_bits =
        ceil_log2(u64::try_from(columns).map_err(|_| BenchmarkError::ArithmeticOverflow)?);
    let metadata_bits = row_bits
        .checked_add(column_bits)
        .ok_or(BenchmarkError::ArithmeticOverflow)?;
    Ok((reconstructed, payload_bits, metadata_bits))
}

fn codebook_block(source: &[f32]) -> Result<(Vec<f32>, u64, u64), BenchmarkError> {
    let reconstructed = source
        .iter()
        .map(|value| {
            CODEBOOK
                .iter()
                .copied()
                .min_by(|left, right| (*value - left).abs().total_cmp(&(*value - right).abs()))
                .expect("fixed codebook is non-empty")
        })
        .collect();
    let payload_bits = bits(source.len(), 3)?;
    // The codebook identifier is part of the metadata so a record cannot
    // silently reinterpret indices with another fixed table.
    let metadata_bits = bits(CODEBOOK.len(), 16)?
        .checked_add(16)
        .ok_or(BenchmarkError::ArithmeticOverflow)?;
    Ok((reconstructed, payload_bits, metadata_bits))
}

fn residual_block(source: &[f32]) -> Result<(Vec<f32>, u64, u64), BenchmarkError> {
    let scale = maximum_abs(source);
    let levels = 7.0_f32;
    let step = if scale == 0.0 { 1.0 } else { scale / levels };
    let mut reconstructed = Vec::with_capacity(source.len());
    let mut residual_count = 0_u64;
    for value in source {
        let quantized = (*value / step).round().clamp(-levels, levels);
        let base = quantized * step;
        let error = *value - base;
        if error.abs() > scale / 16.0 {
            residual_count = residual_count
                .checked_add(1)
                .ok_or(BenchmarkError::ArithmeticOverflow)?;
            reconstructed.push(base + decode_f16(encode_f16(error)));
        } else {
            reconstructed.push(base);
        }
    }
    let payload_bits = bits(source.len(), 4)?
        .checked_add(bits(
            usize::try_from(residual_count).map_err(|_| BenchmarkError::ArithmeticOverflow)?,
            16,
        )?)
        .ok_or(BenchmarkError::ArithmeticOverflow)?;
    // One F32 scale plus one residual-presence bit per logical value.
    let metadata_bits = 32_u64
        .checked_add(u64::try_from(source.len()).map_err(|_| BenchmarkError::ArithmeticOverflow)?)
        .ok_or(BenchmarkError::ArithmeticOverflow)?;
    Ok((reconstructed, payload_bits, metadata_bits))
}

fn reconstruction_metrics(source: &[f32], reconstructed: &[f32]) -> ReconstructionMetrics {
    let mut sum_squared_error = 0.0_f64;
    let mut max_absolute_error = 0.0_f64;
    for (source, reconstructed) in source.iter().zip(reconstructed) {
        let error = f64::from(*source) - f64::from(*reconstructed);
        sum_squared_error += error * error;
        max_absolute_error = max_absolute_error.max(error.abs());
    }
    ReconstructionMetrics {
        sum_squared_error,
        max_absolute_error,
    }
}

fn maximum_abs(values: &[f32]) -> f32 {
    values
        .iter()
        .map(|value| value.abs())
        .fold(0.0_f32, f32::max)
}

fn bits(value_count: usize, bit_width: u64) -> Result<u64, BenchmarkError> {
    u64::try_from(value_count)
        .map_err(|_| BenchmarkError::ArithmeticOverflow)?
        .checked_mul(bit_width)
        .ok_or(BenchmarkError::ArithmeticOverflow)
}

fn checked_add(left: u64, right: u64) -> Result<u64, BenchmarkError> {
    left.checked_add(right)
        .ok_or(BenchmarkError::ArithmeticOverflow)
}

fn aligned_bits(bits: u64, alignment: u64) -> Result<(u64, u64), BenchmarkError> {
    let remainder = bits % alignment;
    let padding = if remainder == 0 {
        0
    } else {
        alignment
            .checked_sub(remainder)
            .ok_or(BenchmarkError::ArithmeticOverflow)?
    };
    Ok((
        bits.checked_add(padding)
            .ok_or(BenchmarkError::ArithmeticOverflow)?,
        padding,
    ))
}

fn ceil_log2(value: u64) -> u64 {
    if value <= 1 {
        0
    } else {
        64 - u64::from((value - 1).leading_zeros())
    }
}

fn encode_bf16(value: f32) -> u16 {
    let bits = value.to_bits();
    let rounding = 0x7fff_u32 + ((bits >> 16) & 1);
    (bits.wrapping_add(rounding) >> 16) as u16
}

fn decode_bf16(value: u16) -> f32 {
    f32::from_bits(u32::from(value) << 16)
}

fn encode_f16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7f_ffff;
    if exponent == 0xff {
        let payload = if mantissa == 0 {
            0
        } else {
            ((mantissa >> 13) as u16) | 1
        };
        return sign | 0x7c00 | payload;
    }
    let half_exponent = exponent - 127 + 15;
    if half_exponent >= 31 {
        return sign | 0x7c00;
    }
    if half_exponent <= 0 {
        if half_exponent < -10 {
            return sign;
        }
        let mantissa = mantissa | 0x80_0000;
        let shift = (14 - half_exponent) as u32;
        let mut rounded = mantissa >> shift;
        let remainder = mantissa & ((1_u32 << shift) - 1);
        let halfway = 1_u32 << (shift - 1);
        if remainder > halfway || (remainder == halfway && rounded & 1 != 0) {
            rounded += 1;
        }
        return sign | rounded as u16;
    }
    let mut half_mantissa = (mantissa >> 13) as u16;
    let remainder = mantissa & 0x1fff;
    if remainder > 0x1000 || (remainder == 0x1000 && half_mantissa & 1 != 0) {
        half_mantissa += 1;
        if half_mantissa == 0x400 {
            return sign | ((half_exponent as u16 + 1) << 10);
        }
    }
    sign | ((half_exponent as u16) << 10) | half_mantissa
}

fn decode_f16(value: u16) -> f32 {
    let sign = u32::from(value & 0x8000) << 16;
    let exponent = (value >> 10) & 0x1f;
    let mantissa = value & 0x03ff;
    match exponent {
        0 => {
            if mantissa == 0 {
                f32::from_bits(sign)
            } else {
                let magnitude = (f32::from(mantissa) / 1024.0) * 2_f32.powi(-14);
                if sign == 0 {
                    magnitude
                } else {
                    -magnitude
                }
            }
        }
        0x1f => f32::from_bits(sign | 0x7f80_0000 | (u32::from(mantissa) << 13)),
        exponent => {
            let exponent = u32::from(exponent) + 112;
            f32::from_bits(sign | (exponent << 23) | (u32::from(mantissa) << 13))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_corpus_is_stable_and_well_shaped() {
        let first = SyntheticCorpus::deterministic();
        let second = SyntheticCorpus::deterministic();
        assert_eq!(first, second);
        assert_eq!(first.blocks().len(), SYNTHETIC_BLOCK_COUNT);
        assert_eq!(first.logical_value_count(), 512);
        assert!(first
            .blocks()
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
    }

    #[test]
    fn corpus_rejects_wrong_shape_and_non_finite_values() {
        assert_eq!(
            SyntheticCorpus::new(1, 2, 2, vec![vec![0.0; 3]]),
            Err(BenchmarkError::InvalidBlockShape {
                block: 0,
                expected: 4,
                actual: 3,
            })
        );
        assert_eq!(
            SyntheticCorpus::new(1, 1, 1, vec![vec![f32::NAN]]),
            Err(BenchmarkError::NonFiniteValue { block: 0, value: 0 })
        );
        assert_eq!(
            SyntheticCorpus::new(1, 0, 1, vec![vec![]]),
            Err(BenchmarkError::InvalidBlockDimensions)
        );
    }

    #[test]
    fn fixed_candidate_set_covers_all_required_families() {
        let families: Vec<_> = FIXED_CANDIDATES
            .iter()
            .map(|candidate| candidate.family())
            .collect();
        assert!(families.contains(&RepresentationFamily::Dense));
        assert!(families.contains(&RepresentationFamily::FixedQuantized));
        assert!(families.contains(&RepresentationFamily::Sparse));
        assert!(families.contains(&RepresentationFamily::LowRank));
        assert!(families.contains(&RepresentationFamily::Codebook));
        assert!(families.contains(&RepresentationFamily::HeterogeneousResidual));
    }

    #[test]
    fn fixed_baseline_is_deterministic_and_does_not_select_a_winner() {
        let corpus = SyntheticCorpus::deterministic();
        let first = run_fixed_baseline(&corpus).unwrap();
        let second = run_fixed_baseline(&corpus).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), FIXED_CANDIDATES.len());
        assert_eq!(first[0].candidate, CandidateId::DenseF32);
        assert_eq!(first[0].family, RepresentationFamily::Dense);
    }

    #[test]
    fn every_result_preserves_exact_accounting_identities() {
        let results = run_fixed_baseline(&SyntheticCorpus::deterministic()).unwrap();
        for result in results {
            assert_eq!(
                result.serialized_bits,
                result.raw_payload_bits + result.metadata_bits + result.serialized_padding_bits
            );
            assert_eq!(
                result.resident_bits,
                result.serialized_bits + result.resident_padding_bits
            );
            assert!(result.blocks.iter().all(|block| {
                block.serialized_bits
                    == block.raw_payload_bits + block.metadata_bits + block.serialized_padding_bits
                    && block.resident_bits == block.serialized_bits + block.resident_padding_bits
            }));
        }
    }

    #[test]
    fn dense_f32_reconstructs_the_canonical_values_exactly() {
        let result = run_fixed_baseline(&SyntheticCorpus::deterministic())
            .unwrap()
            .into_iter()
            .find(|result| result.candidate == CandidateId::DenseF32)
            .unwrap();
        assert_eq!(result.raw_payload_bits, 512 * 32);
        assert_eq!(result.metadata_bits, 0);
        assert_eq!(result.serialized_padding_bits, 0);
        assert_eq!(result.resident_padding_bits, 0);
        assert!(result
            .blocks
            .iter()
            .all(|block| block.reconstruction.max_absolute_error == 0.0));
    }

    #[test]
    fn canonical_records_include_schema_and_per_block_evidence() {
        let result = run_fixed_baseline(&SyntheticCorpus::deterministic())
            .unwrap()
            .into_iter()
            .find(|result| result.candidate == CandidateId::Int4ResidualF16)
            .unwrap();
        let record = result.canonical_record();
        assert!(record.starts_with("protocol=elastic-bit-allocation-v1;"));
        assert!(record.contains("candidate=int4-residual-f16"));
        assert!(record.contains(";block0:logical_values=32,raw_payload_bits="));
        assert!(record.contains("mse="));
    }
}
