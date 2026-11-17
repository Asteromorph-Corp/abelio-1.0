use std::cmp::Ordering;

use rerankers::Reranker;
use rerankers::schema::{Document, Score};
use rerankers::zerank_2::Zerank2Reranker;

const DEFAULT_RETRIES: usize = 3;

#[derive(Clone)]
pub struct RerankerCtx {
    reranker_client: Zerank2Reranker,
    retries: usize,
}

impl RerankerCtx {
    pub async fn new() -> anyhow::Result<Self> {
        Self::with_retries(DEFAULT_RETRIES).await
    }

    pub async fn with_retries(retries: usize) -> anyhow::Result<Self> {
        let reranker_endpoint =
            std::env::var("ZERANK2_URL").expect("ZERANK2_URL must be set in env variables");

        let reranker_api_key =
            std::env::var("ZERANK2_API_KEY").expect("ZERANK2_API_KEY must be set in env variables");

        let reranker_endpoint = reqwest::Url::parse(&reranker_endpoint)?;
        let reranker_client =
            Zerank2Reranker::new(reranker_endpoint).with_api_key(reranker_api_key);

        Ok(Self {
            reranker_client,
            retries,
        })
    }

    pub async fn score_documents(&self, query: &str, docs: &[String]) -> anyhow::Result<Vec<f64>> {
        let docs = docs
            .iter()
            .enumerate()
            .map(|(id, text)| Document::new(id, text))
            .collect::<Vec<_>>();
        let results: Vec<Score> = {
            let mut attempts = 0;
            loop {
                match self.reranker_client.score_docs(query, &docs).await {
                    Ok(res) => break Ok(res),
                    Err(e) => {
                        attempts += 1;
                        tracing::warn!(error = %e, attempt = attempts, "reranker attempt failed");
                        if attempts >= self.retries {
                            break Err(e);
                        }
                    }
                }
            }
        }?;
        if docs.len() != results.len() {
            anyhow::bail!("Reranker returned mismatched number of results");
        }
        Ok(results.into_iter().map(|s| s.0).collect())
    }

    pub async fn top_n(
        &self,
        query: &str,
        docs: &[String],
        n: usize,
    ) -> anyhow::Result<Vec<String>> {
        let scores = self.score_documents(query, docs).await?;
        let mut doc_scores = docs
            .iter()
            .cloned()
            .zip(scores)
            .collect::<Vec<(String, f64)>>();
        doc_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        Ok(doc_scores.into_iter().take(n).map(|(doc, _)| doc).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "live: calls the Zerank2 API"]
    async fn test_reranker_ctx() {
        dotenvy::dotenv().ok();
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
        let reranker_ctx = RerankerCtx::new().await.unwrap();
        let results = reranker_ctx
            .score_documents(
                "machine learning in healthcare",
                &[
                    "healthcare research".to_string(),
                    "medical data analysis".to_string(),
                    "quantum computing".to_string(),
                ],
            )
            .await
            .unwrap();
        println!("Results: {:?}", results);
        assert!(!results.is_empty());
    }
}
