//! Pins what every agent sends to the model against goldens captured from the build used for the
//! paper runs. The goldens are never regenerated from this build.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;
use talin::agent::AgentOptions;
use talin::chain::OutputCtor;
use talin::llm::Llm;
use talin::schemas::{ContentPart, Message, ToolOutput, ToolResultMessage};
use talin::strategy::{SharedTagStrategy, Strategy};
use talin::tools::{Tool, ToolExt, ToolSpec};

use super::*;

const REPORT_INPUT: &str = "Concept: a split-intein {\"key\": [1, 2]} <b>bridge</b> for {{AAV}} delivery — ü\n\nSecond paragraph.\n";
const PLAN_INPUT: &str = "# Concept: Split intein\n\nBody [apple] text.\n\n---\n\n# References\n{\"apple\":\"# Title\\nDOI: https://doi.org/10.1/x\"}";
const SUMMARY_INPUT: &str = "# Concept: Report body\n\nSome \\<escaped\\> text.\n";
const TAG_ENTRIES: [&str; 3] = [
    "# Alpha\nDOI: https://doi.org/10.1/a",
    "Beta entry",
    "Gamma entry",
];

#[derive(Debug)]
struct Captured;

impl std::fmt::Display for Captured {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("request captured")
    }
}

impl std::error::Error for Captured {}

#[derive(Default)]
struct Capture {
    messages: Vec<Message>,
    request: Option<Value>,
}

/// Records the initial messages and the first request body, then aborts the turn before sending.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Capture>>);

