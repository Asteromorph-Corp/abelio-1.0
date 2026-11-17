use serde::de::DeserializeOwned;

const DEFAULT_BASE_URL: &str = "https://api.semanticscholar.org/graph/v1";

#[derive(Debug, thiserror::Error)]
pub(crate) enum SemanticScholarError {
    #[error("Semantic Scholar request to {path} failed: {source}")]
    Request {
        path: String,
        source: reqwest::Error,
    },
    #[error("Semantic Scholar returned HTTP {status} for {path}: {body}")]
    Status {
        path: String,
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("Semantic Scholar returned an unexpected response for {path}: {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
}

/// A client for the Semantic Scholar Graph API, configured from `S2_API_URL` and `S2_API_KEY`.
pub(crate) struct SemanticScholar {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl SemanticScholar {
    pub(crate) fn from_env() -> Self {
        let base_url = std::env::var("S2_API_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key: std::env::var("S2_API_KEY").ok(),
        }
    }

    /// Sends `GET {base_url}{path}` and parses the JSON body.
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<T, SemanticScholarError> {
        let request_error = |source| SemanticScholarError::Request {
            path: path.to_owned(),
            source,
        };

        let mut request = self
            .client
            .get(format!("{}{path}", self.base_url))
            .query(params);
        if let Some(api_key) = &self.api_key {
            request = request.header("x-api-key", api_key);
        }
        let response = request.send().await.map_err(request_error)?;

        let status = response.status();
        let body = response.text().await.map_err(request_error)?;
        if !status.is_success() {
            return Err(SemanticScholarError::Status {
                path: path.to_owned(),
                status,
                body,
            });
        }
        serde_json::from_str(&body).map_err(|source| SemanticScholarError::Parse {
            path: path.to_owned(),
            source,
        })
    }
}
