use async_trait::async_trait;

use crate::error::RerankerError;
use crate::schema::{Document, Score};

pub mod zerank_2;

/// Reranker for scoring documents based on their relevance to a query.
#[async_trait]
pub trait Reranker {
    /// Timeout for each reranking operation in milliseconds.
    const TIMEOUT_MS: u64 = 15000;

    /// Implementation of [`score_docs`](Self::score_docs), without retries.
    ///
    /// Each score corresponds to the document at the same index.
    async fn score_docs_raw<'a>(
        &self,
        query: &str,
        docs: &[Document<'a>],
    ) -> Result<Vec<Score>, RerankerError>;

    /// Return a score for each input document, retrying with exponential backoff until
    /// [`TIMEOUT_MS`](Self::TIMEOUT_MS) elapses.
    ///
    /// Each score corresponds to the document at the same index.
    async fn score_docs<'a>(
        &self,
        query: &str,
        docs: &[Document<'a>],
    ) -> Result<Vec<Score>, RerankerError> {
        backoff::future::retry(
            backoff::ExponentialBackoff {
                max_elapsed_time: Some(std::time::Duration::from_millis(Self::TIMEOUT_MS)),
                ..Default::default()
            },
            || async {
                let scores = self.score_docs_raw(query, docs).await?;
                Ok(scores)
            },
        )
        .await
    }
}
