mod agents;
mod core;
mod progress;
mod types;
mod utils;

pub use core::{summarize, write_report};

pub use progress::{Progress, ProgressHandle};
pub use resm;

#[cfg(feature = "cli")]
pub fn initialize() -> eyre::Result<impl resm::Store> {
    use resm::StoreExt;
    use resm::backend::{FileStore, RedbStore};
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, fmt};

    color_eyre::install()?;
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cache = RedbStore::new(".cache/db")?;
    let view = FileStore::new(".cache/artifacts")?.coalesced();
    let store = cache.fanout(view);

    Ok(store)
}
