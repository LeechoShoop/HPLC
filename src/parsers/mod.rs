//! Parsers for laboratory data formats (CSV, JSON, mzML, …).
//!
//! # Extension point
//!
//! Every parser implements [`LabDataParser`].  New formats only need to add a
//! zero-sized struct and a `impl LabDataParser for MyParser` block; the rest
//! of the binary picks them up via [`parser_for_format`].

pub mod csv_parser;
pub mod hplc_csv;
pub mod json_parser;
pub mod mzml;

use std::path::Path;

use anyhow::{Context, Result};

use crate::model::Chromatogram;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Unified interface for all laboratory data parsers.
///
/// Implementors read a file from disk and return a [`Chromatogram`] that maps
/// the format's x-axis (time, m/z, wavenumber, …) to `time_series.0` and
/// the signal to `time_series.1`.
pub trait LabDataParser {
    fn parse(&self, input: &Path) -> Result<Chromatogram>;
}

// ---------------------------------------------------------------------------
// HplcCsvParser
// ---------------------------------------------------------------------------

/// Parser for Agilent/Waters-style HPLC CSV text exports.
///
/// Delegates to [`hplc_csv::parse_hplc_csv`] after reading the file to a
/// `String` (HPLC CSVs are always plain text).
pub struct HplcCsvParser;

impl LabDataParser for HplcCsvParser {
    fn parse(&self, input: &Path) -> Result<Chromatogram> {
        let text = std::fs::read_to_string(input)
            .with_context(|| format!("cannot read HPLC CSV file: {}", input.display()))?;
        hplc_csv::parse_hplc_csv(&text)
            .with_context(|| format!("HPLC CSV parse failed for: {}", input.display()))
    }
}

// ---------------------------------------------------------------------------
// MzMlParser
// ---------------------------------------------------------------------------

/// Parser for HUPO-PSI mzML mass-spectrometry files.
///
/// Delegates to [`mzml::parse_mzml`] after reading the file to raw bytes
/// (mzML is UTF-8 XML but the inner binary arrays are opaque bytes).
pub struct MzMlParser;

impl LabDataParser for MzMlParser {
    fn parse(&self, input: &Path) -> Result<Chromatogram> {
        let bytes = std::fs::read(input)
            .with_context(|| format!("cannot read mzML file: {}", input.display()))?;
        mzml::parse_mzml(&bytes)
            .with_context(|| format!("mzML parse failed for: {}", input.display()))
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Return the right [`LabDataParser`] implementation for a `--format` flag value.
///
/// | `format`        | Parser selected |
/// |-----------------|-----------------|
/// | `"hplc"`        | [`HplcCsvParser`] |
/// | `"ms"` / `"mzml"` | [`MzMlParser`] |
///
/// Returns [`anyhow::Error`] for any unrecognised string so the caller can
/// surface a clean error message to the user.
pub fn parser_for_format(format: &str) -> Result<Box<dyn LabDataParser>> {
    match format.to_ascii_lowercase().as_str() {
        "hplc" => Ok(Box::new(HplcCsvParser)),
        "ms" | "mzml" => Ok(Box::new(MzMlParser)),
        other => anyhow::bail!(
            "unknown format {:?}; valid values are: hplc, ms (or mzml)",
            other
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_returns_hplc_for_hplc_flag() {
        // We can't call .parse() without a real file, but we can verify that
        // parser_for_format succeeds and that an unknown format errors.
        assert!(parser_for_format("hplc").is_ok());
        assert!(parser_for_format("HPLC").is_ok()); // case-insensitive
    }

    #[test]
    fn factory_returns_ms_for_ms_and_mzml_flags() {
        assert!(parser_for_format("ms").is_ok());
        assert!(parser_for_format("mzml").is_ok());
        assert!(parser_for_format("MS").is_ok()); // case-insensitive
    }

    #[test]
    fn factory_errors_on_unknown_format() {
        match parser_for_format("csv") {
            Err(e) => assert!(e.to_string().contains("unknown format")),
            Ok(_)  => panic!("expected Err for unknown format"),
        }
    }
}
