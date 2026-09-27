//! Parser for the mzML mass-spectrometry data format (HUPO-PSI standard).
//!
//! ## What this parser handles
//!
//! * Streams the XML with [`quick_xml`] — no full-document DOM allocation.
//! * Reads `<cvParam>` accession codes inside `<binaryDataArray>` to determine:
//!   - **Data kind**: m/z array (`MS:1000514`) or intensity array (`MS:1000515`).
//!   - **Precision**: 32-bit (`MS:1000521`) or 64-bit float (`MS:1000523`).
//!   - **Compression**: zlib (`MS:1000574`) or no compression (`MS:1000576`).
//! * Base64-decodes the `<binary>` element content, optionally zlib-inflates it,
//!   then reinterprets the bytes as IEEE-754 floats in little-endian order.
//! * Metadata is extracted from the first `<referenceableParamGroup>` /
//!   `<instrumentConfiguration>` / `<run>` elements that carry recognisable
//!   `<cvParam>` values.
//! * m/z is mapped to the x-axis and intensity to the y-axis of
//!   [`Chromatogram::time_series`].
//! * [`Chromatogram::peaks`] is left empty — peak picking is a separate step.
//!
//! ## Limitations
//!
//! Only MS level 1 spectra are currently collected.  The first spectrum's
//! arrays are concatenated with all subsequent ones so that the caller gets
//! the full ion-current picture across the run.

use std::io::Read as _;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use flate2::read::ZlibDecoder;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::model::{Chromatogram, Metadata, Peak};

// ---------------------------------------------------------------------------
// CV accession constants
// ---------------------------------------------------------------------------

const CV_MZ_ARRAY: &[u8]        = b"MS:1000514";
const CV_INTENSITY_ARRAY: &[u8] = b"MS:1000515";
const CV_32_BIT_FLOAT: &[u8]    = b"MS:1000521";
const CV_64_BIT_FLOAT: &[u8]    = b"MS:1000523";
const CV_ZLIB: &[u8]            = b"MS:1000574";
const CV_NO_COMPRESSION: &[u8]  = b"MS:1000576";

// Metadata CV codes
const CV_INSTRUMENT_MODEL: &[u8]   = b"MS:1000031";
const CV_INSTRUMENT_SERIAL: &[u8]  = b"MS:1000529";
const CV_SCAN_START_TIME: &[u8]    = b"MS:1000016";

// ---------------------------------------------------------------------------
// Internal state for one <binaryDataArray> block
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ArrayDesc {
    is_mz: bool,
    is_intensity: bool,
    is_64bit: bool,
    zlib: bool,
    /// Raw base64 text accumulated from <binary> … </binary>
    b64_text: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse an mzML document from a byte slice and return a [`Chromatogram`].
///
/// m/z values become the x-axis (`time_series.0`) and intensities become the
/// y-axis (`time_series.1`), matching the unified model's convention.
pub fn parse_mzml(input: &[u8]) -> Result<Chromatogram> {
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(true);

    let mut instrument   = String::new();
    let mut method       = String::new();
    let mut sample_id    = String::new();
    let mut date         = String::new();
    let mut detector_type = String::from("MS");   // sensible default

    // Accumulated across all spectra in the run.
    let mut mz_values:  Vec<f64> = Vec::new();
    let mut int_values: Vec<f64> = Vec::new();

    // State flags
    let mut in_binary_data_array = false;
    let mut in_binary            = false;
    let mut current_array        = ArrayDesc::default();
    let mut in_run               = false;
    let mut buf                  = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                match e.local_name().as_ref() {
                    b"run" => {
                        in_run = true;
                        // startTimeStamp is an XML attribute, not a cvParam.
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"startTimeStamp" {
                                date = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                            if attr.key.local_name().as_ref() == b"id"
                                || attr.key.local_name().as_ref() == b"sampleRef"
                            {
                                if sample_id.is_empty() {
                                    sample_id =
                                        String::from_utf8_lossy(&attr.value).into_owned();
                                }
                            }
                        }
                    }
                    b"binaryDataArray" if in_run => {
                        in_binary_data_array = true;
                        current_array = ArrayDesc::default();
                    }
                    b"binary" if in_binary_data_array => {
                        in_binary = true;
                    }
                    b"cvParam" => {
                        handle_cv_param(
                            e,
                            in_binary_data_array,
                            &mut current_array,
                            &mut instrument,
                            &mut method,
                            &mut detector_type,
                        );
                    }
                    _ => {}
                }
            }

            Ok(Event::Text(ref t)) if in_binary => {
                // Accumulate base64 text (may arrive in multiple chunks).
                current_array
                    .b64_text
                    .push_str(std::str::from_utf8(t.as_ref()).unwrap_or(""));
            }

            Ok(Event::End(ref e)) => match e.local_name().as_ref() {
                b"binary" if in_binary_data_array => {
                    in_binary = false;
                }
                b"binaryDataArray" if in_binary_data_array => {
                    in_binary_data_array = false;
                    // Decode the completed array and store it.
                    let values = decode_array(&current_array)
                        .context("failed to decode binaryDataArray")?;
                    if current_array.is_mz {
                        mz_values.extend_from_slice(&values);
                    } else if current_array.is_intensity {
                        int_values.extend_from_slice(&values);
                    }
                    current_array = ArrayDesc::default();
                }
                b"run" => {
                    in_run = false;
                }
                _ => {}
            },

            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("XML parse error: {e}")),
            _ => {}
        }
        buf.clear();
    }

    // ── Zip m/z + intensity into time_series ──────────────────────────────
    if mz_values.is_empty() {
        anyhow::bail!("no m/z data found in mzML input");
    }

    let time_series: Vec<(f64, f64)> = mz_values
        .into_iter()
        .zip(int_values.into_iter().chain(std::iter::repeat(0.0)))
        .collect();

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

