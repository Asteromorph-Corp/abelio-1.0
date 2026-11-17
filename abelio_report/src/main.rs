use std::fs;
use std::path::{Path, PathBuf};

use abelio_report::{initialize, summarize, write_report};
use chrono::Utc;
use clap::Parser;
use resm::prelude::*;

const OUTPUT_DIRECTORY: &str = "output";

#[derive(Parser, Debug)]
struct Args {
    input: Vec<PathBuf>,
}

fn output_dir() -> eyre::Result<PathBuf> {
    let now = Utc::now();
    let seoul = now.with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).unwrap());
    let path = Path::new(OUTPUT_DIRECTORY).join(seoul.format("%Y-%m-%d_%H-%M-%S").to_string());
    fs::create_dir_all(&path)?;
    Ok(path)
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let store = initialize()?;
    let store = store.into_arced();

    let args = Args::parse();
    let out = output_dir()?;

    if args.input.is_empty() {
        tracing::warn!("No input provided");
        return Ok(());
    }

    for (idx, path) in args.input.iter().enumerate() {
        let input = fs::read_to_string(path)?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("{idx:0>3}"));
        let subdir = out.join(format!("{idx:0>3}_{stem}"));
        fs::create_dir_all(&subdir)?;

        let scoped_store = store.clone().scoped(format!("task/{:0>2}", idx + 1));

        tracing::info!(stem, "Starting");

        let report = write_report(&input, &scoped_store, &None).await?;
        let summary = summarize(&report, &scoped_store, &None).await?;

        fs::write(subdir.join("report.md"), &report)?;
        fs::write(subdir.join("summary.md"), &summary)?;

        tracing::info!(stem, "Done");
    }

    Ok(())
}
