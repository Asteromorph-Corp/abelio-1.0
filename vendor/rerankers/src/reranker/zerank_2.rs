//! Reranker implementation for Zerank2 models hosted on ZeroEntropy.

use async_trait::async_trait;
use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};

use crate::Reranker;
use crate::error::RerankerError;
use crate::schema::{Document, Score};

const MODEL_NAME: &str = "zerank-2";

/// The reranker corresponding to Zerank2 reranker models hosted on ZeroEntropy.
#[derive(Clone)]
pub struct Zerank2Reranker {
    /// The HTTP client used to making requests.
    client: reqwest::Client,
    /// The API endpoint URL.
    api_base: Url,
    /// The API key.
    api_key: Option<SecretString>,
}

impl Zerank2Reranker {
    /// Create a new Zerank2Reranker.
    pub fn new(api_base: Url) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_base,
            api_key: None,
        }
    }

    /// Set the API key.
    pub fn with_api_key(mut self, api_key: impl Into<SecretString>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }
}

/// The request body for Zerank2 API.
#[derive(serde::Serialize)]
struct Zerank2Request<'a> {
    model: &'static str,
    query: &'a str,
    documents: Vec<&'a str>,
}

/// The response body for Zerank2 API.
#[derive(serde::Deserialize)]
struct Zerank2Response {
    results: Vec<Zerank2Document>,
}

/// The document inside Zerank2Response.
#[derive(serde::Deserialize)]
struct Zerank2Document {
    index: usize,
    relevance_score: f64,
}

#[async_trait]
impl Reranker for Zerank2Reranker {
    async fn score_docs_raw<'a>(
        &self,
        query: &str,
        docs: &[Document<'a>],
    ) -> Result<Vec<Score>, RerankerError> {
        let payload = Zerank2Request {
            model: MODEL_NAME,
            query,
            documents: docs.iter().map(|d| d.text.as_ref()).collect::<Vec<_>>(),
        };
        let mut req = self.client.post(self.api_base.clone()).json(&payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key.expose_secret());
        }

        let mut res = req
            .send()
            .await?
            .error_for_status()?
            .json::<Zerank2Response>()
            .await?;
        res.results.sort_by_key(|doc| doc.index);
        let scores = res
            .results
            .into_iter()
            .map(|doc| Score(doc.relevance_score))
            .collect::<Vec<_>>();
        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use mockito::{Matcher, Server};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn scores_follow_document_index_order() -> anyhow::Result<()> {
        let mut server = Server::new_async().await;

        let query = "Find the capital city of France.";
        let texts = [
            "The capital of France is Paris.",
            "The capital of Germany is Berlin.",
            "The capital of Italy is Rome.",
        ];

        let expected_req = json!({
            "model": "zerank-2",
            "query": query,
            "documents": texts
        });
        let res = json!({
          "results": [
            { "index": 3, "relevance_score": 0.42 },
            { "index": 1, "relevance_score": 0.8 },
            { "index": 2, "relevance_score": 0.3 }
          ]
        });

        let mock = server
            .mock("POST", "/")
            .match_header("content-type", Matcher::Regex("application/json.*".into()))
            .match_header("authorization", "Bearer test-key")
            .match_body(Matcher::PartialJson(expected_req))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(res.to_string())
            .expect(1)
            .create_async()
            .await;

        let api_base = Url::parse(&server.url())?;
        let reranker = Zerank2Reranker::new(api_base).with_api_key("test-key");

        let docs = texts
            .iter()
            .enumerate()
            .map(|(id, text)| Document::new(id, *text))
            .collect::<Vec<_>>();
        let scores = reranker.score_docs(query, &docs).await?;

        mock.assert_async().await;
        assert_eq!(scores, vec![Score(0.8), Score(0.3), Score(0.42)]);

        Ok(())
    }
}