/// Inspect a `<cvParam>` element and update parser state accordingly.
fn handle_cv_param(
    e: &quick_xml::events::BytesStart<'_>,
    in_binary_data_array: bool,
    arr: &mut ArrayDesc,
    instrument: &mut String,
    method: &mut String,
    detector_type: &mut String,
) {
    // Collect accession and value attributes in one pass.
    let mut accession: Option<Vec<u8>> = None;
    let mut value: Option<String>      = None;
    let mut name: Option<String>       = None;

    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"accession" => accession = Some(attr.value.to_vec()),
            b"value"     => value = Some(String::from_utf8_lossy(&attr.value).into_owned()),
            b"name"      => name  = Some(String::from_utf8_lossy(&attr.value).into_owned()),
            _ => {}
        }
    }

    let acc = match accession.as_deref() {
        Some(a) => a,
        None    => return,
    };

    if in_binary_data_array {
        match acc {
            CV_MZ_ARRAY           => arr.is_mz        = true,
            CV_INTENSITY_ARRAY    => arr.is_intensity  = true,
            CV_64_BIT_FLOAT       => arr.is_64bit      = true,
            CV_32_BIT_FLOAT       => arr.is_64bit      = false,
            CV_ZLIB               => arr.zlib          = true,
            CV_NO_COMPRESSION     => arr.zlib          = false,
            _ => {}
        }
        return;
    }

    // Metadata cvParams (outside binaryDataArray).
    match acc {
        CV_INSTRUMENT_MODEL | CV_INSTRUMENT_SERIAL => {
            if instrument.is_empty() {
                *instrument = name.or(value).unwrap_or_default();
            }
        }
        CV_SCAN_START_TIME => {
            if method.is_empty() {
                *method = value.unwrap_or_default();
            }
        }
        // MS detector type
        b"MS:1000026" => {
            *detector_type = name.unwrap_or_else(|| "MS".into());
        }
        _ => {}
    }
}

