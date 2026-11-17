//! Live-LLM smoke tests for every non-base agent.
//!
//! Each test constructs the agent, calls `run_immed(...)` against real provider APIs with a fixture
//! input, then prints the returned output for inspection. These hit the network and cost money, so
//! they are `#[ignore]` by default.
//!
//! Run with:
//! ```bash
//! cargo test --test agent_calls -- --ignored --nocapture
//! ```
//!
//! A `.env` file with the relevant provider keys (OPENAI_API_KEY, ANTHROPIC_API_KEY,
//! GOOGLE_API_KEY, XAI_API_KEY, ...) is loaded on startup.

mod common;

use abelio_ideation::testing::*;
use common::*;

fn dump(label: &str, output: &impl std::fmt::Debug) {
    println!("output: {label}\n{output:#?}");
}

#[tokio::test]
#[ignore]
async fn divergent_formulation_scope() -> anyhow::Result<()> {
    init();
    let obj = objective();
    let out = DivergentFormulationScopeAgent::default()
        .await
        .run_immed(&obj.core_goal, &obj.key_constraints)
        .await?;
    dump("divergent_formulation_scope", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn divergent_formulation_mechanism() -> anyhow::Result<()> {
    init();
    let obj = objective();
    let out = DivergentFormulationMechanismAgent::default()
        .await
        .run_immed(&obj.core_goal, &obj.key_constraints)
        .await?;
    dump("divergent_formulation_mechanism", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn divergent_formulation_gaps() -> anyhow::Result<()> {
    init();
    let obj = objective();
    let out = DivergentFormulationGapsAgent::default()
        .await
        .run_immed(&obj.core_goal, &obj.key_constraints)
        .await?;
    dump("divergent_formulation_gaps", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn formulation_enhancer() -> anyhow::Result<()> {
    init();
    let obj = objective();
    let out = FormulationEnhancerAgent::default()
        .await
        .run_immed(formulation(), &obj)
        .await?;
    dump("formulation_enhancer", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn goal_simplifier() -> anyhow::Result<()> {
    init();
    let out = GoalSimplifierAgent::default()
        .await
        .run_immed(formulation())
        .await?;
    dump("goal_simplifier", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn requirement_search() -> anyhow::Result<()> {
    init();
    let obj = objective();
    let out = RequirementSearchAgent::default()
        .await
        .run_immed(formulation(), &obj)
        .await?;
    dump("requirement_search", &out);
    Ok(())
}

macro_rules! element_search_call {
    ($name:ident, $agent:ident, $preset:expr) => {
        #[tokio::test]
        #[ignore]
        async fn $name() -> anyhow::Result<()> {
            init();
            let req = requirement();
            let out = $agent::new($preset).await.run_immed(&req.major).await?;
            dump(stringify!($name), &out);
            Ok(())
        }
    };
}

element_search_call!(
    concept_element_search_anthropic,
    ConceptElementSearchAgent,
    Models::claude_sonnet_45()
);
element_search_call!(
    concept_element_search_openai,
    ConceptElementSearchAgent,
    Models::gpt_52()
);
element_search_call!(
    concept_element_search_google,
    ConceptElementSearchAgent,
    Models::gemini_3_5_flash()
);
element_search_call!(
    concept_element_search_xai,
    ConceptElementSearchAgent,
    Models::grok_41_fast_reasoning()
);

element_search_call!(
    entity_element_search_anthropic,
    EntityElementSearchAgent,
    Models::claude_sonnet_45()
);
element_search_call!(
    entity_element_search_openai,
    EntityElementSearchAgent,
    Models::gpt_52()
);
element_search_call!(
    entity_element_search_google,
    EntityElementSearchAgent,
    Models::gemini_3_5_flash()
);
element_search_call!(
    entity_element_search_xai,
    EntityElementSearchAgent,
    Models::grok_41_fast()
);

#[tokio::test]
#[ignore]
async fn alternative_element_search() -> anyhow::Result<()> {
    init();
    let obj = objective();
    let req = requirement();
    let out = AlternativeElementSearchAgent::default()
        .await
        .run_immed(&obj.key_constraints, "TAT peptide", &req.major)
        .await?;
    dump("alternative_element_search", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn element_merger() -> anyhow::Result<()> {
    init();
    let req = requirement();
    let els = sample_elements();
    let out = ElementMergerAgent::default()
        .await
        .run_immed(&els, &req)
        .await?;
    dump("element_merger", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn element_novelty_filter() -> anyhow::Result<()> {
    init();
    let els = sample_elements();
    let out = ElementNoveltyFilterAgent::default()
        .await
        .run_immed(&els, formulation())
        .await?;
    dump("element_novelty_filter", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn element_constraints_filter() -> anyhow::Result<()> {
    init();
    let obj = objective();
    let els = sample_elements();
    let out = ElementConstraintsFilterAgent::default()
        .await
        .run_immed(&obj.key_constraints, &els, formulation())
        .await?;
    dump("element_constraints_filter", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn element_requirement_filter() -> anyhow::Result<()> {
    init();
    let req = requirement();
    let els = sample_elements();
    let out = ElementRequirementFilterAgent::default()
        .await
        .run_immed(&els, &req.minor, formulation())
        .await?;
    dump("element_requirement_filter", &out);
    Ok(())
}

// proposal_maker has no input and continues the conversation of an upstream element_selector run.
// Each proposal_maker call therefore chains through the matching selector first.

macro_rules! element_selector_call {
    ($name:ident, $preset:expr) => {
        #[tokio::test]
        #[ignore]
        async fn $name() -> anyhow::Result<()> {
            init();
            let obj = objective();
            let els = sample_elements();
            let out = ElementSelectorAgent::new($preset)
                .await
                .run_immed(&obj.key_constraints, &obj.core_goal, &els, formulation())
                .await?;
            dump(stringify!($name), &out);
            Ok(())
        }
    };
}

element_selector_call!(element_selector_anthropic, Models::claude_sonnet_45());
element_selector_call!(element_selector_openai, Models::gpt_52());
element_selector_call!(element_selector_google, Models::gemini_3_pro_preview());
element_selector_call!(element_selector_xai, Models::grok_41_fast_reasoning());

macro_rules! proposal_maker_call {
    ($name:ident, $preset:expr) => {
        #[tokio::test]
        #[ignore]
        async fn $name() -> anyhow::Result<()> {
            init();
            let obj = objective();
            let els = sample_elements();

            let mut history = ChatHistory::default();
            let selector = ElementSelectorAgent::new($preset).await;
            let input = ElementSelectorAgentInput::new(
                &obj.key_constraints,
                &obj.core_goal,
                &els,
                formulation(),
            );
            history.run(&selector, input).await?;

            let maker = ProposalMakerAgent::new($preset).await;
            let out = history.run(&maker, ProposalMakerAgentInput::new()).await?;
            dump(stringify!($name), &out);
            Ok(())
        }
    };
}

proposal_maker_call!(proposal_maker_anthropic, Models::claude_sonnet_45());
proposal_maker_call!(proposal_maker_openai, Models::gpt_52());
proposal_maker_call!(proposal_maker_google, Models::gemini_3_pro_preview());
proposal_maker_call!(proposal_maker_xai, Models::grok_41_fast_reasoning());

#[tokio::test]
#[ignore]
async fn reviewer() -> anyhow::Result<()> {
    init();
    let submission = ReviewSubmission::new_without_statement(sample_idea(), 1);
    let out = ReviewerAgent::default()
        .await
        .run_immed(&submission)
        .await?;
    dump("reviewer", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn defender() -> anyhow::Result<()> {
    init();
    let obj = objective();
    let challenge = RefinementChallenge::new(sample_critique(), sample_idea(), 1);
    let out = DefenderAgent::default()
        .await
        .run_immed(
            &challenge,
            &obj.core_goal,
            &obj.key_constraints,
            formulation(),
        )
        .await?;
    dump("defender", &out);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn judge() -> anyhow::Result<()> {
    init();
    let obj = objective();
    let out = JudgeAgent::default()
        .await
        .run_immed(
            sample_idea(),
            sample_critique(),
            sample_defense(),
            &obj.key_constraints,
        )
        .await?;
    dump("judge", &out);
    Ok(())
}
