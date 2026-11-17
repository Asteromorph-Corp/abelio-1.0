use crate::agents::{
    ElementSelectorAgent, ElementSelectorAgentInput, Models, ProposalMakerAgent,
    ProposalMakerAgentInput,
};
use crate::config::PipelineConfig;
use crate::types::{Elements, Objective};
use crate::utils::{ChatHistory, sample_by_ratio};

/// The model that ideates each idea.
const IDEATION_MODEL: &str = "gpt_5";

/// Each formulation yields at most one idea.
pub(crate) const IDEAS_PER_FORMULATION: usize = 1;

async fn ideate_once(
    config: &PipelineConfig,
    formulation: &str,
    objective: &Objective,
    elements: &Elements,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<String> {
    let preset = Models::any(IDEATION_MODEL)?;
    let idea: String = resm::cached_text!(
        store,
        "c_idea",
        deps: &(
            (ElementSelectorAgent.fingerprint(), ProposalMakerAgent.fingerprint()),
            IDEATION_MODEL,
            formulation,
            objective,
            elements,
            config.sample_ratio.to_bits(),
        ),
        {
            let elements: Elements =
                sample_by_ratio(elements.clone().into(), config.sample_ratio).into();

            let element_selector = ElementSelectorAgent::any(preset.clone()).await;
            let proposal_maker = ProposalMakerAgent::any(preset).await;

            let element_selector_input = ElementSelectorAgentInput::new(
                &objective.key_constraints,
                &objective.core_goal,
                &elements,
                formulation,
            );

            // The proposal maker continues the element selector's conversation.
            let mut history = ChatHistory::default();
            history
                .run(&element_selector, element_selector_input)
                .await?;
            let idea = history
                .run(&proposal_maker, ProposalMakerAgentInput::new())
                .await?;
            Ok::<_, anyhow::Error>(idea)
        }
    )?;

    Ok(idea)
}

#[tracing::instrument(skip_all)]
pub async fn ideate_with_elements(
    config: &PipelineConfig,
    formulation: &str,
    objective: &Objective,
    elements: &Elements,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Vec<String>> {
    let idea = ideate_once(config, formulation, objective, elements, store)
        .await
        .inspect_err(|err| tracing::warn!(error = %err, "ideation failed"))
        .ok();

    Ok(idea.into_iter().collect())
}
