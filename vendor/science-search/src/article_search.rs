use std::error::Error;

use async_trait::async_trait;
use serde::Deserialize;
use talin::tools::{FromConfig, Function};

use crate::SearchToolConfig;
use crate::query_input::QueryInput;
use crate::semantic_scholar::SemanticScholar;

const FIELDS: &str = "title,year,venue,authors,abstract,url,externalIds";

/// A tool that searches the titles and abstracts of research articles through Semantic Scholar's
/// paper relevance search.
pub struct ArticleSearchTool {
    semantic_scholar: SemanticScholar,
    max_results: usize,
}

impl FromConfig for ArticleSearchTool {
    type Config = SearchToolConfig;

    fn from_config(config: Self::Config) -> Self {
        Self {
            semantic_scholar: SemanticScholar::from_env(),
            max_results: config.max_results,
        }
    }
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<Paper>,
}

#[derive(Deserialize)]
struct Paper {
    title: Option<String>,
    year: Option<u16>,
    venue: Option<String>,
    #[serde(default)]
    authors: Vec<Author>,
    r#abstract: Option<String>,
    url: Option<String>,
    #[serde(rename = "externalIds")]
    external_ids: Option<ExternalIds>,
}

#[derive(Deserialize)]
struct Author {
    name: Option<String>,
}

#[derive(Deserialize)]
struct ExternalIds {
    #[serde(rename = "DOI")]
    doi: Option<String>,
}

impl Paper {
    fn render(self) -> String {
        let mut lines = Vec::new();
        if let Some(title) = self.title {
            lines.push(format!("<title>{title}</title>"));
        }
        if let Some(year) = self.year {
            lines.push(format!("<year>{year}</year>"));
        }
        if let Some(venue) = self.venue.filter(|venue| !venue.is_empty()) {
            lines.push(format!("<venue>{venue}</venue>"));
        }
        let authors = self
            .authors
            .into_iter()
            .filter_map(|author| author.name)
            .collect::<Vec<_>>();
        if !authors.is_empty() {
            lines.push(format!("<authors>{}</authors>", authors.join(", ")));
        }
        if let Some(doi) = self.external_ids.and_then(|ids| ids.doi) {
            lines.push(format!("<doi>{doi}</doi>"));
        }
        if let Some(url) = self.url {
            lines.push(format!("<url>{url}</url>"));
        }
        if let Some(abstract_text) = self.r#abstract {
            lines.push(format!("\n<abstract>{abstract_text}</abstract>"));
        }
        lines.join("\n")
    }
}

#[async_trait]
impl Function for ArticleSearchTool {
    type Input = QueryInput;
    type Output = Vec<String>;

    fn name(&self) -> String {
        "Article search".to_string()
    }

    fn description(&self) -> String {
        "Provides abstracts of relevant research papers based on a query.\nTitle, source, DOI, and abstract are returned for each paper.".to_string()
    }

    fn strict(&self) -> bool {
        true
    }

    async fn call(&self, input: Self::Input) -> Result<Self::Output, Box<dyn Error + Send + Sync>> {
        let response: SearchResponse = self
            .semantic_scholar
            .get(
                "/paper/search",
                &[
                    ("query", input.query.clone()),
                    ("limit", self.max_results.to_string()),
                    ("fields", FIELDS.to_owned()),
                ],
            )
            .await?;

        let results = response
            .data
            .into_iter()
            .take(self.max_results)
            .map(Paper::render)
            .collect::<Vec<_>>();
        if results.is_empty() {
            return Ok(vec![format!(
                "No papers found for the query: {}",
                input.query
            )]);
        }
        Ok(results)
    }

    fn summarize_input(&self, input: &Self::Input) -> Option<String> {
        Some(format!("Article search using the query: {}", input.query))
    }
}