/// Decode a completed [`ArrayDesc`]: base64 → optional zlib → f64 values.
fn decode_array(arr: &ArrayDesc) -> Result<Vec<f64>> {
    // 1. Base64 decode (strip any embedded whitespace first).
    let clean: String = arr.b64_text.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    let compressed_bytes = B64
        .decode(clean.as_bytes())
        .context("base64 decode failed")?;

    // 2. Optionally decompress.
    let raw_bytes: Vec<u8> = if arr.zlib {
        let mut decoder = ZlibDecoder::new(compressed_bytes.as_slice());
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).context("zlib decompression failed")?;
        out
    } else {
        compressed_bytes
    };

    // 3. Reinterpret bytes as little-endian IEEE-754 floats.
    let values = if arr.is_64bit {
        if raw_bytes.len() % 8 != 0 {
            anyhow::bail!("64-bit array byte length {} is not a multiple of 8", raw_bytes.len());
        }
        raw_bytes
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
            .collect()
    } else {
        if raw_bytes.len() % 4 != 0 {
            anyhow::bail!("32-bit array byte length {} is not a multiple of 4", raw_bytes.len());
        }
        raw_bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()) as f64)
            .collect()
    };

    Ok(values)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

    /// Encode a slice of f32 values as little-endian bytes → base64.
    fn encode_f32_array(vals: &[f32]) -> String {
        let bytes: Vec<u8> = vals
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        B64.encode(&bytes)
    }

    /// Encode a slice of f64 values as little-endian bytes, zlib-compress, then base64.
    fn encode_f64_zlib(vals: &[f64]) -> String {
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write as _;

        let raw: Vec<u8> = vals
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&raw).unwrap();
        B64.encode(enc.finish().unwrap())
    }

    /// Build a minimal but valid mzML document containing a single spectrum
    /// with one m/z array (32-bit, uncompressed) and one intensity array
    /// (64-bit, zlib-compressed).
    fn make_mzml(mz: &[f32], intensity: &[f64]) -> String {
        let mz_b64  = encode_f32_array(mz);
        let int_b64 = encode_f64_zlib(intensity);

        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<mzML xmlns="http://psi.hupo.org/ms/mzml">
  <referenceableParamGroupList count="0"/>
  <instrumentConfigurationList count="1">
    <instrumentConfiguration id="IC1">
      <cvParam accession="MS:1000031" name="Orbitrap Exploris 480"/>
      <cvParam accession="MS:1000026" name="Orbitrap"/>
    </instrumentConfiguration>
  </instrumentConfigurationList>
  <run id="run1" startTimeStamp="2026-09-25T10:00:00Z">
    <spectrumList count="1" defaultDataProcessingRef="dp1">
      <spectrum index="0" id="scan=1" defaultArrayLength="{n}">
        <cvParam accession="MS:1000511" name="ms level" value="1"/>
        <binaryDataArrayList count="2">
          <binaryDataArray encodedLength="{mz_len}">
            <cvParam accession="MS:1000514" name="m/z array"/>
            <cvParam accession="MS:1000521" name="32-bit float"/>
            <cvParam accession="MS:1000576" name="no compression"/>
            <binary>{mz_b64}</binary>
          </binaryDataArray>
          <binaryDataArray encodedLength="{int_len}">
            <cvParam accession="MS:1000515" name="intensity array"/>
            <cvParam accession="MS:1000523" name="64-bit float"/>
            <cvParam accession="MS:1000574" name="zlib compression"/>
            <binary>{int_b64}</binary>
          </binaryDataArray>
        </binaryDataArrayList>
      </spectrum>
    </spectrumList>
  </run>
</mzML>"#,
            n       = mz.len(),
            mz_len  = mz_b64.len(),
            int_len = int_b64.len(),
            mz_b64  = mz_b64,
            int_b64 = int_b64,
        )
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[test]
    fn test_parse_mzml_basic() {
        let mz_vals  = [100.0_f32, 200.0, 300.0, 400.0, 500.0];
        let int_vals = [1000.0_f64, 5000.0, 9999.0, 3000.0, 750.0];

        let xml = make_mzml(&mz_vals, &int_vals);
        let chrom = parse_mzml(xml.as_bytes()).expect("parse should succeed");

        // Metadata extracted from cvParams / XML attributes.
        assert_eq!(chrom.metadata.instrument, "Orbitrap Exploris 480");
        assert_eq!(chrom.metadata.detector_type, "Orbitrap");
        assert_eq!(chrom.metadata.date, "2026-09-25T10:00:00Z");

        // m/z → x, intensity → y
        assert_eq!(chrom.time_series.len(), 5);

        for (i, (&expected_mz, &expected_int)) in
            mz_vals.iter().zip(int_vals.iter()).enumerate()
        {
            let (x, y) = chrom.time_series[i];
            assert!(
                (x - expected_mz as f64).abs() < 1e-4,
                "m/z[{i}] expected {expected_mz}, got {x}"
            );
            assert!(
                (y - expected_int).abs() < 1e-6,
                "int[{i}] expected {expected_int}, got {y}"
            );
        }

        // Parser does not pre-populate peaks.
        assert!(chrom.peaks.is_empty());
    }

    #[test]
    fn test_parse_mzml_empty_spectrum_is_error() {
        // A run with no spectra / no binaryDataArray should return Err.
        let xml = r#"<?xml version="1.0"?>
<mzML xmlns="http://psi.hupo.org/ms/mzml">
  <run id="empty">
    <spectrumList count="0"/>
  </run>
</mzML>"#;
        assert!(parse_mzml(xml.as_bytes()).is_err());
    }
}
