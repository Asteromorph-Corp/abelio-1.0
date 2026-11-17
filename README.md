# Abelio

Abelio turns a research objective into hypotheses and writes them up as research reports.
Its Ideation Core runs four stages (Formulation, Element Selection, Ideation, and Refinement) and is followed by a report writer and a summarizer.

This repository holds the code of the system, the data and analysis behind the paper's ablation, and the reports from the epithelial barrier repair runs.

## Contents

| Path | What it holds |
|---|---|
| [`abelio_ideation/`](abelio_ideation/README.md) | The Ideation Core: Formulation, Element Selection, Ideation, and Refinement |
| [`abelio_report/`](abelio_report/README.md) | The report writer and summarizer that turn ideas into a report |
| [`gateway/`](gateway/README.md) | The system prompt of the Gateway LLM |
| `vendor/` | Path crates the two crates above depend on, including the literature search stand-in |
| [`data/`](data/README.md) | Inference data, evaluation prompts, and analysis scripts for the ablation |
| `artifacts/` | PDF reports from the epithelial barrier repair runs |

## Requirements

- Rust 1.85 or later (the crates use edition 2024).
- A [text-embeddings-inference](https://github.com/huggingface/text-embeddings-inference) server for embeddings, and access to the zerank-2 reranker.
- API keys for the LLM providers the agents use.
- [uv](https://docs.astral.sh/uv/), only for the analysis scripts in `data/`.

## Build

```
cargo build --workspace
```

Everything resolves from crates.io and the in-tree `vendor/` crates.
The command-line binaries `abelio_ideation`, `abelio_report`, and `abelio_report_pure` need the `cli` feature.

## Run

Generate ideas from an objective, given inline or as a JSON file:

```
cargo run -p abelio_ideation --features cli -- "<core goal>" -k "<constraint>" -o output.md
cargo run -p abelio_ideation --features cli -- --file objective.json
```

`abelio_ideation` reads `pipeline.toml` from the working directory unless `--config` names another file, and caches results under `.cache`.

Write reports from ideas:

```
cargo run -p abelio_report --features cli --bin abelio_report -- idea1.md idea2.md
cargo run -p abelio_report --features cli --bin abelio_report_pure -- idea.md report.md
```

`abelio_report` writes `report.md` and `summary.md` for each input under `output/<timestamp>/`.
`abelio_report_pure` writes a single report, without the summary.

## Environment variables

Both crates load a `.env` file from the working directory on startup.

| Variable | Required | Used for |
|---|---|---|
| `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, `XAI_API_KEY`, `OPENROUTER_API_KEY` | For each provider in use | The model configs under each crate's `configs/models/` |
| `TEI_URL` | Yes | The text-embeddings-inference endpoint |
| `ZERANK2_URL`, `ZERANK2_API_KEY` | Yes | The zerank-2 reranker that scores candidate elements |
| `S2_API_KEY` | No | Sent to Semantic Scholar as the `x-api-key` header |
| `S2_API_URL` | No | Overrides the Semantic Scholar base URL |

## Reproducing the paper

The ablation figure is rebuilt from the published data without running any model:

```
cd data
uv run python analysis/figure.py --data data --out results
```

[`data/README.md`](data/README.md) describes each file and maps the data's labels to the paper's stages.

To run the system as it was for the paper, check out the `paper` tag.
The next section lists how that tag differs from the paper runs.

## Differences from the paper runs

### At the `paper` tag

The `paper` tag sends the model the same prompts as the paper runs.
Its code was cleaned up for release, and it behaves differently from the code behind the runs only where that code could not be released or no longer runs:

- **Literature search.** The backend is a Semantic Scholar stand-in, and the production implementation is omitted.
  The tool names, descriptions, and input schemas shown to the model are unchanged.
- **Gemini.** `gemini-3-flash-preview` is now `gemini-3.5-flash`, which Google may serve with a newer Flash.
- **Prompt caching.** Claude requests now enable prompt caching (`cache_control`).
  The model input is unchanged, and only billing differs.
- **Report output.** The report CLI writes markdown.
  The PDFs in `artifacts/` were rendered from that markdown.

## Artifacts

`artifacts/` holds the PDF reports from three runs on epithelial barrier repair:

- `1_original` is the first run.
- `2_peg` is the PEG branch.
- `3_dna` is the DNA nanobridge branch.

The `1_selected_*` file in each directory is that run's final report, and the other files are the reports it was selected from.
`1_original` holds the final report and 8 others.
`2_peg` and `3_dna` hold the final report and 7 others each, because one report from each of those runs is missing.

## License

The code is under the [PolyForm Noncommercial License 1.0.0](LICENSE).
The contents of `data/` and `artifacts/` are under [CC BY-NC-ND 4.0](data/LICENSE).
The one exception is `abelio_ideation/protos/tei/v1/tei.proto`, which comes from [text-embeddings-inference](https://github.com/huggingface/text-embeddings-inference) and stays under Apache-2.0 ([`abelio_ideation/protos/tei/LICENSE`](abelio_ideation/protos/tei/LICENSE)).
