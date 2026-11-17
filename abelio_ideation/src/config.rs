use serde::Deserialize;

/// Configuration for the ideation pipeline.
///
/// Controls formulation count, element discovery parameters, refinement rounds, and concurrency
/// limits for each pipeline stage. All fields have sensible defaults (see [`Default`] impl) and can
/// be overridden via TOML with [`PipelineConfig::from_toml`].
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    pub num_formulations: usize,
    pub entity_entries: usize,
    pub sample_ratio: f64,
    pub refinement: RefinementConfig,
    pub concurrency: ConcurrencyConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RefinementConfig {
    /// Total rounds including finalization (e.g. 7 = 6 review rounds + 1 finalization)
    pub max_rounds: usize,
    /// Rounds before the judge starts evaluating
    pub warmup_rounds: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ConcurrencyConfig {
    pub formulations: usize,
    pub refinement: usize,
    pub formulation_enhance: usize,
    pub element_selection_alternative_search: usize,
    pub rerank: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            num_formulations: 10,
            entity_entries: 8,
            sample_ratio: 0.75,
            refinement: RefinementConfig::default(),
            concurrency: ConcurrencyConfig::default(),
        }
    }
}

impl Default for RefinementConfig {
    fn default() -> Self {
        Self {
            max_rounds: 7,
            warmup_rounds: 2,
        }
    }
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            formulations: 256,
            refinement: 256,
            formulation_enhance: 256,
            element_selection_alternative_search: 256,
            rerank: 256,
        }
    }
}

impl std::hash::Hash for RefinementConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.max_rounds.hash(state);
        self.warmup_rounds.hash(state);
    }
}

impl PipelineConfig {
    pub fn from_toml(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}
