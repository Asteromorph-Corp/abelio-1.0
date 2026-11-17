//! # Abelio ideation
//!
//! LLM-powered ideation pipeline that transforms a research objective into refined ideas.
//!
//! The pipeline runs four stages for each formulation of the objective:
//!
//! 1. **Formulation** — reframe the objective into divergent goal perspectives.
//! 2. **Element selection** — discover concept and entity building blocks relevant to each
//!    formulation.
//! 3. **Ideation** — synthesize research ideas from the discovered elements.
//! 4. **Refinement** — refine each idea through multi-round critique / defend / judge loops.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use abelio_ideation::{Objective, PipelineConfig, ideate, initialize};
//! use resm::backend::{FileStore, RedbStore};
//! use resm::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     initialize();
//!
//!     let config = PipelineConfig::from_toml("pipeline.toml")?;
//!     let objective = Objective {
//!         core_goal: "Your research goal here".into(),
//!         key_constraints: vec!["constraint 1".into()].into(),
//!     };
//!     let cache = RedbStore::new(".cache/db")?;
//!     let view = FileStore::new(".cache/artifacts")?;
//!     let store = cache.fanout(view);
//!
//!     let results = ideate(&config, &objective, &store, &None).await?;
//!     for (formulation, ideas) in &results {
//!         println!("{formulation}: {} ideas", ideas.len());
//!     }
//!     Ok(())
//! }
//! ```

mod agents;
mod config;
mod element_selection;
mod entrypoint;
mod formulation;
mod ideation;
mod progress;
mod refinement;
mod tools;
mod types;
mod utils;

pub mod tei {
    tonic::include_proto!("tei.v1");
}

pub use config::PipelineConfig;
pub use entrypoint::ideate;
pub use progress::{Progress, ProgressHandle};
pub use resm;
pub use types::Objective;

#[doc(hidden)]
pub mod testing {
    pub use crate::agents::*;
    pub use crate::types::*;
    pub use crate::utils::ChatHistory;
}

/// Initialize logging and environment. Call once at startup.
#[cfg(feature = "cli")]
pub fn initialize() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}
