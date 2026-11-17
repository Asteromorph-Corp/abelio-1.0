use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use talin::chain::{ChainOutput, Ctor};

/// Structured output from the report writer agent.
#[derive(Debug, Clone, Serialize, Deserialize, ChainOutput, Ctor)]
#[chain(derive_schema)]
#[serde(deny_unknown_fields)]
pub struct ReportWriterAgentOutput {
    /// Brief title describing the core research concept.
    pub concept_title: String,
    /// Single paragraph describing the novel research idea with scientific rationales.
    pub concept: String,
    /// Background context and how this idea addresses current limitations.
    pub background: String,
    /// Detailed mechanistic explanation at the molecular/technical level.
    pub operating_principles: String,
}

/// Structured output from the pathfinder evaluation agent.
#[derive(Debug, Clone, Serialize, Deserialize, ChainOutput, Ctor)]
#[chain(derive_schema)]
#[serde(deny_unknown_fields)]
pub struct PathfinderAgentOutput {
    /// Evaluation framework in markdown: criterion, mechanism elements, and orchestration plan.
    pub execution_plans: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputWithCitations<T = String> {
    pub output: T,
    pub citations: HashMap<String, String>,
}
