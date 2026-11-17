use futures::{StreamExt, stream};
use resm::prelude::*;

use super::types::Requirement;
use crate::agents::{
    AlternativeElementSearchAgent, AlternativeElementSearchAgentInput, EntityElementSearchAgent,
    EntityElementSearchAgentInput, Models,
};
use crate::config::PipelineConfig;
use crate::types::{Constraints, Elements};
use crate::utils::{ChatHistory, cluster_then_pick_k, initialize_embed_client};

/// The [`Models`] the entity elements are searched with.
const ENTITY_BACKENDS: [&str; 2] = ["gpt_51", "gemini_3_5_flash"];

#[tracing::instrument(skip_all)]
pub async fn discover_entity_elements(
    config: &PipelineConfig,
    requirement: &Requirement,
    constraints: &Constraints,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Elements> {
    // a. Search for entity elements
    let search_store = store.clone().scoped("a_search");
    let elements = collect_raw_elements(requirement, &search_store).await?;

    // b. Refine entity elements
    let refine_store = store.clone().scoped("b_refinement");
    let elements =
        optimize_entity_elements(config, requirement, &elements, constraints, &refine_store)
            .await?;

    Ok(elements)
}

async fn collect_raw_elements(
    requirement: &Requirement,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Elements> {
    let mut futs = Vec::new();

    for (idx, backend) in ENTITY_BACKENDS.into_iter().enumerate() {
        let store = store.clone();
        futs.push(async move {
            let preset = Models::any(backend)?;
            // we cache error results here as well to avoid repeated failed calls
            let elements: Option<Elements> = resm::cached_json!(
                store,
                format!("a_entity_{idx}"),
                deps: &(EntityElementSearchAgent.fingerprint(), backend, requirement),
                {
                    let agent = EntityElementSearchAgent::any(preset).await;
                    let input = EntityElementSearchAgentInput::new(requirement.major.as_str());
                    ChatHistory::run_once(&agent, input)
                        .await
                        .map(|found| found.elements)
                        .inspect_err(|err| tracing::warn!(error = %err, "entity element search failed"))
                        .ok()
                }
            )?;
            anyhow::Ok(elements)
        });
    }

    let elements = stream::iter(futs)
        .buffered(usize::MAX)
        .filter_map(|res| async move {
            res.inspect_err(
                |err| tracing::warn!(error = %err, "unknown error on entity element search"),
            )
            .ok()
            .flatten()
        })
        .flat_map(stream::iter)
        .collect::<Elements>()
        .await;

    Ok(elements)
}

async fn optimize_entity_elements(
    config: &PipelineConfig,
    requirement: &Requirement,
    elements: &Elements,
    constraints: &Constraints,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Elements> {
    let alt_store = store.clone().scoped("a_alternatives");
    let candidates =
        explore_alternative_candidates(config, elements, requirement, constraints, &alt_store)
            .await?;

    let all_candidates: Vec<String> = candidates.into_iter().flatten().collect();

    let elements: Elements = resm::cached_json!(
        store,
        "b_selected_elements",
        deps: &(&all_candidates, &requirement.major, config.entity_entries),
        {
            let client = initialize_embed_client().await?;
            let selected = cluster_then_pick_k(
                &all_candidates,
                &requirement.major,
                config.entity_entries,
                1,
                config.concurrency.rerank,
                client,
            )
            .await?;
            Ok::<Elements, anyhow::Error>(selected.into())
        }
    )?;

    Ok(elements)
}

async fn explore_alternative_candidates(
    config: &PipelineConfig,
    elements: &Elements,
    requirement: &Requirement,
    constraints: &Constraints,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Vec<Vec<String>>> {
    let fp = AlternativeElementSearchAgent.fingerprint();
    let mut futs = Vec::new();
    for (idx, element) in elements.iter().enumerate() {
        let store = store.clone();
        let element = element.clone();
        let requirement = requirement.clone();
        let constraints = constraints.clone();
        futs.push(async move {
            resm::cached_json!(
                store,
                format!("alt_search_{idx:02}"),
                deps: &(fp, &element, &constraints, &requirement.major),
                {
                    let agent = AlternativeElementSearchAgent::default().await;
                    let input = AlternativeElementSearchAgentInput::new(
                        &constraints,
                        &element,
                        &requirement.major,
                    );
                    ChatHistory::run_once(&agent, input)
                        .await
                        .inspect_err(
                            |err| tracing::warn!(error = %err, "alternative element search failed"),
                        )
                        .map(|res| res.entities)
                        .unwrap_or_default()
                }
            )
        });
    }

    let candidates = stream::iter(futs)
        .buffered(config.concurrency.element_selection_alternative_search)
        .filter_map(|res| async move { res.ok() })
        .collect::<Vec<Vec<String>>>()
        .await;

    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use crate::config::PipelineConfig;
    use crate::types::Requirement;

    #[tokio::test]
    #[ignore = "live: calls the LLM provider, TEI and reranker APIs"]
    async fn test_entity_element_search() {
        dotenvy::dotenv().ok();
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();

        let config = PipelineConfig::default();
        let requirement = Requirement {
            major: "a company that produces electric cars".to_string(),
            minor: vec![].into(),
        };
        let constraints = vec![
            "The entity should be a company, not a product or a technology.".to_string(),
            "The entity should be currently active in the market.".to_string(),
        ]
        .into();
        let store = resm::backend::MemoryStore::new();

        let elements = super::discover_entity_elements(&config, &requirement, &constraints, &store)
            .await
            .unwrap();

        println!("Discovered elements: {:#?}", elements);
    }
}
