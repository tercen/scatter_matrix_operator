//! scatter_matrix_operator — bivariate scatter-matrix rows for a Plot step.
//!
//! Rows are markers, columns are cells, y is the value. The result is one row per (panel,
//! marker pair, drawn cell): the two values and whether the cell is background or the panel's
//! population. One Plot per population then shows that population over all cells on every
//! marker pair — the way bivariate plots are paged through when checking a clustering. See
//! `matrix.rs` for the layout.
pub mod context;
pub mod input;
pub mod matrix;
pub mod output;
pub mod pagecache;
pub mod progress;
pub mod props;
pub mod tson;
pub mod upload;

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use tercen_rs::context::ContextBase;
use tercen_rs::{DevContext, TercenClient};

use progress::Reporter;
use tson::TsonWriter;

const CHUNK: usize = 1_000_000;

pub fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

pub fn require_env(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| anyhow!("{name} is not set"))
}

pub async fn run(task_id: &str) -> Result<()> {
    tracing::info!("scatter_matrix_operator starting (task_id={task_id})");
    let client = build_client().await?;
    let ctx = context::from_task_id(client, task_id).await?;
    execute(
        &ctx,
        Mode::Production {
            task_id: task_id.to_string(),
        },
    )
    .await
}

pub async fn run_dev(workflow_id: &str, step_id: &str) -> Result<()> {
    tracing::info!("scatter_matrix_operator starting in dev mode ({workflow_id} / {step_id})");
    let client = build_client().await?;
    let ctx = DevContext::from_workflow_step(client, workflow_id, step_id)
        .await
        .map_err(|e| anyhow!("load workflow {workflow_id} / step {step_id}: {e}"))?;
    execute(
        &ctx,
        Mode::Dev {
            workflow_id: workflow_id.to_string(),
            step_id: step_id.to_string(),
        },
    )
    .await
}

enum Mode {
    Production {
        task_id: String,
    },
    Dev {
        workflow_id: String,
        step_id: String,
    },
}

async fn build_client() -> Result<Arc<TercenClient>> {
    let client = TercenClient::from_env()
        .await
        .map_err(|e| anyhow!("connect to Tercen: {e}"))?;
    tracing::info!("connected to Tercen");
    Ok(Arc::new(client))
}

