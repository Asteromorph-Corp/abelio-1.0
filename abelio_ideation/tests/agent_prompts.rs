//! Renders every agent with fixed fixture inputs and compares what the model would see (messages,
//! tool specs, built-in tools, response format) against `tests/golden/<agent>.txt`. An agent with
//! per-model-type overrides keeps one golden per former provider variant, rendered with a preset of
//! that variant's model type.
//!
//! The goldens were captured from the build used for the paper runs and have since been edited only
//! by the proofreading in this repository's history; they are never regenerated from this build.
//! No LLM API calls are made.

mod common;

use std::fmt::Write as _;
use std::sync::Once;

use abelio_ideation::testing::*;
use common::*;
use talin::agent::Agent;
use talin::chain::{InputCtor, OutputCtor};
use talin::llm::Llm;
use talin::schemas::{ContentExt, Message, Prompt};
use talin::strategy::Strategy;
use talin::tools::{ToolExt, ToolSpec};

static ENV: Once = Once::new();

/// Models read their keys from the environment when constructed.
fn env() {
    ENV.call_once(|| {
        for var in [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "GEMINI_API_KEY",
            "XAI_API_KEY",
            "OPENROUTER_API_KEY",
        ] {
            // SAFETY: called once, before any test reads the environment.
            unsafe { std::env::set_var(var, "golden-dummy-key") };
        }
    });
}

/// Whether the generated agent sends a response format; talin keeps it in the model's private
/// options.
fn has_response_format(id: &str) -> bool {
    let path = format!("{}/agents/{id}.rs", env!("OUT_DIR"));
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    src.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .contains(".with_response_format(")
}

/// The response format derived from the agent's output type.
fn response_format<L, I, O, S>(_agent: &Agent<L, I, O, S>) -> String
where
    L: Llm,
    I: InputCtor,
    O: OutputCtor,
    S: Strategy<L, O> + Clone,
    for<'a> O::Target<'a>: schemars::JsonSchema,
{
    let schema = schemars::schema_for!(<O as OutputCtor>::Target<'static>);
    let name = schema
        .get("title")
        .and_then(|v| v.as_str())
        .expect("schema title");
    let description = schema.get("description").and_then(|v| v.as_str());
    let mut out = String::new();
    writeln!(out, "--- response format").unwrap();
    writeln!(out, "name: {name}").unwrap();
    writeln!(out, "description: {}", description.unwrap_or("(none)")).unwrap();
    writeln!(out, "strict: true").unwrap();
    writeln!(out, "schema:").unwrap();
    writeln!(out, "{}", serde_json::to_string_pretty(&schema).unwrap()).unwrap();
    out
}

fn render<L, I, O, S>(
    name: &str,
    agent: &Agent<L, I, O, S>,
    prompt: Prompt,
    response_format: Option<String>,
) -> String
where
    L: Llm,
    L::BuiltinTool: serde::Serialize,
    I: InputCtor,
    O: OutputCtor,
    S: Strategy<L, O> + Clone,
{
    assert_eq!(
        has_response_format(agent.id()),
        response_format.is_some(),
        "response format presence for {name}"
    );

    let mut out = String::new();
    writeln!(out, "=== agent: {name}").unwrap();
    for (idx, message) in prompt.to_messages().into_iter().enumerate() {
        let (role, content) = match message {
            Message::System(m) => ("system", m.content),
            Message::User(m) => ("user", m.content.to_text().unwrap_or_default()),
            Message::Assistant(m) => ("assistant", m.content.to_text().unwrap_or_default()),
            Message::ToolCall(m) => ("assistant", m.content.to_text().unwrap_or_default()),
            Message::Tool(m) => ("tool", m.output.content.to_text().unwrap_or_default()),
        };
        writeln!(out, "--- message[{idx}] {role}").unwrap();
        writeln!(out, "{content}").unwrap();
    }

    let session = agent.session();
    let mut functions = session
        .tools
        .values()
        .map(|tool| match tool.get_spec() {
            ToolSpec::Function(f) => f,
            other => panic!("unexpected tool spec in {name}: {other:?}"),
        })
        .collect::<Vec<_>>();
    functions.sort_by(|a, b| a.name.cmp(&b.name));
    for f in functions {
        writeln!(out, "--- tool: {}", f.name).unwrap();
        writeln!(out, "description:").unwrap();
        writeln!(out, "{}", f.description.as_deref().unwrap_or("(none)")).unwrap();
        writeln!(out, "strict: {}", f.strict).unwrap();
        writeln!(out, "parameters:").unwrap();
        // `to_value` keeps the key order sent on the wire, which differs from `Schema`'s
        // `Serialize`.
        let parameters = f.parameters.to_value();
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&parameters).unwrap()
        )
        .unwrap();
    }

    for builtin in &session.builtin_tools {
        writeln!(out, "--- builtin tool").unwrap();
        writeln!(out, "{}", serde_json::to_string(builtin).unwrap()).unwrap();
    }

    out.extend(response_format);
    out
}

fn assert_golden(id: &str, actual: String) {
    let path = format!("{}/tests/golden/{id}.txt", env!("CARGO_MANIFEST_DIR"));
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert!(actual == expected, "{id} differs from {path}:\n{actual}");
}

macro_rules! golden {
    (@format schema, $a:ident) => {
        Some(response_format(&$a))
    };
    (@format text, $a:ident) => {
        None
    };
    (@agent $agent:ident) => {
        $agent::default().await
    };
    (@agent $agent:ident, $preset:expr) => {
        $agent::new($preset).await
    };
    ($id:ident, $agent:ident $(($preset:expr))?, $format:ident, |$a:ident| $prompt:expr) => {
        #[tokio::test]
        async fn $id() -> anyhow::Result<()> {
            env();
            let $a = golden!(@agent $agent $(, $preset)?);
            let prompt = $prompt?;
            let format = golden!(@format $format, $a);
            assert_golden(stringify!($id), render(stringify!($id), &$a, prompt, format));
            Ok(())
        }
    };
}

