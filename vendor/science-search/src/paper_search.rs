use std::error::Error;
use std::fmt;

use async_trait::async_trait;
use serde::Deserialize;
use talin::tools::{FromConfig, Function};

use crate::SearchToolConfig;
use crate::query_input::QueryInput;
use crate::semantic_scholar::SemanticScholar;

const FIELDS: &str = "snippet.text,snippet.section";

/// A tool that searches the full text of open-access papers through Semantic Scholar's snippet
/// search.
pub struct PaperSearchTool {
    semantic_scholar: SemanticScholar,
    max_results: usize,
}

impl FromConfig for PaperSearchTool {
    type Config = SearchToolConfig;

    fn from_config(config: Self::Config) -> Self {
        Self {
            semantic_scholar: SemanticScholar::from_env(),
            max_results: config.max_results,
        }
    }
}

#[derive(Deserialize)]
struct SnippetResponse {
    #[serde(default)]
    data: Vec<SnippetMatch>,
}

#[derive(Deserialize)]
struct SnippetMatch {
    snippet: Snippet,
    paper: SnippetPaper,
}

#[derive(Deserialize)]
struct Snippet {
    text: String,
    section: Option<String>,
}

#[derive(Deserialize)]
struct SnippetPaper {
    #[serde(rename = "corpusId")]
    corpus_id: Option<CorpusId>,
    title: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
}

/// The API documents `corpusId` as a string but identifies it elsewhere as an integer.
#[derive(Deserialize)]
#[serde(untagged)]
enum CorpusId {
    Number(u64),
    Text(String),
}

impl fmt::Display for CorpusId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(id) => write!(f, "{id}"),
            Self::Text(id) => write!(f, "{id}"),
        }
    }
}

impl SnippetMatch {
    fn render(self) -> String {
        let mut lines = Vec::new();
        if let Some(title) = self.paper.title {
            lines.push(format!("<title>{title}</title>"));
        }
        if !self.paper.authors.is_empty() {
            lines.push(format!(
                "<authors>{}</authors>",
                self.paper.authors.join(", ")
            ));
        }
        if let Some(corpus_id) = self.paper.corpus_id {
            lines.push(format!(
                "<url>https://api.semanticscholar.org/CorpusId:{corpus_id}</url>"
            ));
        }
        if let Some(section) = self.snippet.section.filter(|section| !section.is_empty()) {
            lines.push(format!("<section>{section}</section>"));
        }
        lines.push(format!("\n<chunk>\n{}\n</chunk>", self.snippet.text));
        lines.join("\n")
    }
}

#[async_trait]
impl Function for PaperSearchTool {
    type Input = QueryInput;
    type Output = Vec<String>;

    fn name(&self) -> String {
        "Paper Search".to_string()
    }

    fn description(&self) -> String {
        "Provides relevant research papers based on a query.\nTitle and content chunks are returned in a structured format, with optional DOI and abstract if available.".to_string()
    }

    fn strict(&self) -> bool {
        true
    }

    async fn call(&self, input: Self::Input) -> Result<Self::Output, Box<dyn Error + Send + Sync>> {
        let response: SnippetResponse = self
            .semantic_scholar
            .get(
                "/snippet/search",
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
            .map(SnippetMatch::render)
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
        Some(format!("Paper Search with the query: {}", input.query))
    }
}