async fn execute(ctx: &ContextBase, mode: Mode) -> Result<()> {
    let t_start = Instant::now();
    tracing::info!(
        workflow = ctx.workflow_id(),
        step = ctx.step_id(),
        namespace = ctx.namespace(),
        "context loaded"
    );
    let rep = match &mode {
        Mode::Production { task_id } => Reporter::spawn(Arc::clone(ctx.client()), task_id.clone()),
        Mode::Dev { .. } => Reporter::silent(),
    };
    let s = props::read(ctx)?;

    let n_values = input::cell_count(ctx).await?;
    let channels = input::row_labels(ctx).await?;
    let p = channels.len();
    if p < 2 {
        anyhow::bail!(
            "the projection has {p} rows; a scatter matrix needs at least two markers on rows"
        );
    }
    let n_cells = n_values / p;
    tracing::info!(n_values, n_cells, markers = p, "projection");

    // Marker order: as listed in `markers`, else the crosstab's row order.
    let order: Vec<usize> = if s.markers.is_empty() {
        (0..p).collect()
    } else {
        s.markers
            .iter()
            .map(|m| {
                channels.iter().position(|c| c == m).ok_or_else(|| {
                    anyhow!("marker '{m}' in property 'markers' is not a row of the projection (rows are {channels:?})")
                })
            })
            .collect::<Result<_>>()?
    };
    if order.len() < 2 {
        anyhow::bail!(
            "property 'markers' names {} marker; a scatter matrix needs at least two",
            order.len()
        );
    }

    let labels: Option<Vec<String>> = if s.panel_factor.is_empty() {
        None
    } else {
        let l = input::column_labels(ctx, &s.panel_factor).await?;
        if l.len() != n_cells {
            anyhow::bail!(
                "column factor '{}' has {} values for {n_cells} cells",
                s.panel_factor,
                l.len()
            );
        }
        Some(l)
    };

    rep.at(0, "Reading the crosstab");
    let data = gather(ctx, n_values, n_cells, p, &rep).await?;

    rep.at(progress::FIT.0, "Drawing cells and pairs");
    let (panels, rows) = matrix::build(
        &data,
        n_cells,
        p,
        &order,
        labels.as_deref(),
        s.max_cells,
        s.max_cells_per_panel,
        s.seed,
    );
    drop(data);
    let markers: Vec<String> = order.iter().map(|&i| channels[i].clone()).collect();
    let n_pairs = markers.len() * (markers.len() - 1) / 2;
    tracing::info!(
        panels = panels.len(),
        pairs = n_pairs,
        rows = rows.len(),
        "scatter matrix"
    );
    rep.info(format!(
        "Scatter matrix: {} panels x {n_pairs} marker pairs, {} rows from {n_cells} cells",
        panels.len(),
        rows.len()
    ));

    rep.at(progress::WRITE.0, "Writing the result");
    let work_root = std::env::temp_dir().join(format!(
        "scatter_matrix_op_{}_{}",
        ctx.workflow_id(),
        ctx.step_id()
    ));
    std::fs::create_dir_all(&work_root)
        .with_context(|| format!("create {}", work_root.display()))?;
    let _guard = TempDirGuard(work_root.clone());
    let result_path = work_root.join("result.tson");
    {
        let f = std::fs::File::create(&result_path)
            .with_context(|| format!("create {}", result_path.display()))?;
        let w = std::io::BufWriter::with_capacity(4 << 20, pagecache::Releasing::new(f, 256 << 20));
        let mut w = TsonWriter::new(w)?;
        output::write_rows(
            &mut w,
            &table_name(ctx),
            ctx.namespace(),
            &panels,
            &markers,
            &rows,
        )?;
        output::write_footer(&mut w)?;
    }
    let bytes = std::fs::metadata(&result_path)?.len();
    tracing::info!(bytes, "result written");
    pagecache::release_path(&result_path);

    rep.at(progress::UPLOAD.0, "Uploading the result");
    match mode {
        Mode::Production { task_id } => {
            upload::save_production(ctx, &task_id, &result_path, &rep).await?
        }
        Mode::Dev {
            workflow_id,
            step_id,
        } => {
            let saved = upload::save_dev(ctx, &workflow_id, &step_id, &result_path).await?;
            tracing::info!(task_id = saved.task_id, "dev result saved");
        }
    }
    rep.at(100, "Done");
    tracing::info!(
        total_secs = format!("{:.1}", t_start.elapsed().as_secs_f64()),
        peak_rss_kb = peak_rss_kb().unwrap_or(0),
        "done"
    );
    Ok(())
}

/// Gather the crosstab into a cell-by-marker matrix (`f32`: a picture needs no more).
///
/// Scattered by `(.ri, .ci)`, so no ordering is assumed.
async fn gather(
    ctx: &ContextBase,
    n_values: usize,
    n_cells: usize,
    p: usize,
    rep: &Reporter,
) -> Result<Vec<f32>> {
    let mut data = vec![f32::NAN; n_cells * p];
    let mut seen = 0usize;
    let mut out_of_range = 0usize;
    input::for_each_chunk(ctx, &[".ri", ".ci", ".y"], n_values, CHUNK, |c| {
        for k in 0..c.y.len() {
            let (ri, ci) = (c.ri[k] as usize, c.ci[k] as usize);
            if ri >= p || ci >= n_cells {
                out_of_range += 1;
                continue;
            }
            data[ci + ri * n_cells] = c.y[k] as f32;
        }
        seen += c.len();
        rep.at(
            progress::band(progress::READ, seen, n_values.max(1)),
            format!("Read {seen} of {n_values}"),
        );
        Ok(())
    })
    .await?;
    if out_of_range > 0 {
        anyhow::bail!(
            "{out_of_range} values fell outside the {n_cells} x {p} crosstab the schema \
             described; the projection changed under the run"
        );
    }
    let missing = data.iter().filter(|v| v.is_nan()).count();
    if missing > 0 {
        anyhow::bail!(
            "{missing} of {} cell-channel pairs have no value. A scatter matrix needs every \
             marker of every cell. Filter the projection, or fill the gaps \
             upstream.",
            data.len()
        );
    }
    Ok(data)
}

struct TempDirGuard(std::path::PathBuf);
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn table_name(ctx: &ContextBase) -> String {
    format!("{}_{}", ctx.step_id(), ctx.qt_hash())
}

fn peak_rss_kb() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
}
