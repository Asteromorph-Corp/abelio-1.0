//! Compile-only guard: the ideate then write_report/summarize flow must stay spawnable on tokio
//! without a raised `recursion_limit`.

use abelio_ideation::{Objective, PipelineConfig};
use abelio_report::resm::Store;
use futures::future::join_all;

async fn execute(objective: String, constraints: Vec<String>) -> anyhow::Result<Vec<String>> {
    let objective = Objective {
        core_goal: objective,
        key_constraints: constraints.into(),
    };
    let ideation_store = abelio_ideation::resm::backend::MemoryStore::new();
    let hypotheses = abelio_ideation::ideate(
        &PipelineConfig::default(),
        &objective,
        &ideation_store,
        &None,
    )
    .await?;
    let hypotheses = hypotheses
        .into_iter()
        .flat_map(|(_, h)| h)
        .collect::<Vec<_>>();
    let report_store = abelio_report::resm::backend::MemoryStore::new();
    let results = join_all(
        hypotheses
            .into_iter()
            .enumerate()
            .map(|(i, h)| report(i, h, report_store.clone())),
    )
    .await;
    Ok(results.into_iter().flatten().collect())
}

async fn report(
    index: usize,
    hypothesis: String,
    store: impl Store + Clone,
) -> anyhow::Result<String> {
    let report = abelio_report::write_report(&hypothesis, &store, &None)
        .await
        .map_err(|e| anyhow::anyhow!("report {index}: {e}"))?;
    let summary = abelio_report::summarize(&report, &store, &None)
        .await
        .map_err(|e| anyhow::anyhow!("summary {index}: {e}"))?;
    Ok(format!("{report}{summary}"))
}

#[allow(dead_code)]
fn spawns_without_recursion_limit() {
    tokio::spawn(execute(String::new(), Vec::new()));
}
