//! Error types for reranking operations.

use thiserror::Error;

/// Errors that can occur during reranking operations.
#[derive(Debug, Error)]
pub enum RerankerError {
    /// A network request error.
    #[error("Reqwest error: {0:?}")]
    Reqwest(#[from] reqwest::Error),
}
