pub mod types;

use futures::{StreamExt, stream};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::agents::{
    DivergentFormulationGapsAgent, DivergentFormulationGapsAgentInput,
    DivergentFormulationMechanismAgent, DivergentFormulationMechanismAgentInput,
    DivergentFormulationScopeAgent, DivergentFormulationScopeAgentInput, FormulationEnhancerAgent,
    FormulationEnhancerAgentInput, GoalSimplifierAgent, GoalSimplifierAgentInput,
};
use crate::config::PipelineConfig;
use crate::types::Objective;
use crate::utils::{ChatHistory, cluster_then_pick_k, initialize_embed_client};

#[derive(Serialize, Deserialize, Hash)]
struct FormulationIntermediate {
    divergent_goal: String,
    enhanced_goal: String,
    simplified_goal: String,
}

#[tracing::instrument(skip_all)]
async fn diverge_formulations(objective: &Objective) -> anyhow::Result<Vec<String>> {
    let scope_agent = DivergentFormulationScopeAgent::default().await;
    let mechanism_agent = DivergentFormulationMechanismAgent::default().await;
    let gaps_agent = DivergentFormulationGapsAgent::default().await;

    let (goal, constraints) = (&objective.core_goal, &objective.key_constraints);
    let (sc, mc, gp) = tokio::try_join!(
        ChatHistory::run_once(
            &scope_agent,
            DivergentFormulationScopeAgentInput::new(goal, constraints)
        ),
        ChatHistory::run_once(
            &mechanism_agent,
            DivergentFormulationMechanismAgentInput::new(goal, constraints)
        ),
        ChatHistory::run_once(
            &gaps_agent,
            DivergentFormulationGapsAgentInput::new(goal, constraints)
        ),
    )?;

    let formulations = sc
        .formulations
        .into_iter()
        .chain(mc.formulations.into_iter())
        .chain(gp.formulations.into_iter())
        .collect::<Vec<_>>();

    Ok(formulations)
}

async fn refine_and_simplify(
    divergent_goal: impl Into<String>,
    objective: &Objective,
) -> anyhow::Result<FormulationIntermediate> {
    let divergent_goal = divergent_goal.into();

    let enhancer = FormulationEnhancerAgent::default().await;
    let enhanced_goal = ChatHistory::run_once(
        &enhancer,
        FormulationEnhancerAgentInput::new(&divergent_goal, objective),
    )
    .await?;

    let simplifier = GoalSimplifierAgent::default().await;
    let simplified_goal =
        ChatHistory::run_once(&simplifier, GoalSimplifierAgentInput::new(&enhanced_goal)).await?;

    Ok(FormulationIntermediate {
        divergent_goal,
        enhanced_goal,
        simplified_goal,
    })
}

#[tracing::instrument(skip_all, fields(num_formulations))]
pub async fn formulate_objective(
    config: &PipelineConfig,
    objective: &Objective,
    num_formulations: usize,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Vec<String>> {
    // a. Diverge Formulated Goals
    let diverge_fp = (
        DivergentFormulationScopeAgent.fingerprint(),
        DivergentFormulationMechanismAgent.fingerprint(),
        DivergentFormulationGapsAgent.fingerprint(),
    );
    let formulations: Vec<String> = resm::cached_json!(
        store,
        "a_divergent_formulated_goals",
        deps: &(diverge_fp, objective),
        {
            diverge_formulations(objective).await
        }
    )?;

    // b. Refine and Simplify Formulated Goals
    let refine_fp = (
        FormulationEnhancerAgent.fingerprint(),
        GoalSimplifierAgent.fingerprint(),
    );
    let refined: Vec<FormulationIntermediate> = resm::cached_json!(
        store,
        "b1_refined_formulated_goals",
        deps: &(refine_fp, &formulations),
        {
            let mut futs = Vec::new();
            for divergent_goal in &formulations {
                futs.push(refine_and_simplify(divergent_goal, objective));
            }
            stream::iter(futs)
                .buffered(config.concurrency.formulation_enhance)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
        }
    )?;

    let simplified: Vec<String> = resm::cached_json!(
        store,
        "b2_simplified_goals",
        deps: &(refine_fp, &formulations),
        {
            let mut simplified: Vec<String> =
                refined.into_iter().map(|eg| eg.simplified_goal).collect();
            simplified.shuffle(&mut rand::rng());
            simplified
        }
    )?;

    // c. Cluster and Select Formulated Goals
    let selected: Vec<String> = resm::cached_json!(
        store,
        "c_selected_goals",
        deps: &(&simplified, objective, num_formulations),
        {
            let client = initialize_embed_client().await?;
            let selected = cluster_then_pick_k(
                &simplified,
                serde_json::to_string(objective)?,
                num_formulations,
                1,
                config.concurrency.rerank,
                client,
            )
            .await?;

            Ok::<_, anyhow::Error>(selected)
        }
    )?;

    Ok(selected)
}

#[cfg(test)]
mod tests {
    use futures::future::join_all;
    use resm::backend::MemoryStore;
    use resm::prelude::*;

    use crate::config::PipelineConfig;
    use crate::types::Objective;
    use crate::utils::Importer;

    #[tokio::test]
    #[ignore = "live: calls the LLM provider and TEI APIs and reads input/test/parsed_goal"]
    async fn test_formulation_process() -> anyhow::Result<()> {
        use crate::formulation::formulate_objective;

        dotenvy::dotenv().ok();
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();

        let config = PipelineConfig::default();
        let store = MemoryStore::new();
        let importer = Importer::new("input/test/parsed_goal");

        let futs = importer
            .iter_dir(None)?
            .enumerate()
            .map(|(id, path)| {
                let store = store.clone().scoped(format!("entry_{:02}", id + 1));
                let config = &config;
                async move {
                    let objective = Importer::import_json_immed::<Objective>(&path)
                        .map_err(|e| anyhow::anyhow!(e))?;
                    formulate_objective(config, &objective, 12, &store).await
                }
            })
            .collect::<Vec<_>>();

        let _ = join_all(futs)
            .await
            .into_iter()
            .map(|res| match res {
                Ok(_) => (),
                Err(e) => eprintln!("Error occured: {:?}", e),
            })
            .collect::<Vec<_>>();

        Ok(())
    }
}
