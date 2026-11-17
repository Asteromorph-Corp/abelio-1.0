//! A Semantic Scholar stand-in for the `article_search` and `paper_search` tools of the paper runs,
//! whose production implementation is omitted. The tool names, descriptions, and input schemas are
//! what the model sees and match the paper runs.

mod article_search;
mod config;
mod paper_search;
mod query_input;
mod semantic_scholar;

pub use article_search::ArticleSearchTool;
pub use config::SearchToolConfig;
pub use paper_search::PaperSearchTool;
pub use query_input::QueryInput;
