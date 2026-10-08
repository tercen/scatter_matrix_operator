//! The scatter matrix itself: which cells, which marker pairs, which panel.
//!
//! A bivariate check of a population draws every lineage-marker pair, all cells in grey and the
//! population on top. A Tercen Plot step draws what one projection holds, so the operator writes
//! that picture as rows: for each panel (one per population), each pair `(x_marker, y_marker)`
//! with `x_marker` before `y_marker` in the marker order (the lower triangle), and each drawn
//! cell, one row with the two values and whether the cell is background (`all cells`) or the
//! panel's population (`this population`).
//!
//! Two seeded draws keep the picture readable at any cohort size: `max_cells` background cells
//! from the whole projection, and up to `max_cells_per_panel` from each population, so a small
//! population is drawn as densely as a large one. Background rows come before population rows,
//! so a plot that draws in row order puts the population on top.
use rand::SeedableRng;
use rand::seq::SliceRandom;

pub const SET_ALL: &str = "all cells";
pub const SET_PANEL: &str = "this population";

/// The rows to write, as indices into small tables plus the two values.
#[derive(Debug, Default)]
pub struct Rows {
    /// Index into the panel names.
    pub panel: Vec<u32>,
    /// Index into the marker names.
    pub x_marker: Vec<u16>,
    pub y_marker: Vec<u16>,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// 0 = all cells (background), 1 = this population.
    pub set: Vec<u8>,
    /// The cell (crosstab column) each row belongs to.
    pub ci: Vec<i32>,
}

impl Rows {
    pub fn len(&self) -> usize {
        self.x.len()
    }
    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }
}

/// A sorted seeded draw of at most `k` of `idx`. ChaCha8 on the seed and a per-draw salt, so the
/// same seed gives the same cells run after run and two draws do not share a stream.
pub fn draw(idx: &[usize], k: usize, seed: u64, salt: u64) -> Vec<usize> {
    let mut v = idx.to_vec();
    if v.len() > k {
        let mut rng =
            rand_chacha::ChaCha8Rng::seed_from_u64(seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        v.shuffle(&mut rng);
        v.truncate(k);
    }
    v.sort_unstable();
    v
}

/// Build the rows.
///
/// `data` is column-major `n` cells × `p` markers. `order` lists the marker indices to use, in
/// panel order. `labels` holds one population label per cell, or is `None` for a single panel
/// of all cells. Returns the panel names (sorted) and the rows.
#[allow(clippy::too_many_arguments)]
pub fn build(
    data: &[f32],
    n: usize,
    p: usize,
    order: &[usize],
    labels: Option<&[String]>,
    max_cells: usize,
    max_cells_per_panel: usize,
    seed: u64,
) -> (Vec<String>, Rows) {
    assert_eq!(data.len(), n * p);
    let all: Vec<usize> = (0..n).collect();
    let background = draw(&all, max_cells, seed, 0);

    let panels: Vec<(String, Vec<usize>)> = match labels {
        None => vec![(SET_ALL.to_string(), Vec::new())],
        Some(lab) => {
            assert_eq!(lab.len(), n);
            let mut names: Vec<&String> = lab.iter().collect();
            names.sort();
            names.dedup();
            names
                .into_iter()
                .enumerate()
                .map(|(k, name)| {
                    let members: Vec<usize> = (0..n).filter(|&i| &lab[i] == name).collect();
                    (
                        name.clone(),
                        draw(&members, max_cells_per_panel, seed, 1 + k as u64),
                    )
                })
                .collect()
        }
    };

    let pairs: Vec<(usize, usize)> = (0..order.len())
        .flat_map(|a| (a + 1..order.len()).map(move |b| (a, b)))
        .collect();
    let per_panel: usize = panels
        .iter()
        .map(|(_, m)| pairs.len() * (background.len() + m.len()))
        .sum();
    let mut r = Rows {
        panel: Vec::with_capacity(per_panel),
        x_marker: Vec::with_capacity(per_panel),
        y_marker: Vec::with_capacity(per_panel),
        x: Vec::with_capacity(per_panel),
        y: Vec::with_capacity(per_panel),
        set: Vec::with_capacity(per_panel),
        ci: Vec::with_capacity(per_panel),
    };
    let value = |cell: usize, marker: usize| data[cell + marker * n] as f64;
    for (pi, (_, members)) in panels.iter().enumerate() {
        for &(a, b) in &pairs {
            let (mx, my) = (order[a], order[b]);
            for (set, cells) in [(0u8, &background), (1u8, members)] {
                for &c in cells.iter() {
                    r.panel.push(pi as u32);
                    r.x_marker.push(a as u16);
                    r.y_marker.push(b as u16);
                    r.x.push(value(c, mx));
                    r.y.push(value(c, my));
                    r.set.push(set);
                    r.ci.push(c as i32);
                }
            }
        }
    }
    (panels.into_iter().map(|(n, _)| n).collect(), r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 6 cells × 3 markers; marker m of cell c is 10*m + c.
    fn data() -> Vec<f32> {
        let (n, p) = (6, 3);
        let mut d = vec![0f32; n * p];
        for m in 0..p {
            for c in 0..n {
                d[c + m * n] = (10 * m + c) as f32;
            }
        }
        d
    }

    #[test]
    fn lower_triangle_in_the_given_order() {
        let (panels, r) = build(&data(), 6, 3, &[2, 0, 1], None, 100, 100, 1);
        assert_eq!(panels, vec![SET_ALL.to_string()]);
        // 3 pairs × 6 background cells, no population rows without labels.
        assert_eq!(r.len(), 18);
        // First pair is (order[0], order[1]) = markers 2 and 0.
        assert_eq!((r.x_marker[0], r.y_marker[0]), (0, 1));
        assert_eq!((r.x[0], r.y[0]), (20.0, 0.0));
        assert!(r.set.iter().all(|s| *s == 0));
    }

    #[test]
    fn each_panel_has_background_then_its_population() {
        let lab: Vec<String> = ["b", "a", "b", "a", "a", "b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (panels, r) = build(&data(), 6, 3, &[0, 1, 2], Some(&lab), 4, 2, 7);
        assert_eq!(panels, vec!["a".to_string(), "b".to_string()]);
        // Per panel: 3 pairs × (4 background + 2 members).
        assert_eq!(r.len(), 2 * 3 * 6);
        for k in 0..r.len() {
            if r.set[k] == 1 {
                let name = &panels[r.panel[k] as usize];
                assert_eq!(
                    &lab[r.ci[k] as usize], name,
                    "row {k} is in the wrong panel"
                );
            }
            // The values are the cell's own.
            let c = r.ci[k] as f64;
            assert_eq!(r.x[k], 10.0 * r.x_marker[k] as f64 + c);
            assert_eq!(r.y[k], 10.0 * r.y_marker[k] as f64 + c);
        }
        // Background first within each (panel, pair) block.
        let block = &r.set[0..6];
        assert_eq!(block, &[0, 0, 0, 0, 1, 1]);
    }

    #[test]
    fn draws_repeat_and_differ_by_seed() {
        let idx: Vec<usize> = (0..1000).collect();
        assert_eq!(draw(&idx, 10, 3, 0), draw(&idx, 10, 3, 0));
        assert_ne!(draw(&idx, 10, 3, 0), draw(&idx, 10, 4, 0));
        assert_eq!(draw(&idx[..5], 10, 3, 0), vec![0, 1, 2, 3, 4]);
    }
}
