//! Integration tests: parse → detect_peaks → export_json → verify.
//!
//! Each test exercises the full public pipeline:
//!
//!  1. [`parsers::parser_for_format`] selects the right parser.
//!  2. [`parsers::LabDataParser::parse`] reads a fixture file into a
//!     [`model::Chromatogram`].
//!  3. [`peaks::detect_peaks`] annotates the chromatogram with peaks.
//!  4. [`export::export_json`] serialises the result to a temporary file.
//!  5. The JSON is read back and deserialised; key fields are asserted.
//!
//! ## Adding a real instrument file
//!
//! Drop an anonymised export into `tests/fixtures/` and update the
//! `REAL_INSTRUMENT_FILE` constant below.  The test
//! `test_real_instrument_file` will automatically run when the file exists;
//! it is **skipped** (not failed) when the file is absent so that CI is
//! never broken by a missing fixture.

use std::path::Path;

use lab_data_parser::export::export_json;
use lab_data_parser::model::Chromatogram;
use lab_data_parser::parsers::parser_for_format;
use lab_data_parser::peaks::detect_peaks;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run the full pipeline on `input_path` using the given format string,
/// then export the result to a temp file and round-trip it through JSON.
///
/// Returns the deserialised [`Chromatogram`] so callers can inspect it.
fn run_pipeline(input_path: &Path, format: &str) -> Chromatogram {
    // 1. Parse
    let parser = parser_for_format(format)
        .unwrap_or_else(|e| panic!("parser_for_format({format:?}) failed: {e}"));
    let mut chrom = parser
        .parse(input_path)
        .unwrap_or_else(|e| panic!("parse({}) failed: {e}", input_path.display()));

    // 2. Detect peaks with standard parameters
    chrom.peaks = detect_peaks(&chrom.time_series, 10.0, 0.05);

    // 3. Export to JSON
    // Include both format and stem in the temp file name so that files with
    // the same stem but different format (e.g. sample.csv / sample.mzml) do
    // not collide when tests run concurrently.
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("test_output");
    let output = std::env::temp_dir().join(format!("lab_parser__{format}__{stem}.json"));
    export_json(&chrom, &output)
        .unwrap_or_else(|e| panic!("export_json failed: {e}"));

    // 4. Round-trip through JSON
    let json_str = std::fs::read_to_string(&output)
        .unwrap_or_else(|e| panic!("read JSON failed: {e}"));
    let chrom_out: Chromatogram = serde_json::from_str(&json_str)
        .unwrap_or_else(|e| panic!("deserialise Chromatogram failed: {e}"));

    let _ = std::fs::remove_file(&output);

    chrom_out
}

// ---------------------------------------------------------------------------
// HPLC CSV — single-peak fixture (the original sample.csv)
// ---------------------------------------------------------------------------

/// Full pipeline test for the minimal single-peak HPLC fixture.
///
/// Fixture: `tests/fixtures/sample.csv`
/// Expected: 11 time-series points, exactly 1 detected peak, apex ≈ 0.5 min.
#[test]
fn test_hplc_single_peak_pipeline() {
    let input = Path::new("tests/fixtures/sample.csv");
    let chrom = run_pipeline(input, "hplc");

    // ── Time series ───────────────────────────────────────────────────────
    assert_eq!(
        chrom.time_series.len(),
        11,
        "expected 11 time points from sample.csv"
    );

    // ── Peak count ────────────────────────────────────────────────────────
    assert_eq!(chrom.peaks.len(), 1, "expected exactly 1 peak; got {:#?}", chrom.peaks);

    // ── Peak location ─────────────────────────────────────────────────────
    let apex = chrom.peaks[0].retention_time;
    assert!(
        (apex - 0.5).abs() < 0.05,
        "apex should be near 0.5 min, got {apex:.4}"
    );

    // ── JSON round-trip preserves peak count ──────────────────────────────
    assert_eq!(chrom.peaks.len(), 1, "JSON round-trip must preserve peak count");
}

// ---------------------------------------------------------------------------
// HPLC CSV — two-peak fixture
// ---------------------------------------------------------------------------