macro_rules! divergent_formulation_golden {
    ($id:ident, $agent:ident) => {
        golden!($id, $agent, schema, |a| {
            let obj = objective();
            a.get_prompt_immed(&obj.core_goal, &obj.key_constraints)
        });
    };
}

divergent_formulation_golden!(divergent_formulation_scope, DivergentFormulationScopeAgent);
divergent_formulation_golden!(
    divergent_formulation_mechanism,
    DivergentFormulationMechanismAgent
);
divergent_formulation_golden!(divergent_formulation_gaps, DivergentFormulationGapsAgent);

golden!(formulation_enhancer, FormulationEnhancerAgent, text, |a| {
    a.get_prompt_immed(formulation(), &objective())
});
golden!(goal_simplifier, GoalSimplifierAgent, text, |a| a
    .get_prompt_immed(formulation()));
golden!(requirement_search, RequirementSearchAgent, schema, |a| {
    a.get_prompt_immed(formulation(), &objective())
});

// One golden per former provider variant, rendered with a preset of that variant's model type.
macro_rules! element_search_golden {
    ($id:ident, $agent:ident, $preset:expr) => {
        golden!($id, $agent($preset), schema, |a| a
            .get_prompt_immed(&requirement().major));
    };
}

element_search_golden!(
    concept_element_search,
    ConceptElementSearchAgent,
    Models::deepseek_v32()
);
element_search_golden!(
    concept_element_search_anthropic,
    ConceptElementSearchAgent,
    Models::claude_sonnet_45()
);
element_search_golden!(
    concept_element_search_openai,
    ConceptElementSearchAgent,
    Models::gpt_52()
);
element_search_golden!(
    concept_element_search_google,
    ConceptElementSearchAgent,
    Models::gemini_3_5_flash()
);
element_search_golden!(
    concept_element_search_xai,
    ConceptElementSearchAgent,
    Models::grok_41_fast_reasoning()
);
element_search_golden!(
    entity_element_search,
    EntityElementSearchAgent,
    Models::deepseek_v32()
);
element_search_golden!(
    entity_element_search_anthropic,
    EntityElementSearchAgent,
    Models::claude_sonnet_45()
);
element_search_golden!(
    entity_element_search_openai,
    EntityElementSearchAgent,
    Models::gpt_52()
);
element_search_golden!(
    entity_element_search_google,
    EntityElementSearchAgent,
    Models::gemini_3_5_flash()
);

golden!(
    alternative_element_search,
    AlternativeElementSearchAgent,
    schema,
    |a| {
        a.get_prompt_immed(
            &objective().key_constraints,
            "TAT peptide",
            &requirement().major,
        )
    }
);
golden!(element_merger, ElementMergerAgent, schema, |a| {
    a.get_prompt_immed(&sample_elements(), &requirement())
});
golden!(
    element_novelty_filter,
    ElementNoveltyFilterAgent,
    schema,
    |a| { a.get_prompt_immed(&sample_elements(), formulation()) }
);
golden!(
    element_constraints_filter,
    ElementConstraintsFilterAgent,
    schema,
    |a| {
        a.get_prompt_immed(
            &objective().key_constraints,
            &sample_elements(),
            formulation(),
        )
    }
);
golden!(
    element_requirement_filter,
    ElementRequirementFilterAgent,
    schema,
    |a| { a.get_prompt_immed(&sample_elements(), &requirement().minor, formulation()) }
);

macro_rules! element_selector_golden {
    ($id:ident, $preset:expr) => {
        golden!($id, ElementSelectorAgent($preset), text, |a| {
            let obj = objective();
            a.get_prompt_immed(
                &obj.key_constraints,
                &obj.core_goal,
                &sample_elements(),
                formulation(),
            )
        });
    };
}

element_selector_golden!(element_selector, Models::deepseek_v32());
element_selector_golden!(element_selector_anthropic, Models::claude_sonnet_45());
element_selector_golden!(element_selector_openai, Models::gpt_52());
element_selector_golden!(element_selector_google, Models::gemini_3_pro_preview());
element_selector_golden!(element_selector_xai, Models::grok_41_fast_reasoning());

macro_rules! proposal_maker_golden {
    ($id:ident, $preset:expr) => {
        golden!($id, ProposalMakerAgent($preset), text, |a| a
            .get_prompt_immed());
    };
}

proposal_maker_golden!(proposal_maker, Models::deepseek_v32());
proposal_maker_golden!(proposal_maker_anthropic, Models::claude_sonnet_45());
proposal_maker_golden!(proposal_maker_openai, Models::gpt_52());
proposal_maker_golden!(proposal_maker_google, Models::gemini_3_pro_preview());
proposal_maker_golden!(proposal_maker_xai, Models::grok_41_fast_reasoning());

golden!(reviewer, ReviewerAgent, text, |a| {
    a.get_prompt_immed(&ReviewSubmission::new_without_statement(sample_idea(), 1))
});
golden!(defender, DefenderAgent, schema, |a| {
    let obj = objective();
    let challenge = RefinementChallenge::new(sample_critique(), sample_idea(), 1);
    a.get_prompt_immed(
        &challenge,
        &obj.core_goal,
        &obj.key_constraints,
        formulation(),
    )
});
golden!(judge, JudgeAgent, schema, |a| {
    a.get_prompt_immed(
        sample_idea(),
        sample_critique(),
        sample_defense(),
        &objective().key_constraints,
    )
});
