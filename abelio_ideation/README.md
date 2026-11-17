# abelio_ideation

LLM-powered ideation pipeline that transforms a research objective into refined ideas through four stages: Formulation, Element Selection, Ideation, and Refinement.

## Public API

### `Objective`

The top-level input to the pipeline. Describes what to ideate on.

```rust
let objective = Objective {
    core_goal: "Development of Cas9 nickase fused to rolling-circle polymerase functional in mammalian cells.".into(),
    key_constraints: vec![
        "Must be compatible with mammalian cellular environment.".into(),
        "Should maintain high specificity for target DNA sequences.".into(),
    ].into(),
};
```

- `core_goal` -- the research goal statement.
- `key_constraints` -- bounds on the search space that all generated ideas must respect.

### `PipelineConfig`

Controls pipeline behavior: how many formulations to generate, element discovery parameters, refinement rounds, and concurrency limits. All fields have sensible defaults.

```rust
// Use defaults
let config = PipelineConfig::default();

// Or load from TOML
let config = PipelineConfig::from_toml("pipeline.toml")?;
```

Key fields:

| Field                      | Default | Description                                         |
| -------------------------- | ------- | --------------------------------------------------- |
| `num_formulations`         | 10      | Number of divergent goal re-framings to generate    |
| `entity_entries`           | 8       | Number of entity elements to select per formulation |
| `sample_ratio`             | 0.75    | Fraction of elements to sample for ideation         |
| `refinement.max_rounds`    | 7       | Total refinement rounds (including finalization)    |
| `refinement.warmup_rounds` | 2       | Rounds before the judge starts evaluating           |

Concurrency limits (`concurrency.*`) control parallelism for each stage but do not affect output determinism.

### `ideate(config, objective, store, progress) -> Vec<(String, Vec<String>)>`

Runs the full pipeline. For each formulation of the objective:

1. **Formulation** -- reframe the objective into divergent goal perspectives.
2. **Element selection** -- discover concept and entity building blocks.
3. **Ideation** -- synthesize research ideas from the discovered elements.
4. **Refinement** -- refine each idea through multi-round critique / defend / judge loops.

Returns a list of `(formulation, refined_ideas)` pairs. Formulations whose processing fails are silently dropped.

The `store` parameter accepts any `resm::Store + Clone` implementation (`FileStore` for disk persistence, `MemoryStore` for in-memory / tests).
All intermediate results are cached through the store for resumability.
The `progress` parameter takes `&Some(handle)` to receive progress events, or `&None` to run without them.

```rust
use abelio_ideation::resm::backend::FileStore;
use abelio_ideation::{Objective, PipelineConfig, ideate, initialize};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    initialize();

    let config = PipelineConfig::from_toml("pipeline.toml")?;
    let objective = Objective {
        core_goal: "Your research goal here".into(),
        key_constraints: vec!["constraint 1".into()].into(),
    };
    let store = FileStore::new("output")?;

    let results = ideate(&config, &objective, &store, &None).await?;
    for (formulation, ideas) in &results {
        println!("{formulation}: {} ideas", ideas.len());
    }
    Ok(())
}
```

### `initialize()`

Loads `.env` via `dotenvy` and sets up `tracing-subscriber` with `RUST_LOG` env filter. Call once at startup.

## Command line

```
cargo run -p abelio_ideation --features cli -- "<core goal>" -k "<constraint>" -o output.md
cargo run -p abelio_ideation --features cli -- --file objective.json
```

`--file` reads `{ "core_goal": "...", "key_constraints": ["..."] }` and cannot be combined with the positional goal or `-k`.
`--config` (default `pipeline.toml`), `--cache-dir` (default `.cache`), `--output` (default `output.md`), and `--session` (reuse an id to resume from cache) are optional.

## Environment variables

- `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, `XAI_API_KEY`, `OPENROUTER_API_KEY`: provider keys read by the model configs.
- `TEI_URL`: text-embeddings-inference endpoint.
- `ZERANK2_URL` and `ZERANK2_API_KEY`: zerank-2 reranker.
- `S2_API_KEY` and `S2_API_URL`: optional, read by the literature search stand-in.
