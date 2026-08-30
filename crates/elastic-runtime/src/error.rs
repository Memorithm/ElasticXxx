//! Runtime error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("observation error: {0}")]
    Observation(String),
    #[error("planning error: {0}")]
    Planning(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("actuation error: {0}")]
    Actuation(String),
    #[error("verification error: {0}")]
    Verification(String),
    #[error("commit error: {0}")]
    Commit(String),
    #[error("rollback error: {0}")]
    Rollback(String),
}

impl RuntimeError {
    pub fn configuration(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }

    pub fn observation(msg: impl Into<String>) -> Self {
        Self::Observation(msg.into())
    }

    pub fn planning(msg: impl Into<String>) -> Self {
        Self::Planning(msg.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn actuation(msg: impl Into<String>) -> Self {
        Self::Actuation(msg.into())
    }

    pub fn verification(msg: impl Into<String>) -> Self {
        Self::Verification(msg.into())
    }

    pub fn commit(msg: impl Into<String>) -> Self {
        Self::Commit(msg.into())
    }

    pub fn rollback(msg: impl Into<String>) -> Self {
        Self::Rollback(msg.into())
    }
}
