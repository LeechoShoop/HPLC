//! Automatic peak detection for chromatographic and mass-spectrometric data.
//!
//! ## Algorithm outline
//!
//! 1. **Baseline correction** – a rolling-minimum filter with a configurable
//!    window is applied to estimate the local baseline.  Each signal sample is
//!    then baseline-subtracted before any further processing.
//!
//! 2. **Local-maxima detection** – a point is a candidate peak apex if it is
//!    strictly greater than both of its immediate neighbours *and* its
//!    baseline-corrected height exceeds `threshold`.
//!
//! 3. **Peak boundary walking** – from the apex the algorithm walks left and
//!    right until the signal drops below `threshold`, defining the integration
//!    window.
//!
//! 4. **Area** – trapezoidal integration over the boundary window.
//!
//! 5. **Width** – full-width at half-maximum (FWHM), estimated by linear
//!    interpolation on the left and right flanks to find where the signal
//!    crosses `apex_height / 2`.
//!
//! 6. **Filtering** – peaks whose FWHM is smaller than `min_width` are
//!    discarded to suppress noise spikes.

use crate::model::Peak;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detect peaks in a baseline-corrected signal.
///
/// # Parameters
/// | Name         | Meaning |
/// |--------------|---------|
/// | `series`     | `(x, y)` pairs in ascending x order |
/// | `threshold`  | minimum baseline-corrected height for a peak to be reported |
/// | `min_width`  | minimum FWHM (same units as x) – spikes narrower than this are discarded |
///
/// Returns a `Vec<Peak>` sorted by `retention_time`.
pub fn detect_peaks(series: &[(f64, f64)], threshold: f64, min_width: f64) -> Vec<Peak> {
    if series.len() < 3 {
        return Vec::new();
    }

    // ── Step 1: baseline correction ────────────────────────────────────────
    // Window = ~10 % of the series length, at least 3 points.  A wider
    // window means the rolling minimum tracks the true background beneath
    // a peak rather than clipping its tails.
    let window = (series.len() / 10).max(3);
    let baseline = rolling_minimum_baseline(series, window);
    let corrected: Vec<(f64, f64)> = series
        .iter()
        .zip(baseline.iter())
        .map(|(&(x, y), &b)| (x, y - b))
        .collect();

    // ── Step 2: local maxima above threshold ───────────────────────────────
    let n = corrected.len();
    let mut peaks = Vec::new();

    for i in 1..n - 1 {
        let (_, yc) = corrected[i];
        if yc <= threshold {
            continue;
        }
        if yc <= corrected[i - 1].1 || yc <= corrected[i + 1].1 {
            continue;
        }

        // ── Step 3: boundary walk ──────────────────────────────────────────
        let left  = boundary_left(&corrected, i, threshold);
        let right = boundary_right(&corrected, i, threshold);

        // ── Step 4: trapezoidal area ───────────────────────────────────────
        let area = trapezoid_area(&corrected[left..=right]);

        // ── Step 5: FWHM ──────────────────────────────────────────────────
        let half = yc / 2.0;
        let x_left  = interp_left_crossing(&corrected,  i, half, left);
        let x_right = interp_right_crossing(&corrected, i, half, right);
        let width = x_right - x_left;

        // ── Step 6: filter narrow spikes ──────────────────────────────────
        if width < min_width {
            continue;
        }

        peaks.push(Peak {
            retention_time: corrected[i].0,
            area,
            height: yc,
            width,
            name: None,
        });
    }

    // ── Step 7: merge peaks closer than min_width (keep the taller) ───────
    // Baseline artefacts can leave two very close apexes for a single true
    // peak.  We collapse any pair whose centres are within min_width of each
    // other, retaining the one with the greater height.
    merge_nearby_peaks(peaks, min_width)
}