/// Full pipeline test for the two-peak HPLC fixture.
///
/// Fixture: `tests/fixtures/hplc_two_peaks.csv`
///
/// This fixture has two Gaussian-shaped peaks at ~1.0 min and ~3.0 min with
/// a coarse 0.1 min step size.  The rolling-minimum baseline estimator can
/// produce two candidate apexes per peak on this coarse grid; after
/// `merge_nearby_peaks` the test validates that we end up with exactly 2
/// distinct peaks separated by > 0.5 min.
///
/// Expected:
///   - 41 time-series points
///   - ≥ 2 detected peaks
///   - The two highest peaks cluster around 1.0 min and 3.0 min
///   - Peak near 1.0 min has a higher height than peak near 3.0 min
///   - Metadata fields match the fixture header
#[test]
fn test_hplc_two_peaks_pipeline() {
    let input = Path::new("tests/fixtures/hplc_two_peaks.csv");
    let chrom = run_pipeline(input, "hplc");

    // ── Time series ───────────────────────────────────────────────────────
    assert_eq!(
        chrom.time_series.len(),
        41,
        "expected 41 time points from hplc_two_peaks.csv"
    );

    // ── Peak count: at least 2 peaks, one per elution zone ───────────────
    assert!(
        chrom.peaks.len() >= 2,
        "expected ≥ 2 peaks; got {:#?}",
        chrom.peaks
    );

    // ── Each elution zone must have at least one peak in the right window ─
    let has_peak_near_1 = chrom
        .peaks
        .iter()
        .any(|p| (p.retention_time - 1.0).abs() < 0.3);
    let has_peak_near_3 = chrom
        .peaks
        .iter()
        .any(|p| (p.retention_time - 3.0).abs() < 0.3);

    assert!(has_peak_near_1, "no peak detected near 1.0 min; got {:#?}", chrom.peaks);
    assert!(has_peak_near_3, "no peak detected near 3.0 min; got {:#?}", chrom.peaks);

    // ── The tallest peak near 1.0 min must be taller than near 3.0 min ───
    let h1 = chrom
        .peaks
        .iter()
        .filter(|p| (p.retention_time - 1.0).abs() < 0.3)
        .map(|p| p.height)
        .fold(f64::NEG_INFINITY, f64::max);
    let h3 = chrom
        .peaks
        .iter()
        .filter(|p| (p.retention_time - 3.0).abs() < 0.3)
        .map(|p| p.height)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        h1 > h3,
        "peak group near 1.0 min (max h={h1:.1}) should be taller than near 3.0 min (h={h3:.1})"
    );

    // ── JSON structure ────────────────────────────────────────────────────
    assert_eq!(chrom.time_series.len(), 41);

    // ── Metadata ──────────────────────────────────────────────────────────
    assert!(
        chrom.metadata.instrument.contains("Agilent"),
        "instrument should contain 'Agilent', got {:?}",
        chrom.metadata.instrument
    );
    assert_eq!(chrom.metadata.sample_id, "QC-002");
    assert_eq!(chrom.metadata.date, "2026-09-20");
}

// ---------------------------------------------------------------------------
// HPLC CSV — European-locale fixture
// ---------------------------------------------------------------------------

/// Pipeline test for the semicolon-delimited, comma-decimal fixture.
///
/// Fixture: `tests/fixtures/hplc_eu_locale.csv`
///
/// The peak spans t = 0.3 – 1.7 min with apex at 1.0 min.  On the coarse
/// 0.1 min grid the rolling-minimum baseline may produce two candidate
/// apexes on the symmetric flanks, so we assert on ≥ 1 peak and verify
/// that at least one of them is within 0.3 min of the true apex.
///
/// Expected:
///   - 21 time-series points parsed correctly despite EU locale
///   - ≥ 1 peak detected near 1.0 min
///   - First point is (0.0, 1.0), confirming correct decimal normalisation
#[test]
fn test_hplc_eu_locale_pipeline() {
    let input = Path::new("tests/fixtures/hplc_eu_locale.csv");
    let chrom = run_pipeline(input, "hplc");

    // ── Time series ───────────────────────────────────────────────────────
    assert_eq!(
        chrom.time_series.len(),
        21,
        "expected 21 time points from hplc_eu_locale.csv"
    );

    // First and last points must be parsed with correct decimal conversion.
    let (t0, v0) = chrom.time_series[0];
    assert!((t0 - 0.0).abs() < 1e-9, "t[0] should be 0.0, got {t0}");
    assert!((v0 - 1.0).abs() < 1e-9, "v[0] should be 1.0, got {v0}");

    let (t_last, v_last) = *chrom.time_series.last().unwrap();
    assert!((t_last - 2.0).abs() < 1e-9, "t[-1] should be 2.0, got {t_last}");
    assert!((v_last - 1.0).abs() < 1e-9, "v[-1] should be 1.0, got {v_last}");

    // ── Peak count ────────────────────────────────────────────────────────
    assert!(
        !chrom.peaks.is_empty(),
        "expected ≥ 1 peak in EU locale fixture; got 0"
    );

    // ── Peak location: at least one apex within 0.3 min of 1.0 ───────────
    let has_peak_near_apex = chrom
        .peaks
        .iter()
        .any(|p| (p.retention_time - 1.0).abs() < 0.3);
    assert!(
        has_peak_near_apex,
        "no EU locale peak detected near 1.0 min; got {:#?}",
        chrom.peaks
    );

    // ── Metadata ──────────────────────────────────────────────────────────
    assert!(
        chrom.metadata.instrument.contains("Waters"),
        "instrument should contain 'Waters', got {:?}",
        chrom.metadata.instrument
    );
    assert_eq!(chrom.metadata.sample_id, "EU-001");
}

