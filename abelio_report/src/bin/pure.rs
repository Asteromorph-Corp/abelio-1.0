use std::fs;
use std::path::PathBuf;

use abelio_report::{initialize, write_report};
use clap::Parser;
use resm::prelude::*;

#[derive(Parser)]
struct Args {
    /// Input file path.
    input: PathBuf,
    /// Output file path.
    output: PathBuf,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let store = initialize()?;
    let store = store.into_arced();

    let args = Args::parse();
    let input = fs::read_to_string(&args.input)?;

    let report = write_report(&input, &store, &None).await?;
    fs::write(&args.output, &report)?;

    Ok(())
}
