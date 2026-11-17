mod concept;
mod entity;
mod filter;
pub mod types;

use resm::prelude::*;

use self::concept::discover_concept_elements;
use self::entity::discover_entity_elements;
use self::filter::filter_elements;
use crate::agents::{RequirementSearchAgent, RequirementSearchAgentInput};
use crate::config::PipelineConfig;
use crate::types::{Elements, Objective, Requirement};
use crate::utils::ChatHistory;

#[tracing::instrument(skip_all, fields(formulation = %formulation))]
pub async fn discover_elements(
    config: &PipelineConfig,
    formulation: &str,
    objective: &Objective,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Elements> {
    // a. Search for requirement
    let requirement: Requirement = resm::cached_json!(
        store,
        "a_requirements",
        deps: &(RequirementSearchAgent.fingerprint(), formulation, objective),
        {
            let agent = RequirementSearchAgent::default().await;
            ChatHistory::run_once(&agent, RequirementSearchAgentInput::new(formulation, objective))
                .await
        }
    )?;

    // a1 and a2. Concept and Entity Element Generation
    let concept_store = store.clone().scoped("a1_concept");
    let entity_store = store.clone().scoped("a2_entity");

    let (concept_elements, entity_elements) = tokio::join!(
        discover_concept_elements(&requirement, &concept_store),
        discover_entity_elements(
            config,
            &requirement,
            &objective.key_constraints,
            &entity_store
        )
    );

    let concept_elements = concept_elements.unwrap_or_default();
    let entity_elements = entity_elements.unwrap_or_default();

    // b1 and b2. Filter Elements
    let concept_filter_store = store.clone().scoped("b1_concept_filter");
    let entity_filter_store = store.clone().scoped("b2_entity_filter");
    let (concept_elements, entity_elements) = tokio::join!(
        filter_elements(
            formulation,
            &objective.key_constraints,
            &requirement,
            &concept_elements,
            &concept_filter_store
        ),
        filter_elements(
            formulation,
            &objective.key_constraints,
            &requirement,
            &entity_elements,
            &entity_filter_store
        )
    );

    let concept_elements = concept_elements.unwrap_or_default();
    let entity_elements = entity_elements.unwrap_or_default();

    let mut elements = concept_elements;
    elements.extend(entity_elements);
    Ok(elements)
}

#[cfg(test)]
mod tests {
    use resm::backend::MemoryStore;

    use crate::agents::{EntityElementSearchAgent, EntityElementSearchAgentExt, Models};
    use crate::config::PipelineConfig;
    use crate::types::Objective;

    #[tokio::test]
    #[ignore = "live: calls the LLM provider APIs"]
    async fn test_sonnet_entity_search() {
        dotenvy::dotenv().ok();
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();

        let elements = EntityElementSearchAgent::new(Models::gemini_3_5_flash())
            .await
            .run_immed("Development of Cas9 nickase fused to rolling-circle polymerase functional in mammalian cells.")
            .await
            .unwrap()
            .elements;

        println!("Elements: {:?}", elements);
    }

    #[tokio::test]
    #[ignore = "live: calls the LLM provider APIs"]
    async fn test_element_pipeline() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        dotenvy::dotenv().ok();
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();

        let config = PipelineConfig::default();
        let store = MemoryStore::new();
        let objective = Objective {
            core_goal: "Development of Cas9 nickase fused to rolling-circle polymerase functional in mammalian cells.".to_string(),
            key_constraints: vec![
                "Must be compatible with mammalian cellular environment.".to_string(),
                "Should maintain high specificity for target DNA sequences.".to_string(),
                "Needs to demonstrate efficient rolling-circle amplification.".to_string(),
            ].into(),
        };

        let req_els = super::discover_elements(
            &config,
            "Think about developing a Cas9 nickase fused to a rolling-circle polymerase that functions effectively within mammalian cells.",
            &objective,
            &store,
        )
        .await?;

        println!("Requirement Elements: {:?}", req_els);

        Ok(())
    }
}