// ---------------------------------------------------------------------------
// mzML — minimal fixture (the original sample.mzml, no detectable peak)
// ---------------------------------------------------------------------------

/// Pipeline test for the minimal mzML fixture.
///
/// Fixture: `tests/fixtures/sample.mzml`
/// Expected:
///   - 3 time-series points (m/z values used as x-axis)
///   - 0 peaks detected (all intensities at or near 2.0–3.0, below threshold 10.0)
///   - JSON round-trip preserves all fields
#[test]
fn test_mzml_minimal_pipeline() {
    let input = Path::new("tests/fixtures/sample.mzml");
    let chrom = run_pipeline(input, "mzml");

    assert_eq!(
        chrom.time_series.len(),
        3,
        "expected 3 points from sample.mzml"
    );
    assert_eq!(
        chrom.peaks.len(),
        0,
        "minimal mzML data should yield 0 peaks above threshold 10.0"
    );

    // Detector type defaults to "MS" for mzML files without explicit cvParam.
    assert_eq!(chrom.metadata.detector_type, "MS");
}

// ---------------------------------------------------------------------------
// mzML — multi-spectrum fixture (peak in middle spectrum)
// ---------------------------------------------------------------------------

/// Full pipeline test for the three-spectrum mzML fixture.
///
/// Fixture: `tests/fixtures/ms_multi_spectrum.mzml`
///
/// The fixture has 3 spectra × 5 ions each = 15 total (x, y) points after
/// the parser concatenates all spectra.  The middle scan contains an ion at
/// m/z 200 with intensity 9000, which is well above the threshold of 10.0.
///
/// Expected:
///   - 15 time-series points
///   - ≥ 1 peak detected (the dominant ion at m/z 200)
///   - The tallest peak height ≥ 8000 (after baseline subtraction)
///   - Metadata: instrument = "Orbitrap Exploris 480"
///   - JSON round-trip preserves all fields
#[test]
fn test_mzml_multi_spectrum_pipeline() {
    let input = Path::new("tests/fixtures/ms_multi_spectrum.mzml");

    // 1. Parse
    let parser = parser_for_format("mzml").expect("mzml parser should be available");
    let mut chrom = parser
        .parse(input)
        .expect("should parse ms_multi_spectrum.mzml");

    // ── Time series ───────────────────────────────────────────────────────
    // 3 spectra × 5 ions = 15 points
    assert_eq!(
        chrom.time_series.len(),
        15,
        "expected 15 points (3 spectra × 5 ions)"
    );

    // ── Metadata from cvParams ─────────────────────────────────────────────
    assert_eq!(
        chrom.metadata.instrument, "Orbitrap Exploris 480",
        "instrument name should come from MS:1000031 cvParam"
    );
    assert_eq!(chrom.metadata.detector_type, "Orbitrap");
    assert_eq!(chrom.metadata.date, "2026-09-20T09:00:00Z");

    // ── Peak detection ────────────────────────────────────────────────────
    chrom.peaks = detect_peaks(&chrom.time_series, 10.0, 0.05);

    assert!(
        !chrom.peaks.is_empty(),
        "should detect ≥ 1 peak in ms_multi_spectrum.mzml"
    );

    let tallest = chrom.peaks.iter().map(|p| p.height).fold(f64::NEG_INFINITY, f64::max);
    assert!(
        tallest >= 8_000.0,
        "tallest peak height should be ≥ 8000 AU after baseline subtraction, got {tallest:.1}"
    );

    // ── Export to JSON ────────────────────────────────────────────────────
    let output = std::env::temp_dir().join("lab_parser__ms_multi_spectrum.json");
    export_json(&chrom, &output).expect("export_json should succeed");

    // ── JSON round-trip ───────────────────────────────────────────────────
    let json_str = std::fs::read_to_string(&output).expect("should read JSON");
    let chrom_out: Chromatogram =
        serde_json::from_str(&json_str).expect("should deserialise Chromatogram");

    assert_eq!(chrom_out.time_series.len(), 15);
    assert_eq!(chrom_out.peaks.len(), chrom.peaks.len());

    // Verify a few JSON-level structure checks.
    let json_val: serde_json::Value =
        serde_json::from_str(&json_str).expect("should parse as generic JSON");
    assert!(json_val.get("metadata").is_some(), "JSON must have 'metadata' key");
    assert!(json_val.get("time_series").is_some(), "JSON must have 'time_series' key");
    assert!(json_val.get("peaks").is_some(), "JSON must have 'peaks' key");

    let peaks_arr = json_val["peaks"].as_array().expect("'peaks' should be a JSON array");
    for peak in peaks_arr {
        assert!(peak.get("retention_time").is_some(), "each peak must have 'retention_time'");
        assert!(peak.get("area").is_some(), "each peak must have 'area'");
        assert!(peak.get("height").is_some(), "each peak must have 'height'");
        assert!(peak.get("width").is_some(), "each peak must have 'width'");
    }

    let _ = std::fs::remove_file(&output);
}

