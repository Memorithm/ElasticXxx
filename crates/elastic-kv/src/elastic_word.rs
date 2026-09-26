//! Compatibility re-export of the generic ElasticWord v1 contract.
//!
//! ElasticWord is owned by `elastic-core` because width elasticity is generic
//! adaptive-resource state rather than KV-specific semantics. This module keeps
//! the `elastic_kv::elastic_word` path stable for early consumers while KV
//! adapters continue to add only domain-specific validation.

pub use elastic_core::{
    ElasticWordError, ElasticWordPlaneV1, ElasticWordWidthTransitionV1, ElasticWordWidthV1,
    ELASTIC_WORD_LANE_BITS, ELASTIC_WORD_MAX_LANES, ELASTIC_WORD_MIN_LANES,
    ELASTIC_WORD_WIDTH_V1,
};
