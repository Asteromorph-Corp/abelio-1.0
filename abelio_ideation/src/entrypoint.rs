use std::pin::Pin;

use futures::{StreamExt, stream};
use resm::prelude::*;

use crate::config::PipelineConfig;
use crate::element_selection::discover_elements;
use crate::formulation::formulate_objective;
use crate::ideation::{self, ideate_with_elements};
use crate::progress::{self, Progress, SharedProgress};
use crate::refinement::refine_idea;
use crate::types::Objective;

/// Run the full ideation pipeline for a given objective.
///
/// Executes four stages in sequence for each formulation:
/// 1. **Formulation** — generate divergent goal re-framings of the objective.
/// 2. **Element selection** — discover concept and entity elements relevant to each formulation.
/// 3. **Ideation** — produce research ideas from the discovered elements.
/// 4. **Refinement** — refine each idea through a multi-round critique/defend/judge loop.
///
/// Returns a list of `(formulation, refined_ideas)` pairs. Formulations whose processing fails are
/// silently dropped.
///
/// Pass a [`SharedProgress`] to receive real-time progress events, or `None` to run without
/// progress reporting.
#[tracing::instrument(skip_all, fields(num_formulations = config.num_formulations))]
pub async fn ideate(
    config: &PipelineConfig,
    objective: &Objective,
    store: &(impl resm::Store + Clone),
    progress: &Option<SharedProgress>,
) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    // Erased so that a caller's `Send` check does not have to expand the whole pipeline type, which
    // overflows the trait solver once combined with other long futures (E0275).
    let erased: Pin<Box<dyn Future<Output = _> + Send + '_>> =
        Box::pin(run_pipeline(config, objective, store, progress));
    erased.await
}

async fn run_pipeline(
    config: &PipelineConfig,
    objective: &Objective,
    store: &(impl resm::Store + Clone),
    progress: &Option<SharedProgress>,
) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    // a. formulation
    progress::send(progress, Progress::FormulationStarted);
    let formulation_store = store.clone().scoped("a_formulation");
    let mut formulations = vec![objective.core_goal.clone(); 1];
    formulations.extend(
        formulate_objective(
            config,
            objective,
            config.num_formulations - 1,
            &formulation_store,
        )
        .await?,
    );
    tracing::debug!(count = formulations.len(), "formulations generated");
    progress::send(
        progress,
        Progress::FormulationComplete {
            count: formulations.len(),
            ideas_per_formulation: ideation::IDEAS_PER_FORMULATION,
        },
    );

    // b-d. process each formulation
    let total = formulations.len();
    let stores: Vec<_> = (0..total)
        .map(|idx| store.clone().scoped(format!("formulation_{idx:02}")))
        .collect();
    let progresses: Vec<_> = (0..total).map(|_| progress.clone()).collect();
    let mut futs = Vec::new();
    for (idx, formulation) in formulations.iter().enumerate() {
        futs.push(process_single_formulation(
            config,
            formulation,
            objective,
            &stores[idx],
            &progresses[idx],
            idx,
            total,
        ));
    }
    let all_ideas: Vec<Option<Vec<String>>> = stream::iter(futs)
        .buffered(config.concurrency.formulations)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|res| {
            res.inspect_err(|err| tracing::warn!(error = %err, "formulation processing failed"))
                .ok()
        })
        .collect();

    progress::send(progress, Progress::PipelineComplete);

    Ok(formulations
        .into_iter()
        .zip(all_ideas)
        .filter_map(|(formulation, ideas_opt)| ideas_opt.map(|ideas| (formulation, ideas)))
        .collect())
}

#[tracing::instrument(skip_all, fields(formulation = %formulation))]
async fn process_single_formulation(
    config: &PipelineConfig,
    formulation: &str,
    objective: &Objective,
    store: &(impl resm::Store + Clone),
    progress: &Option<SharedProgress>,
    index: usize,
    total: usize,
) -> anyhow::Result<Vec<String>> {
    progress::send(
        progress,
        Progress::FormulationProcessingStarted { index, total },
    );

    // b. element selection
    let elements_store = store.clone().scoped("b_element_selection");
    let elements = discover_elements(config, formulation, objective, &elements_store).await?;
    tracing::debug!(count = elements.len(), "elements discovered");
    progress::send(
        progress,
        Progress::ElementSelectionComplete {
            index,
            count: elements.len(),
        },
    );

    // c. ideation
    let ideation_store = store.clone().scoped("c_ideation");
    let ideas =
        ideate_with_elements(config, formulation, objective, &elements, &ideation_store).await?;
    tracing::debug!(count = ideas.len(), "ideas generated");
    progress::send(
        progress,
        Progress::IdeationComplete {
            index,
            count: ideas.len(),
        },
    );

    // d. refinement
    let idea_count = ideas.len();
    let refinement_stores: Vec<_> = (0..idea_count)
        .map(|idx| store.clone().scoped(format!("idea_{idx:02}/d_refinement")))
        .collect();
    let mut futs = Vec::new();
    for (idx, idea) in ideas.iter().enumerate() {
        futs.push(refine_idea(
            config,
            idea,
            formulation,
            objective,
            &refinement_stores[idx],
        ));
    }
    let ideas: Vec<String> = stream::iter(futs)
        .buffered(config.concurrency.refinement)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(|res| {
            progress::send(progress, Progress::IdeaRefined { index });
            res.inspect_err(|err| tracing::warn!(error = %err, "idea refinement failed"))
                .ok()
                .flatten()
        })
        .collect();
    tracing::debug!(count = ideas.len(), "ideas refined");
    progress::send(
        progress,
        Progress::RefinementComplete {
            index,
            accepted: ideas.len(),
            total: idea_count,
        },
    );

    Ok(ideas)
}