impl Recorder {
    fn take(&self) -> Capture {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

#[async_trait]
impl<L, O> Strategy<L, O> for Recorder
where
    L: Llm,
    L::Request: serde::Serialize,
    O: OutputCtor,
{
    type Output = ();
    type Error = Captured;

    async fn process_initial_messages(
        &mut self,
        messages: Vec<Message>,
    ) -> Result<Vec<Message>, Self::Error> {
        self.0.lock().unwrap().messages = messages.clone();
        Ok(messages)
    }

    async fn process_request(&mut self, request: L::Request) -> Result<L::Request, Self::Error> {
        let mut capture = self.0.lock().unwrap();
        if capture.request.is_none() {
            capture.request = Some(serde_json::to_value(&request).unwrap());
        }
        Err(Captured)
    }

    async fn finalize(self) -> Result<Self::Output, Self::Error> {
        Ok(())
    }
}

fn block(out: &mut String, header: &str, body: &str) {
    writeln!(out, "<<< {header}").unwrap();
    out.push_str(body);
    out.push_str("\n>>>\n");
}

fn text(parts: &[ContentPart]) -> &str {
    match parts {
        [ContentPart::Text(text)] => text,
        other => panic!("expected a single text part, got {other:?}"),
    }
}

fn render<'a>(agent: &str, capture: &Capture, tools: impl IntoIterator<Item = &'a Tool>) -> String {
    let request = capture.request.as_ref().expect("a request was built");
    let mut out = String::new();
    writeln!(out, "agent: {agent}").unwrap();
    block(
        &mut out,
        "request model",
        request["model"].as_str().unwrap(),
    );
    let max_tokens = request
        .get("max_tokens")
        .or(request.get("max_output_tokens"))
        .map_or("none".to_owned(), Value::to_string);
    block(&mut out, "request max_tokens", &max_tokens);

    for (i, message) in capture.messages.iter().enumerate() {
        let (role, content) = match message {
            Message::System(m) => ("system", m.content.as_str()),
            Message::User(m) => ("user", text(&m.content)),
            other => panic!("unexpected initial message {other:?}"),
        };
        block(&mut out, &format!("message {i} role={role}"), content);
    }

    let mut specs = tools
        .into_iter()
        .map(|tool| match tool.get_spec() {
            ToolSpec::Function(spec) => spec,
            other => panic!("unexpected tool {other:?}"),
        })
        .collect::<Vec<_>>();
    specs.sort_by(|a, b| a.name.cmp(&b.name));
    for spec in &specs {
        assert!(spec.custom_description.is_none());
        let name = &spec.name;
        block(&mut out, &format!("tool {name} name"), name);
        block(
            &mut out,
            &format!("tool {name} description"),
            spec.description.as_deref().unwrap_or(""),
        );
        block(
            &mut out,
            &format!("tool {name} parameters"),
            &serde_json::to_string_pretty(&spec.parameters).unwrap(),
        );
        block(
            &mut out,
            &format!("tool {name} strict"),
            &spec.strict.to_string(),
        );
    }

    let format = request
        .get("output_config")
        .or(request.get("text"))
        .and_then(|config| config.get("format"));
    match format {
        None => block(&mut out, "response_format", "none"),
        Some(format) => block(
            &mut out,
            "response_format schema",
            &serde_json::to_string_pretty(&format["schema"]).unwrap(),
        ),
    }
    out
}

fn assert_golden(name: &str, actual: &str) {
    let path = format!("{}/tests/golden/{name}.txt", env!("CARGO_MANIFEST_DIR"));
    let expected = std::fs::read_to_string(&path).unwrap();
    assert!(
        expected == actual,
        "{path} differs from the rendered prompt:\n{actual}"
    );
}

fn set_env() {
    // SAFETY: the test touches only this variable, with the same value, before any agent is built.
    unsafe {
        std::env::set_var("ANTHROPIC_API_KEY", "dummy");
    }
}

fn pipeline_options() -> AgentOptions {
    AgentOptions::default().with_max_consecutive_fails(10)
}

#[tokio::test]
async fn report_writer_prompt_matches_golden() {
    set_env();
    let recorder = Recorder::default();
    let agent = ReportWriterAgent::default()
        .await
        .with_options(pipeline_options());
    let _ = agent
        .run_with_strategy_immed(
            (SharedTagStrategy::default(), recorder.clone()),
            REPORT_INPUT,
        )
        .await;
    let session = agent.session();
    let rendered = render("report_writer", &recorder.take(), session.tools.values());
    assert_golden("report_writer", &rendered);
}

#[tokio::test]
async fn pathfinder_prompt_matches_golden() {
    set_env();
    let recorder = Recorder::default();
    let agent = PathfinderAgent::default()
        .await
        .with_options(pipeline_options());
    let _ = agent
        .run_with_strategy_immed((SharedTagStrategy::default(), recorder.clone()), PLAN_INPUT)
        .await;
    let session = agent.session();
    let rendered = render("pathfinder", &recorder.take(), session.tools.values());
    assert_golden("pathfinder", &rendered);
}

#[tokio::test]
async fn summarizer_prompt_matches_golden() {
    set_env();
    let recorder = Recorder::default();
    let agent = SummarizerAgent::default()
        .await
        .with_options(pipeline_options());
    let _ = agent
        .run_with_strategy_immed(recorder.clone(), SUMMARY_INPUT)
        .await;
    let session = agent.session();
    let rendered = render("summarizer", &recorder.take(), session.tools.values());
    assert_golden("summarizer", &rendered);
}

#[tokio::test]
async fn shared_tag_strategy_tags_match_golden() {
    let mut strategy = SharedTagStrategy::default();
    let output = ToolOutput::new(
        TAG_ENTRIES
            .iter()
            .map(|entry| ContentPart::Text((*entry).to_owned()))
            .collect(),
    );
    let message = ToolResultMessage::new("call_0", "article_search", Ok(output));
    let result =
        Strategy::<talin::llm::ClaudeMessages, ReportWriterAgentOutputCtor>::process_tool_result(
            &mut strategy,
            message,
        )
        .await
        .unwrap();
    let mut entries = result
        .unwrap()
        .content
        .into_iter()
        .map(|part| match part {
            ContentPart::Text(text) => text,
            other => panic!("unexpected content {other:?}"),
        })
        .collect::<Vec<_>>();
    entries.sort();
    let mut rendered = String::new();
    for (i, entry) in entries.iter().enumerate() {
        block(&mut rendered, &format!("tagged entry {i}"), entry);
    }
    assert_golden("shared_tag_strategy", &rendered);
}