/// Collapse peaks that are closer than `min_width` to each other.
///
/// The list is assumed to be in retention-time order (which it will be,
/// because we scanned left-to-right).  We do one forward pass, merging each
/// peak into the previous one if they are within `min_width`.
fn merge_nearby_peaks(mut peaks: Vec<Peak>, min_width: f64) -> Vec<Peak> {
    if peaks.len() < 2 {
        return peaks;
    }
    let mut merged: Vec<Peak> = Vec::with_capacity(peaks.len());
    // Drain so we can move out of peaks without cloning.
    for p in peaks.drain(..) {
        if let Some(last) = merged.last_mut() {
            if (p.retention_time - last.retention_time).abs() < min_width {
                // Keep whichever has the greater height; sum the areas.
                if p.height > last.height {
                    last.retention_time = p.retention_time;
                    last.height         = p.height;
                    last.width          = p.width;
                }
                last.area += p.area;
                continue;
            }
        }
        merged.push(p);
    }
    merged
}

// ---------------------------------------------------------------------------
// Baseline correction
// ---------------------------------------------------------------------------

/// Compute a rolling-minimum baseline.
///
/// For each point `i`, the baseline is the minimum y-value in the window
/// `[i - half_w, i + half_w]` (clamped to array bounds).
fn rolling_minimum_baseline(series: &[(f64, f64)], window: usize) -> Vec<f64> {
    let n = series.len();
    let half = window / 2;
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(n);
            series[lo..hi]
                .iter()
                .map(|&(_, y)| y)
                .fold(f64::INFINITY, f64::min)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Boundary walkers
// ---------------------------------------------------------------------------

/// Walk left from `apex` until the corrected signal falls to or below
/// `threshold`, returning the leftmost index still above threshold.
fn boundary_left(corrected: &[(f64, f64)], apex: usize, threshold: f64) -> usize {
    let mut i = apex;
    while i > 0 && corrected[i - 1].1 > threshold {
        i -= 1;
    }
    i
}

/// Walk right from `apex` until the corrected signal falls to or below
/// `threshold`, returning the rightmost index still above threshold.
fn boundary_right(corrected: &[(f64, f64)], apex: usize, threshold: f64) -> usize {
    let mut i = apex;
    let n = corrected.len();
    while i + 1 < n && corrected[i + 1].1 > threshold {
        i += 1;
    }
    i
}

// ---------------------------------------------------------------------------
// Trapezoidal integration
// ---------------------------------------------------------------------------

/// Integrate a slice of `(x, y)` pairs with the trapezoidal rule.
fn trapezoid_area(slice: &[(f64, f64)]) -> f64 {
    slice
        .windows(2)
        .map(|w| {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            0.5 * (y0 + y1) * (x1 - x0)
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Half-maximum crossing interpolation
// ---------------------------------------------------------------------------

/// Walk left from `apex` to find where the signal crosses `half` and return
/// the interpolated x position.  Falls back to the leftmost boundary x.
fn interp_left_crossing(
    corrected: &[(f64, f64)],
    apex: usize,
    half: f64,
    left_bound: usize,
) -> f64 {
    // Search from apex going left.
    for i in (left_bound + 1..=apex).rev() {
        let (x1, y1) = corrected[i];
        let (x0, y0) = corrected[i - 1];
        if y0 <= half && y1 >= half {
            // Linear interpolation between (x0, y0) and (x1, y1).
            let t = (half - y0) / (y1 - y0);
            return x0 + t * (x1 - x0);
        }
    }
    corrected[left_bound].0
}

/// Walk right from `apex` to find where the signal crosses `half` and return
/// the interpolated x position.  Falls back to the rightmost boundary x.
fn interp_right_crossing(
    corrected: &[(f64, f64)],
    apex: usize,
    half: f64,
    right_bound: usize,
) -> f64 {
    for i in apex..right_bound {
        let (x0, y0) = corrected[i];
        let (x1, y1) = corrected[i + 1];
        if y0 >= half && y1 <= half {
            let t = (half - y0) / (y1 - y0);
            return x0 + t * (x1 - x0);
        }
    }
    corrected[right_bound].0
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Discrete Gaussian: amplitude * exp(-(x - mu)^2 / (2 * sigma^2)).
    fn gaussian(x: f64, mu: f64, sigma: f64, amplitude: f64) -> f64 {
        amplitude * (-(x - mu).powi(2) / (2.0 * sigma.powi(2))).exp()
    }

    /// Analytic Gaussian area: amplitude * sigma * sqrt(2π).
    fn gaussian_area(amplitude: f64, sigma: f64) -> f64 {
        amplitude * sigma * (2.0 * PI).sqrt()
    }

    /// Build a uniform grid [x_min, x_max] with `n` points.
    fn linspace(x_min: f64, x_max: f64, n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| x_min + i as f64 * (x_max - x_min) / (n - 1) as f64)
            .collect()
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    /// Single Gaussian peak on a flat zero baseline.
    ///
    /// Checks:
    /// - exactly one peak is found
    /// - retention_time is within 1 % of the true centre
    /// - area is within 2 % of the analytic Gaussian area
    #[test]
    fn test_single_gaussian_peak() {
        let mu = 5.0_f64;
        let sigma = 0.3_f64;
        let amplitude = 1000.0_f64;
        let n = 1001;

        let xs = linspace(-10.0, 20.0, n);
        let series: Vec<(f64, f64)> = xs
            .iter()
            .map(|&x| (x, gaussian(x, mu, sigma, amplitude)))
            .collect();

        let peaks = detect_peaks(&series, 10.0, 0.05);

        assert_eq!(peaks.len(), 1, "expected exactly 1 peak, got {}", peaks.len());

        let p = &peaks[0];
        assert!(
            (p.retention_time - mu).abs() < 0.01 * mu,
            "retention_time {:.4} deviates more than 1 % from {mu}",
            p.retention_time
        );

        let analytic_area = gaussian_area(amplitude, sigma);
        let area_err = (p.area - analytic_area).abs() / analytic_area;
        assert!(
            area_err < 0.02,
            "area {:.2} deviates {:.1} % from analytic {analytic_area:.2}",
            p.area,
            area_err * 100.0
        );
    }

    /// Two separated Gaussian peaks – detector must find both independently.
    #[test]
    fn test_two_separate_peaks() {
        let n = 1000;
        let xs = linspace(-10.0, 30.0, n);
        let series: Vec<(f64, f64)> = xs
            .iter()
            .map(|&x| {
                let y = gaussian(x, 5.0, 0.3, 800.0) + gaussian(x, 14.0, 0.4, 600.0);
                (x, y)
            })
            .collect();

        let peaks = detect_peaks(&series, 10.0, 0.05);

        assert_eq!(peaks.len(), 2, "expected 2 peaks, got {}", peaks.len());
        assert!((peaks[0].retention_time - 5.0).abs() < 0.05);
        assert!((peaks[1].retention_time - 14.0).abs() < 0.05);
    }

    /// A signal with only noise below threshold must yield no peaks.
    #[test]
    fn test_no_peaks_below_threshold() {
        let series: Vec<(f64, f64)> = (0..200)
            .map(|i| (i as f64 * 0.1, 5.0)) // flat signal at 5.0
            .collect();

        let peaks = detect_peaks(&series, 10.0, 0.01);
        assert!(peaks.is_empty(), "expected no peaks, got {}", peaks.len());
    }

    /// A very narrow spike narrower than min_width must be filtered out.
    #[test]
    fn test_spike_filtered_by_min_width() {
        // Gaussian with sigma = 0.01 → FWHM ≈ 0.024
        let n = 500;
        let xs = linspace(0.0, 10.0, n);
        let series: Vec<(f64, f64)> = xs
            .iter()
            .map(|&x| (x, gaussian(x, 5.0, 0.01, 5000.0)))
            .collect();

        // min_width = 0.1 is wider than the spike FWHM ≈ 0.024 → filtered
        let peaks = detect_peaks(&series, 10.0, 0.1);
        assert!(
            peaks.is_empty(),
            "spike should be filtered by min_width; got {} peaks",
            peaks.len()
        );
    }
}
