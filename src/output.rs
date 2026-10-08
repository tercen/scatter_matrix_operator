//! The result: one table keyed by `.ci`, several rows per cell — one per (panel, marker pair)
//! the cell is drawn in. A `.ci`-keyed result joins to the column table, so a downstream Plot
//! sees exactly these rows with every column factor of their cell.
//!
//! Columns (namespaced): `panel`, `x_marker`, `y_marker`, `x`, `y`, `set` (`all cells` or
//! `this population`), and `.ci`.
use std::io::Write;

use anyhow::{Result, anyhow};

use crate::matrix::{Rows, SET_ALL, SET_PANEL};
use crate::tson::TsonWriter;

pub struct ColSpec<'a> {
    pub name: &'a str,
    pub ty: &'a str, // "double" | "int32" | "string"
}

pub fn write_header<W: Write>(
    w: &mut TsonWriter<W>,
    table_name: &str,
    n_rows: usize,
    cols: &[ColSpec],
    n_tables: usize,
) -> Result<()> {
    w.map(3)?;
    w.key("kind")?;
    w.str("OperatorResult")?;
    w.key("tables")?;
    w.list(n_tables)?;

    w.map(4)?;
    w.key("kind")?;
    w.str("Table")?;
    w.key("nRows")?;
    w.i32(i32::try_from(n_rows).map_err(|_| {
        anyhow!(
            "the result would have {n_rows} rows, more than a Tercen table can hold (i32::MAX). \
             Project fewer cells, or split the step."
        )
    })?)?;
    w.key("properties")?;
    w.map(4)?;
    w.key("kind")?;
    w.str("TableProperties")?;
    w.key("name")?;
    w.str(table_name)?;
    w.key("sortOrder")?;
    w.list(0)?;
    w.key("ascending")?;
    w.bool(false)?;
    w.key("columns")?;
    w.list(cols.len())?;
    Ok(())
}

pub fn write_column_header<W: Write>(
    w: &mut TsonWriter<W>,
    c: &ColSpec,
    n_rows: usize,
) -> Result<()> {
    w.map(6)?;
    w.key("kind")?;
    w.str("Column")?;
    w.key("name")?;
    w.str(c.name)?;
    w.key("type")?;
    w.str(c.ty)?;
    w.key("nRows")?;
    w.i32(n_rows as i32)?;
    w.key("size")?;
    w.i32(n_rows as i32)?;
    w.key("values")?;
    Ok(())
}

/// Write the rows. `panels` and `markers` are the name tables `rows` indexes into.
pub fn write_rows<W: Write>(
    w: &mut TsonWriter<W>,
    table_name: &str,
    namespace: &str,
    panels: &[String],
    markers: &[String],
    rows: &Rows,
) -> Result<()> {
    let n = rows.len();
    let names: Vec<String> = ["panel", "x_marker", "y_marker", "x", "y", "set"]
        .iter()
        .map(|c| format!("{namespace}.{c}"))
        .collect();
    let cols = [
        ColSpec {
            name: &names[0],
            ty: "string",
        },
        ColSpec {
            name: &names[1],
            ty: "string",
        },
        ColSpec {
            name: &names[2],
            ty: "string",
        },
        ColSpec {
            name: &names[3],
            ty: "double",
        },
        ColSpec {
            name: &names[4],
            ty: "double",
        },
        ColSpec {
            name: &names[5],
            ty: "string",
        },
        ColSpec {
            name: ".ci",
            ty: "int32",
        },
    ];
    write_header(w, table_name, n, &cols, 1)?;

    // Strings are written from the small name tables, without materialising a String per row.
    write_strs(w, &cols[0], n, |k| panels[rows.panel[k] as usize].as_str())?;
    write_strs(w, &cols[1], n, |k| {
        markers[rows.x_marker[k] as usize].as_str()
    })?;
    write_strs(w, &cols[2], n, |k| {
        markers[rows.y_marker[k] as usize].as_str()
    })?;
    write_column_header(w, &cols[3], n)?;
    w.f64_list(&rows.x)?;
    write_column_header(w, &cols[4], n)?;
    w.f64_list(&rows.y)?;
    write_strs(w, &cols[5], n, |k| {
        if rows.set[k] == 0 { SET_ALL } else { SET_PANEL }
    })?;
    write_column_header(w, &cols[6], n)?;
    w.i32_list(&rows.ci)?;
    Ok(())
}

fn write_strs<'a, W: Write>(
    w: &mut TsonWriter<W>,
    spec: &ColSpec,
    n: usize,
    it: impl Fn(usize) -> &'a str,
) -> Result<()> {
    write_column_header(w, spec, n)?;
    let bytes: usize = (0..n).map(|k| it(k).len() + 1).sum();
    w.str_list_iter(bytes, (0..n).map(&it))?;
    Ok(())
}

/// Close the result. One relation, so no joins to declare.
pub fn write_footer<W: Write>(w: &mut TsonWriter<W>) -> Result<()> {
    w.key("joinOperators")?;
    w.list(0)?;
    w.flush()?;
    Ok(())
}
