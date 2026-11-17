use std::sync::Mutex;

use mockito::{Matcher, Server, ServerGuard};
use science_search::{ArticleSearchTool, PaperSearchTool, QueryInput, SearchToolConfig};
use talin::tools::{FromConfig, Function};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn tool<T>(server: &ServerGuard, max_results: usize) -> T
where
    T: FromConfig<Config = SearchToolConfig>,
{
    let _guard = ENV_LOCK.lock().unwrap();
    // SAFETY: the lock serializes this binary's tests, and each test builds its tool before using
    // the network.
    unsafe {
        std::env::set_var("S2_API_URL", server.url());
        std::env::set_var("S2_API_KEY", "test-key");
    }
    T::from_config(SearchToolConfig { max_results })
}

fn query(query: &str) -> QueryInput {
    QueryInput {
        query: query.to_owned(),
    }
}

#[tokio::test]
async fn article_search_renders_one_block_per_paper_up_to_max_results() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("GET", "/paper/search")
        .match_header("x-api-key", "test-key")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("query".into(), "protein folding".into()),
            Matcher::UrlEncoded("limit".into(), "2".into()),
            Matcher::UrlEncoded(
                "fields".into(),
                "title,year,venue,authors,abstract,url,externalIds".into(),
            ),
        ]))
        .with_body(
            r#"{"total": 9, "offset": 0, "next": 2, "data": [
                {"paperId": "a", "title": "Folding at scale", "year": 2021, "venue": "Nature",
                 "authors": [{"authorId": "1", "name": "Ada Lovelace"}, {"authorId": "2", "name": "Alan Turing"}],
                 "abstract": "We fold proteins.", "url": "https://www.semanticscholar.org/paper/a",
                 "externalIds": {"DOI": "10.1/fold"}},
                {"paperId": "b", "title": "No abstract paper", "year": null, "venue": "",
                 "authors": [], "abstract": null, "url": "https://www.semanticscholar.org/paper/b"},
                {"paperId": "c", "title": "Over the cap"}
            ]}"#,
        )
        .create_async()
        .await;
    let tool: ArticleSearchTool = tool(&server, 2);

    let blocks = tool.call(query("protein folding")).await.unwrap();

    mock.assert_async().await;
    assert_eq!(
        blocks,
        vec![
            "<title>Folding at scale</title>\n<year>2021</year>\n<venue>Nature</venue>\n<authors>Ada Lovelace, Alan Turing</authors>\n<doi>10.1/fold</doi>\n<url>https://www.semanticscholar.org/paper/a</url>\n\n<abstract>We fold proteins.</abstract>",
            "<title>No abstract paper</title>\n<url>https://www.semanticscholar.org/paper/b</url>",
        ]
    );
}

#[tokio::test]
async fn article_search_fails_on_http_error() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/paper/search")
        .match_query(Matcher::Any)
        .with_status(429)
        .with_body(r#"{"message": "Too Many Requests"}"#)
        .create_async()
        .await;
    let tool: ArticleSearchTool = tool(&server, 5);

    let error = tool.call(query("anything")).await.unwrap_err().to_string();

    assert!(error.contains("429"), "{error}");
    assert!(error.contains("Too Many Requests"), "{error}");
}

#[tokio::test]
async fn article_search_fails_on_unparseable_body() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/paper/search")
        .match_query(Matcher::Any)
        .with_body("<html>gateway page</html>")
        .create_async()
        .await;
    let tool: ArticleSearchTool = tool(&server, 5);

    let error = tool.call(query("anything")).await.unwrap_err().to_string();

    assert!(error.contains("unexpected response"), "{error}");
}

#[tokio::test]
async fn paper_search_renders_one_block_per_snippet() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("GET", "/snippet/search")
        .match_header("x-api-key", "test-key")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("query".into(), "attention mechanism".into()),
            Matcher::UrlEncoded("limit".into(), "3".into()),
            Matcher::UrlEncoded("fields".into(), "snippet.text,snippet.section".into()),
        ]))
        .with_body(
            r#"{"data": [
                {"score": 0.9,
                 "snippet": {"text": "Attention is all you need.", "section": "Introduction"},
                 "paper": {"corpusId": "215416146", "title": "Attention paper", "authors": ["A. Vaswani", "N. Shazeer"]}},
                {"score": 0.5,
                 "snippet": {"text": "Second snippet."},
                 "paper": {"corpusId": 42, "title": "Other paper", "authors": []}}
            ], "retrievalVersion": "v1"}"#,
        )
        .create_async()
        .await;
    let tool: PaperSearchTool = tool(&server, 3);

    let blocks = tool.call(query("attention mechanism")).await.unwrap();

    mock.assert_async().await;
    assert_eq!(
        blocks,
        vec![
            "<title>Attention paper</title>\n<authors>A. Vaswani, N. Shazeer</authors>\n<url>https://api.semanticscholar.org/CorpusId:215416146</url>\n<section>Introduction</section>\n\n<chunk>\nAttention is all you need.\n</chunk>",
            "<title>Other paper</title>\n<url>https://api.semanticscholar.org/CorpusId:42</url>\n\n<chunk>\nSecond snippet.\n</chunk>",
        ]
    );
}

#[tokio::test]
async fn paper_search_fails_on_http_error() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/snippet/search")
        .match_query(Matcher::Any)
        .with_status(400)
        .with_body(r#"{"error": "Unrecognized or unsupported fields"}"#)
        .create_async()
        .await;
    let tool: PaperSearchTool = tool(&server, 5);

    let error = tool.call(query("anything")).await.unwrap_err().to_string();

    assert!(error.contains("400"), "{error}");
    assert!(
        error.contains("Unrecognized or unsupported fields"),
        "{error}"
    );
}

#[tokio::test]
async fn searches_with_no_results_say_so() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/paper/search")
        .match_query(Matcher::Any)
        .with_body(r#"{"total": 0, "offset": 0}"#)
        .create_async()
        .await;
    let tool: ArticleSearchTool = tool(&server, 5);

    let blocks = tool.call(query("zzzz")).await.unwrap();

    assert_eq!(blocks, vec!["No papers found for the query: zzzz"]);
}
