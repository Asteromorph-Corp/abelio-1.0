use science_search::{ArticleSearchTool, PaperSearchTool, SearchToolConfig};
use serde_json::json;
use talin::tools::{FromConfig, FunctionTool};

/// The specs the model saw in the paper runs.
const ARTICLE_SEARCH_SPEC: &str = r##"{"name":"article_search","description":"Provides abstracts of relevant research papers based on a query.\nTitle, source, DOI, and abstract are returned for each paper.","parameters":{"$schema":"https://json-schema.org/draft/2020-12/schema","title":"QueryInput","description":"The input for the search tool.","type":"object","properties":{"query":{"description":"The query to search for. Escape special characters as needed.","type":"string"}},"additionalProperties":false,"required":["query"]},"strict":true}"##;
const PAPER_SEARCH_SPEC: &str = r##"{"name":"paper_search","description":"Provides relevant research papers based on a query.\nTitle and content chunks are returned in a structured format, with optional DOI and abstract if available.","parameters":{"$schema":"https://json-schema.org/draft/2020-12/schema","title":"QueryInput","description":"The input for the search tool.","type":"object","properties":{"query":{"description":"The query to search for. Escape special characters as needed.","type":"string"}},"additionalProperties":false,"required":["query"]},"strict":true}"##;

fn spec_json(tool: &dyn FunctionTool) -> String {
    let spec = tool.get_spec();
    let spec = json!({
        "name": spec.name,
        "description": spec.description,
        "parameters": spec.parameters,
        "strict": spec.strict,
    });
    serde_json::to_string(&spec).unwrap()
}

#[test]
fn tool_specs_match_paper_runs() {
    let config = || SearchToolConfig { max_results: 5 };

    let article_search = ArticleSearchTool::from_config(config());
    let paper_search = PaperSearchTool::from_config(config());

    assert_eq!(spec_json(&article_search), ARTICLE_SEARCH_SPEC);
    assert_eq!(spec_json(&paper_search), PAPER_SEARCH_SPEC);
}
