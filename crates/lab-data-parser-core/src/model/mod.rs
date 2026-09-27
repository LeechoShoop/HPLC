//! Domain model types for laboratory data.

use serde::{Deserialize, Serialize};

/// A single labelled measurement value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub label: String,
    pub value: f64,
    pub unit: Option<String>,
}

/// A collection of measurements from one run / sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSet {
    pub name: String,
    pub measurements: Vec<Measurement>,
}

// ---------------------------------------------------------------------------
// Unified chromatography data model
// ---------------------------------------------------------------------------

/// Instrument and run metadata associated with a chromatographic acquisition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    /// Name or model of the instrument that produced the data.
    pub instrument: String,
    /// Analytical method / gradient program used for the run.
    pub method: String,
    /// Unique identifier for the sample being analysed.
    pub sample_id: String,
    /// Acquisition date (ISO 8601 string recommended, e.g. "2026-09-25").
    pub date: String,
    /// Detector type (e.g. "UV", "FID", "MS", "DAD").
    pub detector_type: String,
}

/// A chromatographic peak identified within a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peak {
    /// Retention time at the peak apex, in minutes.
    pub retention_time: f64,
    /// Integrated peak area (instrument-unit dependent).
    pub area: f64,
    /// Peak height at the apex (instrument-unit dependent).
    pub height: f64,
    /// Peak width at half-maximum (or baseline width), in minutes.
    pub width: f64,
    /// Optional compound name or label assigned to this peak.
    pub name: Option<String>,
}

/// The unified representation of a single chromatographic run.
///
/// `time_series` holds `(time_minutes, signal)` pairs in acquisition order.
/// `peaks` holds all peaks detected or integrated from that signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chromatogram {
    /// Run-level metadata (instrument, method, sample, date, detector).
    pub metadata: Metadata,
    /// Raw signal trace as `(retention_time_min, detector_signal)` tuples.
    pub time_series: Vec<(f64, f64)>,
    /// Peaks identified in this chromatogram.
    pub peaks: Vec<Peak>,
}