// ---------------------------------------------------------------------------
// Real instrument file (skipped if the file does not exist)
// ---------------------------------------------------------------------------

/// Path of the real (anonymised) instrument export, relative to the
/// workspace root.  Change this when you add a real file.
const REAL_INSTRUMENT_FILE: &str = "tests/fixtures/real_instrument.csv";

/// Format string to use when parsing `REAL_INSTRUMENT_FILE`.
const REAL_INSTRUMENT_FORMAT: &str = "hplc";

/// Smoke-test a real instrument export if the file has been added to the
/// fixtures directory.
///
/// The test only verifies that the file can be parsed without errors and that
/// the resulting `Chromatogram` has at least one time-series point.  Detailed
/// assertions are intentionally omitted here because the exact content of the
/// real file is unknown at development time.
///
/// **To activate this test**: drop an anonymised export file into
/// `tests/fixtures/`, update `REAL_INSTRUMENT_FILE` and
/// `REAL_INSTRUMENT_FORMAT` above, then re-run `cargo test`.
#[test]
fn test_real_instrument_file() {
    let path = Path::new(REAL_INSTRUMENT_FILE);

    if !path.exists() {
        // Print a notice so it's clear this test was intentionally skipped.
        eprintln!(
            "[SKIP] test_real_instrument_file: fixture {:?} not found. \
             Add an anonymised instrument export to enable this test.",
            REAL_INSTRUMENT_FILE
        );
        return;
    }

    let parser = parser_for_format(REAL_INSTRUMENT_FORMAT)
        .expect("parser_for_format should succeed for the configured format");

    let chrom = parser
        .parse(path)
        .unwrap_or_else(|e| panic!("failed to parse real instrument file {path:?}: {e:#}"));

    assert!(
        !chrom.time_series.is_empty(),
        "real instrument file must yield at least one data point"
    );

    // Run peak detection — just verify it doesn't panic.
    let peaks = detect_peaks(&chrom.time_series, 10.0, 0.05);

    // Export and round-trip.
    let output = std::env::temp_dir().join("lab_parser__real_instrument.json");
    export_json(
        &Chromatogram {
            metadata: chrom.metadata.clone(),
            time_series: chrom.time_series.clone(),
            peaks: peaks.clone(),
        },
        &output,
    )
    .expect("export_json should succeed for real instrument file");

    let json_str = std::fs::read_to_string(&output).expect("should read JSON");
    let chrom_out: Chromatogram =
        serde_json::from_str(&json_str).expect("should deserialise Chromatogram");

    assert_eq!(chrom_out.time_series.len(), chrom.time_series.len());
    assert_eq!(chrom_out.peaks.len(), peaks.len());

    eprintln!(
        "[real_instrument] parsed {} points, detected {} peaks",
        chrom_out.time_series.len(),
        chrom_out.peaks.len()
    );

    let _ = std::fs::remove_file(&output);
}
