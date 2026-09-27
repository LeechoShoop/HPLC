//! Export a [`Chromatogram`] to several formats suitable for downstream analysis.
//!
//! | Function          | Output                                               |
//! |-------------------|------------------------------------------------------|
//! | [`export_json`]   | Single pretty-printed JSON file                      |
//! | [`export_csv`]    | Two CSV files: `<stem>_signal.csv` + `<stem>_peaks.csv` |
//! | [`export_xlsx`]   | Single XLSX workbook with two named sheets           |
//!
//! All column names are long-form and self-explanatory so that colleagues
//! can open the output directly in Excel, Python (`pandas`), or R without
//! needing to consult documentation.
//!
//! ## CSV column layout
//!
//! **Signal sheet / `_signal.csv`**
//!
//! | `retention_time_min` | `intensity` |
//! |----------------------|-------------|
//!
//! **Peaks sheet / `_peaks.csv`**
//!
//! | `peak_name` | `retention_time_min` | `peak_height` | `peak_area` | `peak_width_fwhm_min` |
//! |-------------|----------------------|---------------|-------------|-----------------------|

use std::io::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use rust_xlsxwriter::{Format, Workbook, XlsxError};

use crate::model::Chromatogram;

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

/// Write `chrom` as a pretty-printed JSON file to `output_path`.
pub fn export_json(chrom: &Chromatogram, output_path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(chrom)
        .context("failed to serialise Chromatogram to JSON")?;
    std::fs::write(output_path, json)
        .with_context(|| format!("cannot write {}", output_path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

/// Write two CSV files derived from `chrom`.
///
/// Given an `output_path` like `run.csv` this produces:
/// * `run_signal.csv`  — the full time/intensity trace
/// * `run_peaks.csv`   — the detected peaks table
///
/// If `output_path` has no stem (unlikely), both files land next to it with
/// the suffixes appended directly.
pub fn export_csv(chrom: &Chromatogram, output_path: &Path) -> Result<()> {
    let stem = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("chromatogram");
    let dir = output_path.parent().unwrap_or(Path::new("."));

    let signal_path = dir.join(format!("{stem}_signal.csv"));
    let peaks_path  = dir.join(format!("{stem}_peaks.csv"));

    write_signal_csv(chrom, &signal_path)
        .with_context(|| format!("signal CSV: {}", signal_path.display()))?;
    write_peaks_csv(chrom, &peaks_path)
        .with_context(|| format!("peaks CSV: {}", peaks_path.display()))?;

    Ok(())
}

fn write_signal_csv(chrom: &Chromatogram, path: &Path) -> Result<()> {
    let mut f = std::fs::File::create(path)
        .with_context(|| format!("cannot create {}", path.display()))?;

    writeln!(f, "retention_time_min,intensity")?;
    for &(t, v) in &chrom.time_series {
        writeln!(f, "{t},{v}")?;
    }
    Ok(())
}

fn write_peaks_csv(chrom: &Chromatogram, path: &Path) -> Result<()> {
    let mut f = std::fs::File::create(path)
        .with_context(|| format!("cannot create {}", path.display()))?;

    writeln!(
        f,
        "peak_name,retention_time_min,peak_height,peak_area,peak_width_fwhm_min"
    )?;
    for p in &chrom.peaks {
        let name = p.name.as_deref().unwrap_or("");
        writeln!(
            f,
            "{name},{},{},{},{}",
            p.retention_time, p.height, p.area, p.width
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// XLSX
// ---------------------------------------------------------------------------

/// Write `chrom` to a two-sheet XLSX workbook at `output_path`.
///
/// * **Sheet "Signal"**  — columns: `retention_time_min`, `intensity`
/// * **Sheet "Peaks"**   — columns: `peak_name`, `retention_time_min`,
///   `peak_height`, `peak_area`, `peak_width_fwhm_min`
///
/// Row 1 of each sheet is a bold header row.  Column widths are set to
/// accommodate typical numeric precision without truncation.
pub fn export_xlsx(chrom: &Chromatogram, output_path: &Path) -> Result<()> {
    let mut wb = Workbook::new();

    let bold = Format::new().set_bold();

    // ── Sheet 1: Signal ───────────────────────────────────────────────────
    {
        let ws = wb.add_worksheet();
        ws.set_name("Signal")
            .map_err(|e: XlsxError| anyhow::anyhow!("xlsx sheet name error: {e}"))?;

        // Header row
        ws.write_with_format(0, 0, "retention_time_min", &bold)
            .map_err(xlsx_err)?;
        ws.write_with_format(0, 1, "intensity", &bold)
            .map_err(xlsx_err)?;

        ws.set_column_width(0, 22.0).map_err(xlsx_err)?;
        ws.set_column_width(1, 16.0).map_err(xlsx_err)?;

        for (row, &(t, v)) in chrom.time_series.iter().enumerate() {
            let r = (row + 1) as u32;
            ws.write(r, 0, t).map_err(xlsx_err)?;
            ws.write(r, 1, v).map_err(xlsx_err)?;
        }
    }

    // ── Sheet 2: Peaks ────────────────────────────────────────────────────
    {
        let ws = wb.add_worksheet();
        ws.set_name("Peaks")
            .map_err(|e: XlsxError| anyhow::anyhow!("xlsx sheet name error: {e}"))?;

        // Header row
        let headers = [
            "peak_name",
            "retention_time_min",
            "peak_height",
            "peak_area",
            "peak_width_fwhm_min",
        ];
        let col_widths: [f64; 5] = [20.0, 22.0, 16.0, 16.0, 22.0];
        for (col, (&h, &w)) in headers.iter().zip(col_widths.iter()).enumerate() {
            ws.write_with_format(0, col as u16, h, &bold)
                .map_err(xlsx_err)?;
            ws.set_column_width(col as u16, w).map_err(xlsx_err)?;
        }

        for (row, p) in chrom.peaks.iter().enumerate() {
            let r = (row + 1) as u32;
            ws.write(r, 0, p.name.as_deref().unwrap_or(""))
                .map_err(xlsx_err)?;
            ws.write(r, 1, p.retention_time).map_err(xlsx_err)?;
            ws.write(r, 2, p.height).map_err(xlsx_err)?;
            ws.write(r, 3, p.area).map_err(xlsx_err)?;
            ws.write(r, 4, p.width).map_err(xlsx_err)?;
        }
    }

    wb.save(output_path)
        .map_err(|e| anyhow::anyhow!("failed to save XLSX: {e}"))?;
    Ok(())
}

/// Convert an [`XlsxError`] into an [`anyhow::Error`].
fn xlsx_err(e: XlsxError) -> anyhow::Error {
    anyhow::anyhow!("xlsx write error: {e}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Metadata, Peak};

    fn sample_chrom() -> Chromatogram {
        Chromatogram {
            metadata: Metadata {
                instrument: "Test-1260".into(),
                method: "C18_gradient".into(),
                sample_id: "QC-001".into(),
                date: "2026-09-25".into(),
                detector_type: "DAD".into(),
            },
            time_series: vec![(0.0, 0.0), (1.0, 50.0), (2.0, 1000.0), (3.0, 50.0), (4.0, 0.0)],
            peaks: vec![Peak {
                retention_time: 2.0,
                area: 1100.0,
                height: 1000.0,
                width: 0.94,
                name: Some("Caffeine".into()),
            }],
        }
    }

    // ── JSON ────────────────────────────────────────────────────────────────

    #[test]
    fn test_export_json_round_trip() {
        let chrom = sample_chrom();
        let path = std::env::temp_dir().join("lab_export_test.json");

        export_json(&chrom, &path).expect("json export should succeed");

        let raw = std::fs::read_to_string(&path).unwrap();
        let back: Chromatogram = serde_json::from_str(&raw).expect("round-trip deserialise");

        assert_eq!(back.metadata.sample_id, chrom.metadata.sample_id);
        assert_eq!(back.time_series.len(), chrom.time_series.len());
        assert_eq!(back.peaks.len(), chrom.peaks.len());
        assert_eq!(back.peaks[0].name.as_deref(), Some("Caffeine"));

        let _ = std::fs::remove_file(&path);
    }

    // ── CSV ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_export_csv_files_exist_and_have_header() {
        let chrom = sample_chrom();
        let base  = std::env::temp_dir().join("lab_export_test.csv");

        export_csv(&chrom, &base).expect("csv export should succeed");

        let tmp = std::env::temp_dir();
        let signal = tmp.join("lab_export_test_signal.csv");
        let peaks  = tmp.join("lab_export_test_peaks.csv");

        // Signal file: header + 5 data rows
        let sig_text = std::fs::read_to_string(&signal).unwrap();
        let sig_lines: Vec<&str> = sig_text.lines().collect();
        assert_eq!(sig_lines[0], "retention_time_min,intensity");
        assert_eq!(sig_lines.len(), 1 + chrom.time_series.len());

        // Peaks file: header + 1 data row, named column present
        let pk_text = std::fs::read_to_string(&peaks).unwrap();
        let pk_lines: Vec<&str> = pk_text.lines().collect();
        assert_eq!(
            pk_lines[0],
            "peak_name,retention_time_min,peak_height,peak_area,peak_width_fwhm_min"
        );
        assert_eq!(pk_lines.len(), 1 + chrom.peaks.len());
        assert!(pk_lines[1].starts_with("Caffeine,"));

        let _ = std::fs::remove_file(&signal);
        let _ = std::fs::remove_file(&peaks);
    }

    // ── XLSX ────────────────────────────────────────────────────────────────

    #[test]
    fn test_export_xlsx_file_created() {
        let chrom = sample_chrom();
        let path  = std::env::temp_dir().join("lab_export_test.xlsx");

        export_xlsx(&chrom, &path).expect("xlsx export should succeed");

        let meta = std::fs::metadata(&path).expect("xlsx file should exist");
        // A minimal xlsx is at least a few KB (it's a zip).
        assert!(meta.len() > 1024, "xlsx suspiciously small: {} bytes", meta.len());

        let _ = std::fs::remove_file(&path);
    }
}
