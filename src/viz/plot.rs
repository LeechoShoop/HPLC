//! PNG chart rendering for [`Chromatogram`] data.
//!
//! Uses the [`plotters`] crate with the bitmap backend to write a 1200 × 600
//! PNG file.  The chart consists of:
//!
//! - A **blue line** tracing the full `time_series` signal.
//! - **Red filled circles** at each detected peak apex.
//! - **Retention-time labels** printed to the right of each marker.
//! - A **light grey mesh grid** with labelled x/y axes.

use std::path::Path;

use anyhow::{Context, Result};
use plotters::prelude::*;

use crate::model::Chromatogram;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const IMG_WIDTH:  u32 = 1200;
const IMG_HEIGHT: u32 = 600;

/// Fraction of the data range added as padding on each axis.
const PAD: f64 = 0.05;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Render `chrom` to a PNG file at `output_path`.
///
/// # Errors
/// Returns an error if the file cannot be created or if the plotters backend
/// fails for any reason.
pub fn render_chromatogram(chrom: &Chromatogram, output_path: &Path) -> Result<()> {
    let series = &chrom.time_series;
    if series.is_empty() {
        anyhow::bail!("cannot render an empty time_series");
    }

    // ── Data ranges ────────────────────────────────────────────────────────
    let x_min = series.iter().map(|p| p.0).fold(f64::INFINITY,  f64::min);
    let x_max = series.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let y_min = series.iter().map(|p| p.1).fold(f64::INFINITY,  f64::min);
    let y_max = series.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);

    let x_pad = (x_max - x_min) * PAD;
    let y_pad = (y_max - y_min) * PAD;

    // Always start y at 0 if all values are non-negative.
    let y_lo = if y_min >= 0.0 { 0.0_f64.min(y_min - y_pad) } else { y_min - y_pad };

    let x_range = (x_min - x_pad)..(x_max + x_pad);
    let y_range = y_lo..(y_max + y_pad);

    // ── Build x-axis caption ───────────────────────────────────────────────
    // If the detector is "MS" the x-axis is m/z; otherwise it is time.
    let x_label = if chrom.metadata.detector_type.to_ascii_uppercase().contains("MS") {
        "m/z"
    } else {
        "Retention Time (min)"
    };

    // ── Title ─────────────────────────────────────────────────────────────
    let title = if chrom.metadata.sample_id.is_empty() {
        format!("Chromatogram — {}", chrom.metadata.instrument)
    } else {
        format!(
            "Chromatogram — {} / {}",
            chrom.metadata.sample_id, chrom.metadata.instrument
        )
    };

    // ── Plotters setup ─────────────────────────────────────────────────────
    let root = BitMapBackend::new(output_path, (IMG_WIDTH, IMG_HEIGHT))
        .into_drawing_area();

    root.fill(&WHITE)
        .context("failed to fill background")?;

    let mut chart = ChartBuilder::on(&root)
        .caption(&title, ("sans-serif", 20).into_font())
        .margin(40)
        .x_label_area_size(45)
        .y_label_area_size(60)
        .build_cartesian_2d(x_range, y_range)
        .context("failed to build chart")?;

    // ── Grid and axes ──────────────────────────────────────────────────────
    chart
        .configure_mesh()
        .x_desc(x_label)
        .y_desc("Signal Intensity")
        .x_label_style(("sans-serif", 13).into_font())
        .y_label_style(("sans-serif", 13).into_font())
        .light_line_style(RGBColor(220, 220, 220))  // light grey grid
        .draw()
        .context("failed to draw mesh")?;

    // ── Signal line ────────────────────────────────────────────────────────
    chart
        .draw_series(LineSeries::new(
            series.iter().map(|&(x, y)| (x, y)),
            BLUE.stroke_width(2),
        ))
        .context("failed to draw signal line")?;

    // ── Peak markers and labels ────────────────────────────────────────────
    for peak in &chrom.peaks {
        let px = peak.retention_time;
        let py = peak.height;

        // Find the actual signal y at this retention time for the marker y.
        // (peak.height is already baseline-corrected; we draw it at that height.)

        // Red filled circle marker.
        chart
            .draw_series(std::iter::once(Circle::new(
                (px, py),
                6_i32,
                RED.filled(),
            )))
            .context("failed to draw peak marker")?;

        // Retention-time label, offset slightly to the right and upward.
        let label = if let Some(name) = &peak.name {
            format!("{name}\n{px:.3}")
        } else {
            format!("{px:.3}")
        };

        let x_offset = (x_max - x_min) * 0.01; // 1 % of x range
        let y_offset = (y_max - y_lo)   * 0.03; // 3 % of y range

        chart
            .draw_series(std::iter::once(Text::new(
                label,
                (px + x_offset, py + y_offset),
                ("sans-serif", 11).into_font().color(&RGBColor(180, 0, 0)),
            )))
            .context("failed to draw peak label")?;
    }

    root.present().context("failed to write PNG file")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Metadata, Peak};
    use std::f64::consts::PI;

    fn make_gaussian_chrom() -> Chromatogram {
        // 500-point Gaussian signal (mu=5, sigma=0.4, amp=1000) on [0, 10].
        let n = 500_usize;
        let time_series: Vec<(f64, f64)> = (0..n)
            .map(|i| {
                let x = i as f64 * 10.0 / (n - 1) as f64;
                let y = 1000.0 * (-(x - 5.0).powi(2) / (2.0 * 0.4_f64.powi(2))).exp();
                (x, y)
            })
            .collect();

        let peak = Peak {
            retention_time: 5.0,
            area: 1000.0 * 0.4 * (2.0 * PI).sqrt(),
            height: 1000.0,
            width: 2.0 * 2.0_f64.ln().sqrt() * 2.0 * 0.4, // FWHM
            name: Some("Test Peak".into()),
        };

        Chromatogram {
            metadata: Metadata {
                instrument: "Test Instrument".into(),
                method: "Test Method".into(),
                sample_id: "SMOKE-001".into(),
                date: "2026-09-25".into(),
                detector_type: "DAD".into(),
            },
            time_series,
            peaks: vec![peak],
        }
    }

    /// End-to-end smoke test: render a synthetic Chromatogram to a PNG and
    /// verify that the file is created with a plausible size (> 1 KB).
    #[test]
    fn test_render_produces_png_file() {
        let chrom = make_gaussian_chrom();
        let tmp = std::env::temp_dir().join("lab_parser_test_render.png");

        render_chromatogram(&chrom, &tmp).expect("render should succeed");

        let meta = std::fs::metadata(&tmp).expect("PNG file should exist");
        assert!(
            meta.len() > 1024,
            "PNG file too small ({} bytes); rendering may have failed silently",
            meta.len()
        );

        // Clean up.
        let _ = std::fs::remove_file(&tmp);
    }

    /// Rendering an empty time_series should return an error, not panic.
    #[test]
    fn test_render_empty_series_errors() {
        let chrom = Chromatogram {
            metadata: Metadata {
                instrument: String::new(),
                method: String::new(),
                sample_id: String::new(),
                date: String::new(),
                detector_type: String::new(),
            },
            time_series: vec![],
            peaks: vec![],
        };
        let tmp = std::env::temp_dir().join("lab_parser_test_empty.png");
        assert!(render_chromatogram(&chrom, &tmp).is_err());
    }
}
