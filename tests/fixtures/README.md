# Test Fixtures

Synthetic and real (anonymised) instrument export files used by the integration
test suite in `tests/integration_test.rs`.

---

## Synthetic fixtures (checked in)

| File | Format | Description |
|------|--------|-------------|
| `sample.csv` | HPLC CSV | Minimal single-peak file: 11 points, apex at t = 0.5 min, height = 100 |
| `hplc_two_peaks.csv` | HPLC CSV | Richer Agilent-style file with **two peaks** at ~1.0 min and ~3.0 min |
| `hplc_eu_locale.csv` | HPLC CSV | Waters-style file using `;` as field delimiter and `,` as decimal separator (EU locale) |
| `sample.mzml` | mzML | Minimal single-spectrum file with 3 data points, no detectable peak |
| `ms_multi_spectrum.mzml` | mzML | Three-spectrum file; the middle scan carries a dominant ion at m/z 200 (intensity 9000) that should be detected as a peak |

### Reproducing the binary payloads in `*.mzml`

All arrays use little-endian IEEE-754 encoding without compression:

```python
import struct, base64

def encode_f32(vals):
    b = b"".join(struct.pack("<f", v) for v in vals)
    return base64.b64encode(b).decode()

def encode_f64(vals):
    b = b"".join(struct.pack("<d", v) for v in vals)
    return base64.b64encode(b).decode()

mz = [100.0, 150.0, 200.0, 250.0, 300.0]
print(encode_f32(mz))          # m/z arrays (32-bit)

baseline = [10.0, 8.0, 12.0, 9.0, 11.0]
print(encode_f64(baseline))    # baseline intensity (64-bit)

peak_int = [100.0, 500.0, 9000.0, 500.0, 100.0]
print(encode_f64(peak_int))    # peak-scan intensity (64-bit)
```

---

## Real instrument files (add here when available)

Place an anonymised export from your instrument in this directory and add the
filename to the table below.  The integration test `test_real_instrument_file`
in `tests/integration_test.rs` will automatically pick it up if the file is
present.

| File | Instrument | Format | Notes |
|------|------------|--------|-------|
| *(none yet)* | — | — | — |

When you have a file to add:
1. Copy / rename it to a descriptive name, e.g. `real_hplc_run1.csv`.
2. Add a row to the table above.
3. Update the `REAL_INSTRUMENT_FILE` constant at the top of `tests/integration_test.rs`.
