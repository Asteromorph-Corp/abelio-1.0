use futures::{StreamExt, stream};
use resm::prelude::*;

use super::types::Requirement;
use crate::agents::{
    ConceptElementSearchAgent, ConceptElementSearchAgentInput, ElementMergerAgent,
    ElementMergerAgentInput, Models,
};
use crate::types::Elements;
use crate::utils::ChatHistory;

/// The [`Models`] the concept elements are searched with.
const CONCEPT_BACKENDS: [&str; 2] = ["gpt_51", "gemini_3_5_flash"];

#[tracing::instrument(skip_all)]
pub async fn discover_concept_elements(
    requirement: &Requirement,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Elements> {
    // a. Search concept elements
    let search_store = store.clone().scoped("a_search");
    let elements = collect_raw_elements(requirement, &search_store).await?;
    // b. Refine concept elements
    let refine_store = store.clone().scoped("b_refinement");
    let elements = distill_elements(requirement, &elements, &refine_store).await?;

    Ok(elements)
}

async fn collect_raw_elements(
    requirement: &Requirement,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Elements> {
    let mut futs = Vec::new();
    for (idx, backend) in CONCEPT_BACKENDS.into_iter().enumerate() {
        let store = store.clone();
        let fut = async move {
            let preset = Models::any(backend)?;
            // we cache error results here as well to avoid repeated failed calls
            let elements: Option<Elements> = resm::cached_json!(
                store,
                format!("a_concept_elements_partial_{idx}"),
                deps: &(ConceptElementSearchAgent.fingerprint(), backend, requirement),
                {
                    let agent = ConceptElementSearchAgent::any(preset).await;
                    let input = ConceptElementSearchAgentInput::new(requirement.major.as_str());
                    ChatHistory::run_once(&agent, input)
                        .await
                        .map(|found| found.elements)
                        .inspect_err(|err| tracing::warn!(error = %err, "concept element search failed"))
                        .ok()
                }
            )?;
            anyhow::Ok(elements)
        };
        futs.push(fut);
    }

    let elements = stream::iter(futs)
        .buffered(usize::MAX)
        .filter_map(|res| async move {
            res.inspect_err(
                |err| tracing::warn!(error = %err, "unknown error on concept element search"),
            )
            .ok()
            .flatten()
        })
        .flat_map(stream::iter)
        .collect::<Elements>()
        .await;

    Ok(elements)
}

async fn distill_elements(
    requirement: &Requirement,
    elements: &Elements,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Elements> {
    let elements: Elements = resm::cached_json!(
        store,
        "b_refined_concept_elements",
        deps: &(ElementMergerAgent.fingerprint(), elements, requirement),
        {
            let agent = ElementMergerAgent::default().await;
            ChatHistory::run_once(&agent, ElementMergerAgentInput::new(elements, requirement))
                .await
                .map(|r| r.distilled)
        }
    )?;

    Ok(elements)
}
