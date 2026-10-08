//! Expected output for tests/test.json, from the same `matrix::build` the operator runs:
//! `cargo run --example fixture -- tests/scatter_input.csv > tests/table1.csv`.
//! Input columns: cell_id, pop, marker, value. Cells are ordered by cell_id and markers by name,
//! as Tercen orders a crosstab's columns and rows.
use scatter_matrix_operator::matrix::{SET_ALL, SET_PANEL, build};
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("input csv");
    let text = std::fs::read_to_string(path).unwrap();
    let mut cells: BTreeMap<i64, String> = BTreeMap::new();
    let mut vals: BTreeMap<(String, i64), f64> = BTreeMap::new();
    for l in text.lines().skip(1) {
        let f: Vec<&str> = l.split(',').collect();
        let c: i64 = f[0].parse().unwrap();
        cells.insert(c, f[1].to_string());
        vals.insert((f[2].to_string(), c), f[3].parse().unwrap());
    }
    let mut markers: Vec<String> = vals.keys().map(|(m, _)| m.clone()).collect();
    markers.sort();
    markers.dedup();
    let (n, p) = (cells.len(), markers.len());
    let mut data = vec![0f32; n * p];
    for (mi, m) in markers.iter().enumerate() {
        for (ci, c) in cells.keys().enumerate() {
            data[ci + mi * n] = vals[&(m.clone(), *c)] as f32;
        }
    }
    let labels: Vec<String> = cells.values().cloned().collect();
    let want = ["CD3", "CD4", "CD8", "CD19"];
    let order: Vec<usize> = want
        .iter()
        .map(|w| markers.iter().position(|m| m == w).unwrap())
        .collect();
    let (panels, r) = build(&data, n, p, &order, Some(&labels), 30, 10, 42);
    println!("ds0.panel,ds0.x_marker,ds0.y_marker,ds0.x,ds0.y,ds0.set,.ci");
    for k in 0..r.len() {
        println!(
            "{},{},{},{:?},{:?},{},{}",
            panels[r.panel[k] as usize],
            want[r.x_marker[k] as usize],
            want[r.y_marker[k] as usize],
            r.x[k],
            r.y[k],
            if r.set[k] == 0 { SET_ALL } else { SET_PANEL },
            r.ci[k]
        );
    }
}
