//! Parser for Agilent/Waters-style HPLC text exports.
//!
//! ## Expected file layout
//!
//! ```text
//! [Header]
//! Instrument: 1260 Infinity II
//! Method: C18_gradient.M
//! Sample ID: QC-001
//! Date: 2026-09-25
//! Detector: DAD
//!
//! [Data]
//! Time,Value
//! 0.000,12.5
//! 0.017,14.3
//! ...
//! ```
//!
//! Both `,` and `;` are accepted as field delimiters, and both `.` and `,`
//! are accepted as the decimal separator (common in European locales).
//! Blank lines and lines that cannot be parsed are silently skipped.

use anyhow::Result;

use crate::model::{Chromatogram, Metadata, Peak};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse an HPLC CSV/text export from a string slice.
///
/// Returns a [`Chromatogram`] with an empty `peaks` list (peak detection is a
/// separate concern); callers that already have peak tables can populate
/// `Chromatogram::peaks` afterwards.
pub fn parse_hplc_csv(input: &str) -> Result<Chromatogram> {
    let mut instrument = String::new();
    let mut method = String::new();
    let mut sample_id = String::new();
    let mut date = String::new();
    let mut detector_type = String::new();
    let mut time_series: Vec<(f64, f64)> = Vec::new();

    let mut in_data_section = false;
    // Track whether we have already skipped the column-header row of [Data].
    let mut header_line_skipped = false;

    for raw_line in input.lines() {
        let line = raw_line.trim();

        // ── Section markers ────────────────────────────────────────────────
        if line.eq_ignore_ascii_case("[data]") {
            in_data_section = true;
            header_line_skipped = false;
            continue;
        }
        if line.starts_with('[') {
            in_data_section = false;
            continue;
        }

        // ── Blank / comment lines ──────────────────────────────────────────
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if !in_data_section {
            if is_data_header(line) {
                in_data_section = true;
                header_line_skipped = true;
                continue;
            }
            if let Some((t, v)) = parse_data_row(line) {
                in_data_section = true;
                header_line_skipped = true;
                time_series.push((t, v));
                continue;
            }
        }

        if in_data_section {
            // Skip the column-header row (e.g. "Time,Value").
            if !header_line_skipped {
                header_line_skipped = true;
                if parse_data_row(line).is_none() {
                    continue;
                }
            }

            // Parse a data row; silently ignore anything that fails.
            if let Some((t, v)) = parse_data_row(line) {
                time_series.push((t, v));
            }
        } else {
            parse_metadata_line(
                line,
                &mut instrument,
                &mut method,
                &mut sample_id,
                &mut date,
                &mut detector_type,
            );
        }
    }

    // ── Build result ───────────────────────────────────────────────────────
    if time_series.is_empty() {
        anyhow::bail!("no valid data rows found in HPLC CSV input");
    }

    let metadata = Metadata {
        instrument,
        method,
        sample_id,
        date,
        detector_type,
    };

    Ok(Chromatogram {
        metadata,
        time_series,
        peaks: Vec::<Peak>::new(),
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Detect the primary field delimiter used in `line`.
///
/// Preference order: `;` → `,` → `\t`.
///
/// Semicolons are checked first because European locale files commonly use
/// `;` as the column separator **and** `,` as the decimal character, so a
/// data row like `"0,000;10,2"` contains both symbols.  Preferring `;` lets
/// [`normalise_decimal`] handle the remaining comma correctly.
fn detect_delimiter(line: &str) -> char {
    if line.contains(';') {
        ';'
    } else if line.contains(',') {
        ','
    } else {
        '\t'
    }
}

/// Normalise a numeric token so it can be parsed by [`str::parse::<f64>`].
///
/// * `"1,234.56"` (thousands comma, period decimal) → `"1234.56"`
/// * `"1,5"` (European comma-decimal, no period)   → `"1.5"`
/// * `"1.5"` (standard)                            → `"1.5"`
fn normalise_decimal(s: &str) -> String {
    if s.contains('.') && s.contains(',') {
        // comma is thousands separator
        s.replace(',', "")
    } else {
        // comma is decimal separator (or absent)
        s.replace(',', ".")
    }
}

fn is_data_header(line: &str) -> bool {
    let delim = detect_delimiter(line);
    let fields = line.split(delim);
    let known_headers = ["time", "signal", "intensity", "retention_time", "response"];
    for field in fields {
        let f = field.trim().trim_matches('"').to_ascii_lowercase();
        if known_headers.contains(&f.as_str()) {
            return true;
        }
    }
    false
}

/// Try to parse a two-column data row into `(time, signal)`.
///
/// Returns `None` for any line that is blank, has only one column, or contains
/// values that cannot be converted to `f64`.
fn parse_data_row(line: &str) -> Option<(f64, f64)> {
    let delim = detect_delimiter(line);
    let mut fields = line.splitn(2, delim);

    let raw_t = fields.next()?.trim().trim_matches('"');
    let raw_v = fields.next()?.trim().trim_matches('"');

    let mut t = normalise_decimal(raw_t).parse::<f64>().ok()?;
    let mut v = normalise_decimal(raw_v).parse::<f64>().ok()?;

    if t == 0.0 { t = 0.0; }
    if v == 0.0 { v = 0.0; }

    Some((t, v))
}

/// Extract a metadata value from a `key: value` or `key = value` line and
/// store it in the matching field.  Unknown keys are silently ignored.
fn parse_metadata_line(
    line: &str,
    instrument: &mut String,
    method: &mut String,
    sample_id: &mut String,
    date: &mut String,
    detector_type: &mut String,
) {
    let sep = if line.contains(':') {
        ':'
    } else if line.contains('=') {
        '='
    } else {
        return;
    };

    let mut parts = line.splitn(2, sep);
    let key = if let Some(k) = parts.next() {
        k.trim().to_ascii_lowercase()
    } else {
        return;
    };
    let value = if let Some(v) = parts.next() {
        v.trim().to_owned()
    } else {
        return;
    };

    match key.as_str() {
        "instrument" | "instrument name" | "system" => *instrument = value,
        "method" | "method name" | "method file" => *method = value,
        "sample" | "sample id" | "sample name" | "sampleid" => *sample_id = value,
        "date" | "run date" | "acqdate" | "acquisition date" => *date = value,
        "detector" | "detector type" | "signal" => *detector_type = value,
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal inline sample that mimics an Agilent-style text export.
    const SAMPLE_CSV: &str = "\
[Header]
Instrument: 1260 Infinity II
Method: C18_gradient.M
Sample ID: QC-001
Date: 2026-09-25
Detector: DAD

[Data]
Time,Value
0.000,12.5
0.017,14.3
0.033,18.7
bad line without delimiter
0.050,22.1

0.067,19.4
";

    #[test]
    fn test_parse_hplc_csv_basic() {
        let chrom = parse_hplc_csv(SAMPLE_CSV).expect("parsing should succeed");

        // Metadata
        assert_eq!(chrom.metadata.instrument, "1260 Infinity II");
        assert_eq!(chrom.metadata.method, "C18_gradient.M");
        assert_eq!(chrom.metadata.sample_id, "QC-001");
        assert_eq!(chrom.metadata.date, "2026-09-25");
        assert_eq!(chrom.metadata.detector_type, "DAD");

        // The malformed line and blank lines are skipped → 5 valid points.
        assert_eq!(chrom.time_series.len(), 5);

        let (t0, v0) = chrom.time_series[0];
        assert!((t0 - 0.000).abs() < 1e-9);
        assert!((v0 - 12.5).abs() < 1e-9);

        let (t4, v4) = chrom.time_series[4];
        assert!((t4 - 0.067).abs() < 1e-9);
        assert!((v4 - 19.4).abs() < 1e-9);

        // Parser does not pre-populate peaks.
        assert!(chrom.peaks.is_empty());
    }

    /// Semicolon-delimited file with comma as decimal separator (EU locale).
    const SAMPLE_EU: &str = "\
[Header]
Instrument: Waters Acquity
Method: RP_method
Sample ID: EU-042
Date: 2026-01-15
Detector: UV

[Data]
Time;Intensity
0,000;10,2
0,017;12,8
0,033;15,6
";

    #[test]
    fn test_parse_hplc_csv_semicolon_comma_decimal() {
        let chrom = parse_hplc_csv(SAMPLE_EU).expect("EU-locale parsing should succeed");

        assert_eq!(chrom.metadata.instrument, "Waters Acquity");
        assert_eq!(chrom.time_series.len(), 3);

        let (t0, v0) = chrom.time_series[0];
        assert!((t0 - 0.0).abs() < 1e-9, "time mismatch: {t0}");
        assert!((v0 - 10.2).abs() < 1e-9, "signal mismatch: {v0}");
    }

    /// An empty [Data] section must return an error, not a panic or empty vec.
    #[test]
    fn test_parse_hplc_csv_no_data_returns_error() {
        let input = "[Header]\nInstrument: Test\n[Data]\nTime,Value\n";
        assert!(
            parse_hplc_csv(input).is_err(),
            "expected Err for empty data section"
        );
    }

    #[test]
    fn test_parse_hplc_csv_no_metadata_just_data() {
        let input = "Time,Signal\n0.00,10.0\n0.01,-0.0\n0.02,20.0\n";
        let chrom = parse_hplc_csv(input).expect("parsing should succeed without metadata");
        assert_eq!(chrom.time_series.len(), 3);
        
        let (t0, v0) = chrom.time_series[0];
        assert_eq!(t0, 0.0);
        assert_eq!(v0, 10.0);
        
        let (t1, v1) = chrom.time_series[1];
        assert_eq!(t1, 0.01);
        assert_eq!(v1, 0.0); // Tests negative zero normalization
        
        let (t2, v2) = chrom.time_series[2];
        assert_eq!(t2, 0.02);
        assert_eq!(v2, 20.0);
    }
}
