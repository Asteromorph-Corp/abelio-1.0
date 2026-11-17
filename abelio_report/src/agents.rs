use science_search::ArticleSearchTool;

use crate::types::{PathfinderAgentOutputCtor, ReportWriterAgentOutputCtor};

talin::include_models!();
talin::include_agents!();

#[cfg(test)]
mod prompt_golden;
