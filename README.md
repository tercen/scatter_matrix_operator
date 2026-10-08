# scatter_matrix_operator

Bivariate scatter-matrix rows for a Tercen Plot step: for each population and each pair of markers,
a seeded draw of **all cells** and of **this population**, so one Plot per population shows that
population over all cells on every marker pair. This is the bivariate check a cytometrist pages
through after clustering ("does population 5 sit where CD8+ CD56+ cells should?").

## Projection

| | |
|---|---|
| rows | marker name (one row factor) |
| columns | cells; the population label must be one of the column factors |
| y | the marker value, usually arcsinh-transformed and normalised |

## Properties

| name | default | meaning |
|---|---|---|
| `panel_factor` | | column factor naming each cell's population; one panel per value. Empty: a single panel of all cells |
| `markers` | | comma-separated row labels in panel order; empty uses every row. Pairs are the lower triangle: `x` is the earlier marker |
| `max_cells` | 5000 | background cells drawn from the whole projection |
| `max_cells_per_panel` | 5000 | cells drawn from each population, so a small population is drawn as densely as a large one |
| `seed` | 42 | seed for both draws |

## Output

One table keyed by `.ci`, several rows per drawn cell: `panel`, `x_marker`, `y_marker`, `x`, `y`,
`set` (`all cells` or `this population`). Rows count is
`panels × pairs × (max_cells + cells drawn per panel)`; 15 populations, 8 markers and the defaults
give 15 × 28 × 10,000 = 4.2 M rows.

Plotting: one Plot step per population, filter `panel = <population>`, columns `x_marker`, rows
`y_marker`, x `x`, y `y`, colour `set` with `all cells` grey and `this population` red. Background
rows come first, so the population is drawn on top.

## Runtime model

- Rust (static musl binary on `scratch`), gRPC operator, one task, single-threaded.
- Reads the crosstab once (streamed), builds the rows in memory, writes one TSON result.
- CPU-bound only in the draw; I/O-bound overall.

## Memory model

- Baseline about 50 MB.
- Linear: 4 bytes per input value (the gathered `f32` matrix) plus about 30 bytes per output row.
- `memory_model.json`: 400 MB + 4 B × values. A 20k-events-per-file cohort of 1.4 M cells × 8 markers
  gathers 45 MB and writes 4.2 M rows (about 130 MB).
- Gathers the whole crosstab (a pair needs both values of a cell), then streams the output.

## Tests

`cargo test` checks the pair layout, the panels and the draws. `tests/test.json` is the platform
test on 60 synthetic cells; its expected table comes from `cargo run --example fixture`.

Licence: Apache-2.0.
