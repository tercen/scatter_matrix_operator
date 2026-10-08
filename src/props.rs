//! Operator properties.
use anyhow::{Result, bail};
use tercen_rs::PropertyReader;
use tercen_rs::context::ContextBase;

#[derive(Debug, Clone)]
pub struct Settings {
    /// `panel_factor`: the column factor naming each cell's population. One panel per value;
    /// empty draws a single panel of all cells.
    pub panel_factor: String,
    /// `markers`: comma-separated row labels, in panel order. Empty uses every row, in the
    /// crosstab's row order.
    pub markers: Vec<String>,
    /// `max_cells`: background cells drawn from the whole projection.
    pub max_cells: usize,
    /// `max_cells_per_panel`: cells drawn from each population.
    pub max_cells_per_panel: usize,
    pub seed: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            panel_factor: String::new(),
            markers: Vec::new(),
            max_cells: 5000,
            max_cells_per_panel: 5000,
            seed: 42,
        }
    }
}

pub fn read(ctx: &ContextBase) -> Result<Settings> {
    let pr = PropertyReader::from_operator_settings(ctx.operator_settings());
    parse(|name| pr.get_string(name, ""))
}

/// Parse from a getter, so the rules are testable without a context.
pub fn parse(get: impl Fn(&str) -> String) -> Result<Settings> {
    let d = Settings::default();
    let count = |name: &str, dflt: usize| -> Result<usize> {
        let raw = get(name).trim().to_string();
        if raw.is_empty() {
            return Ok(dflt);
        }
        match raw.parse::<f64>() {
            Ok(v) if v >= 1.0 && v.fract() == 0.0 => Ok(v as usize),
            _ => bail!("property '{name}' should be a whole number of at least 1, got '{raw}'"),
        }
    };
    let seed = {
        let raw = get("seed").trim().to_string();
        if raw.is_empty() {
            d.seed
        } else {
            match raw.parse::<f64>() {
                Ok(v) if v >= 0.0 && v.fract() == 0.0 => v as u64,
                _ => bail!("property 'seed' should be a non-negative whole number, got '{raw}'"),
            }
        }
    };
    let markers: Vec<String> = get("markers")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(Settings {
        panel_factor: get("panel_factor").trim().to_string(),
        markers,
        max_cells: count("max_cells", d.max_cells)?,
        max_cells_per_panel: count("max_cells_per_panel", d.max_cells_per_panel)?,
        seed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_overrides() {
        let s = parse(|_| String::new()).unwrap();
        assert_eq!(
            (s.max_cells, s.max_cells_per_panel, s.seed),
            (5000, 5000, 42)
        );
        assert!(s.markers.is_empty() && s.panel_factor.is_empty());
        let s = parse(|n| match n {
            "markers" => " CD3, CD4 ,,CD8".into(),
            "max_cells" => "2000".into(),
            "panel_factor" => "names.run9_name".into(),
            _ => String::new(),
        })
        .unwrap();
        assert_eq!(s.markers, vec!["CD3", "CD4", "CD8"]);
        assert_eq!(s.max_cells, 2000);
        assert_eq!(s.panel_factor, "names.run9_name");
    }

    #[test]
    fn bad_numbers_are_refused() {
        assert!(
            parse(|n| if n == "max_cells" {
                "0".into()
            } else {
                String::new()
            })
            .is_err()
        );
        assert!(
            parse(|n| if n == "seed" {
                "-1".into()
            } else {
                String::new()
            })
            .is_err()
        );
        assert!(
            parse(|n| if n == "max_cells_per_panel" {
                "2.5".into()
            } else {
                String::new()
            })
            .is_err()
        );
    }
}
