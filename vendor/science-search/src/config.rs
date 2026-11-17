use talin::tools::ConfigSchema;

/// The per-agent configuration of the search tools, set as `config` in `agents.toml`.
#[derive(Debug, Default, ConfigSchema)]
pub struct SearchToolConfig {
    /// The maximum number of results to return.
    pub max_results: usize,
}
