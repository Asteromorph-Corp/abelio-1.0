# abelio_report

Report writer and summarizer: receive ideas from `abelio_ideation` and turn each into a report and a summary.

## Public API

- `write_report(input, store, progress)` turns an idea into a markdown report with citations.
- `summarize(report, store, progress)` turns a report into a summary.
- `initialize()` (behind the `cli` feature) loads `.env`, sets up tracing, and returns the cache store under `.cache`.

## Command line

```
cargo run -p abelio_report --features cli --bin abelio_report -- idea1.md idea2.md
cargo run -p abelio_report --features cli --bin abelio_report_pure -- idea.md report.md
```

`abelio_report` takes one or more input files and writes `report.md` and `summary.md` for each under `output/<timestamp>/<index>_<name>/`.
`abelio_report_pure` writes a single report to the given output path, without the summary.

## Environment variables

- `ANTHROPIC_API_KEY`: read by the model configs.
- `S2_API_KEY` and `S2_API_URL`: optional, read by the literature search stand-in.
