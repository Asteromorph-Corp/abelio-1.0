use std::path::PathBuf;
use std::sync::Arc;

use abelio_ideation::resm::backend::{FileStore, RedbStore};
use abelio_ideation::resm::prelude::*;
use abelio_ideation::{Objective, PipelineConfig, Progress, ProgressHandle};
use clap::Parser;

/// Abelio ideation — LLM-powered ideation pipeline.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Core goal / objective for the pipeline.
    /// Mutually exclusive with --file.
    goal: Option<String>,

    /// Key constraints (repeatable). Used with positional goal.
    #[arg(short = 'k', long = "constraint")]
    constraints: Vec<String>,

    /// Read objective from a JSON file instead of CLI args.
    /// Expected format: { "core_goal": "...", "key_constraints": ["..."] }
    #[arg(short, long, conflicts_with_all = ["goal", "constraints"])]
    file: Option<PathBuf>,

    /// Path to pipeline config TOML file.
    #[arg(long, default_value = "pipeline.toml")]
    config: PathBuf,

    /// Cache directory for resm FileStore.
    #[arg(long, default_value = ".cache")]
    cache_dir: PathBuf,

    /// Output file path (markdown). Defaults to "output.md".
    #[arg(short, long, default_value = "output.md")]
    output: PathBuf,

    /// Session ID for cache scoping and resumability.
    /// Reuse a session ID to resume a previous run from cache.
    #[arg(short, long)]
    session: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    abelio_ideation::initialize();

    let config = PipelineConfig::from_toml(&cli.config)?;

    let session_id = cli.session.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_millis()
            .to_string()
    });

    let cache = RedbStore::new(cli.cache_dir.join("db"))?;
    let view = FileStore::new(cli.cache_dir.join("artifacts"))?.coalesced();
    let store = cache.fanout(view).scoped(&session_id);

    println!("Session: {session_id}");

    let objective = match (cli.goal, cli.file) {
        (Some(goal), None) => Objective {
            core_goal: goal,
            key_constraints: cli.constraints.into(),
        },
        (None, Some(path)) => {
            let content = tokio::fs::read_to_string(&path).await?;
            serde_json::from_str(&content)?
        }
        (None, None) => {
            anyhow::bail!("provide either a goal argument or --file <path>");
        }
        _ => unreachable!("clap conflicts_with_all prevents this"),
    };

    let (handle, mut events_rx, _fraction_rx) = ProgressHandle::new();
    let progress = Arc::new(handle);

    let progress_task = tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            match &event {
                Progress::FormulationStarted => println!("Generating formulations..."),
                Progress::FormulationComplete { count, .. } => {
                    println!("{count} formulations generated")
                }
                Progress::FormulationProcessingStarted { index, total } => {
                    println!("Processing formulation {}/{total}...", index + 1)
                }
                Progress::ElementSelectionComplete { index, count } => {
                    println!("  Formulation {}: {count} element(s) discovered", index + 1)
                }
                Progress::IdeationComplete { index, count } => {
                    println!("  Formulation {}: {count} idea(s) generated", index + 1)
                }
                Progress::IdeaRefined { index } => {
                    println!("  Formulation {}: refining ideas...", index + 1)
                }
                Progress::RefinementComplete {
                    index,
                    accepted,
                    total,
                } => println!(
                    "  Formulation {}: {accepted}/{total} idea(s) accepted",
                    index + 1
                ),
                Progress::PipelineComplete => println!("Pipeline complete."),
            }
        }
    });

    let results = abelio_ideation::ideate(&config, &objective, &store, &Some(progress)).await?;
    let _ = progress_task.await;

    let markdown = format_markdown(&objective, &results);
    tokio::fs::write(&cli.output, &markdown).await?;

    print_summary(&results);
    println!("Full output written to {}", cli.output.display());

    Ok(())
}

fn format_markdown(objective: &Objective, results: &[(String, Vec<String>)]) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    writeln!(out, "# Ideation Results\n").unwrap();
    writeln!(out, "## Objective\n").unwrap();
    writeln!(out, "**Goal:** {}\n", objective.core_goal).unwrap();
    if !objective.key_constraints.is_empty() {
        writeln!(out, "**Constraints:**\n").unwrap();
        for c in objective.key_constraints.iter() {
            writeln!(out, "- {c}").unwrap();
        }
        writeln!(out).unwrap();
    }

    writeln!(out, "---\n").unwrap();

    for (i, (formulation, ideas)) in results.iter().enumerate() {
        writeln!(out, "## Formulation {}: {}\n", i + 1, formulation).unwrap();
        for (j, idea) in ideas.iter().enumerate() {
            writeln!(out, "### Idea {}.{}\n", i + 1, j + 1).unwrap();
            writeln!(out, "{idea}\n").unwrap();
        }
    }

    out
}

fn print_summary(results: &[(String, Vec<String>)]) {
    let total_ideas: usize = results.iter().map(|(_, ideas)| ideas.len()).sum();
    println!(
        "\n{} formulation(s), {} idea(s) total\n",
        results.len(),
        total_ideas
    );

    for (i, (formulation, ideas)) in results.iter().enumerate() {
        // Truncate long formulation text for terminal display
        let label = truncate(formulation, 80);
        println!("  {}. {} ({} idea(s))", i + 1, label, ideas.len());

        for (j, idea) in ideas.iter().enumerate() {
            let preview = truncate(idea.lines().next().unwrap_or(""), 72);
            println!("     {}.{}. {}", i + 1, j + 1, preview);
        }
        println!();
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..s.floor_char_boundary(max)])
    }
}
