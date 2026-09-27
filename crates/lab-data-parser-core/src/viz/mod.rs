//! Visualisation helpers – renders charts with the `plotters` crate.

pub mod plot;

use anyhow::Result;

pub use plot::render_chromatogram;

/// Placeholder: render a bar chart for `values` and save to `output_path`.
pub fn render_bar_chart(
    _title: &str,
    _labels: &[&str],
    _values: &[f64],
    _output_path: &str,
) -> Result<()> {
    // TODO: implement with plotters
    Ok(())
}
