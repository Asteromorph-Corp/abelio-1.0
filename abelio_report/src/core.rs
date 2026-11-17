use std::collections::HashMap;
use std::pin::Pin;

use resm::prelude::*;
use talin::agent::AgentOptions;
use talin::strategy::{DropThoughtStrategy, SharedTagStrategy};

use crate::agents::{
    PathfinderAgent, PathfinderAgentExt, ReportWriterAgent, ReportWriterAgentExt, SummarizerAgent,
    SummarizerAgentExt,
};
use crate::progress::{self, Progress, SharedProgress};
use crate::types::{OutputWithCitations, PathfinderAgentOutput, ReportWriterAgentOutput};
use crate::utils::{
    extract_lowercase_in_brackets, extract_title_doi, substitute_lowercase_in_brackets,
    take_citations,
};

#[tracing::instrument(skip_all)]
pub async fn write_report(
    input: &str,
    store: &(impl Store + Clone),
    progress: &Option<SharedProgress>,
) -> eyre::Result<String> {
    // Erased so that a caller's `Send` check does not have to expand the agent future types, which
    // overflows the trait solver once combined with other long futures (E0275).
    let erased: Pin<Box<dyn Future<Output = _> + Send + '_>> =
        Box::pin(write_report_inner(input, store, progress));
    erased.await
}

async fn write_report_inner(
    input: &str,
    store: &(impl Store + Clone),
    progress: &Option<SharedProgress>,
) -> eyre::Result<String> {
    tracing::info!("Generating report...");
    let mut citations = HashMap::new();

    let report = write_draft(input, store).await?;
    progress::send(progress, Progress::DraftComplete);
    let report_structured = &report.output;

    let report_content = format!(
        "# Concept: {}\n\n{}\n\n## Background\n\n{}\n\n# Operating Principles\n\n{}\n\n",
        report_structured.concept_title,
        report_structured.concept,
        report_structured.background,
        report_structured.operating_principles
    );

    let report_citations = take_citations(&report_content, &report.citations)?;
    citations.extend(report.citations.clone());

    let report_with_citations = format!(
        "{}\n\n---\n\n# References\n{}",
        report_content,
        serde_json::to_string(&report_citations)?
    );

    let research_plan = plan_research(&report_with_citations, store).await?;
    progress::send(progress, Progress::PlanComplete);
    let research_plan_structured = &research_plan.output;
    citations.extend(research_plan.citations.clone());

    let mut final_citations =
        take_citations(&research_plan_structured.execution_plans, &citations)?;
    final_citations.extend(report_citations);

    let merged_content = merge_reports_structured(
        report_structured,
        research_plan_structured,
        &final_citations,
    )?;
    let merged_content = merged_content.replace("<", "\\<").replace(">", "\\>");

    tracing::info!("Report generated.");
    Ok(merged_content)
}

#[tracing::instrument(skip_all)]
pub async fn summarize(
    input: &str,
    store: &(impl Store + Clone),
    progress: &Option<SharedProgress>,
) -> eyre::Result<String> {
    let erased: Pin<Box<dyn Future<Output = _> + Send + '_>> =
        Box::pin(summarize_inner(input, store, progress));
    erased.await
}

async fn summarize_inner(
    input: &str,
    store: &(impl Store + Clone),
    progress: &Option<SharedProgress>,
) -> eyre::Result<String> {
    let summary: String = resm::cached_text!(
        store.clone(),
        "summary",
        deps: &(SummarizerAgent.fingerprint(), input),
        {
            // Every agent here drops thoughts, as in the build used for the paper runs.
            SummarizerAgent::default()
                .await
                .with_options(AgentOptions::default().with_max_consecutive_fails(10))
                .run_with_strategy_immed(DropThoughtStrategy, input)
                .await
                .map(|output| output.content)
                .map_err(|e| eyre::eyre!(e))
        }
    )?;

    progress::send(progress, Progress::SummaryComplete);
    tracing::info!("Summary generated.");
    Ok(summary)
}

#[tracing::instrument(skip_all)]
async fn write_draft(
    input: &str,
    store: &(impl Store + Clone),
) -> eyre::Result<OutputWithCitations<ReportWriterAgentOutput>> {
    let result: OutputWithCitations<ReportWriterAgentOutput> = resm::cached_json!(
        store.clone(),
        "draft",
        deps: &(ReportWriterAgent.fingerprint(), input),
        {
            let strategy = SharedTagStrategy::default();
            let agent = ReportWriterAgent::default()
                .await
                .with_options(AgentOptions::default().with_max_consecutive_fails(10));

            let report = agent
                .run_with_strategy_immed((strategy.clone(), DropThoughtStrategy), input)
                .await
                .map_err(|e| eyre::eyre!(e))?;

            eyre::Ok(OutputWithCitations {
                output: report.content,
                citations: report.extra_content.0.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            })
        }
    )?;

    Ok(result)
}

#[tracing::instrument(skip_all)]
async fn plan_research(
    input: &str,
    store: &(impl Store + Clone),
) -> eyre::Result<OutputWithCitations<PathfinderAgentOutput>> {
    let result: OutputWithCitations<PathfinderAgentOutput> = resm::cached_json!(
        store.clone(),
        "research_plan",
        deps: &(PathfinderAgent.fingerprint(), input),
        {
            let strategy = SharedTagStrategy::default();
            let agent = PathfinderAgent::default()
                .await
                .with_options(AgentOptions::default().with_max_consecutive_fails(10));

            let output = agent
                .run_with_strategy_immed((strategy.clone(), DropThoughtStrategy), input)
                .await
                .map_err(|e| eyre::eyre!(e))?;

            eyre::Ok(OutputWithCitations {
                output: output.content,
                citations: output.extra_content.0.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            })
        }
    )?;

    Ok(result)
}

fn merge_reports_structured(
    report: &ReportWriterAgentOutput,
    research_plan: &PathfinderAgentOutput,
    reference_list: &HashMap<String, String>,
) -> eyre::Result<String> {
    let mut merged = String::new();

    merged.push_str(&format!("# Concept: {}\n\n", report.concept_title));
    merged.push_str(&report.concept);
    merged.push_str("\n\n");

    merged.push_str("## Background\n\n");
    merged.push_str(&report.background);
    merged.push_str("\n\n");

    merged.push_str("# Operating Principles\n\n");
    merged.push_str(&report.operating_principles);
    merged.push_str("\n\n");

    merged.push_str("# Execution Plans\n\n");
    merged.push_str(&research_plan.execution_plans);
    merged.push_str("\n\n");

    merged.push_str("---\n");

    let mut substituted = substitute_lowercase_in_brackets(&merged)?;

    let tags = extract_lowercase_in_brackets(&merged)?;

    for tag in tags {
        if let Some(ref_info) = reference_list.get(&tag) {
            match extract_title_doi(ref_info) {
                (Some(title), Some(doi)) => {
                    substituted.push_str(&format!("\n[^{}]: {}. {}\n", tag, title, doi));
                }
                (Some(title), None) => {
                    substituted.push_str(&format!("\n[^{}]: {}\n", tag, title));
                }
                (None, Some(doi)) => {
                    substituted.push_str(&format!("\n[^{}]: {}\n", tag, doi));
                }
                (None, None) => {
                    substituted = substituted.replace(&format!("[^{}]", tag), "");
                }
            }
        } else {
            substituted = substituted.replace(&format!("[^{}]", tag), "");
        }
    }

    Ok(substituted)
}
