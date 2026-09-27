//! CSV parser using the `csv` crate.

use anyhow::Result;
use crate::model::{DataSet, Measurement};

/// Parse a CSV file into a [`DataSet`].
pub fn parse_csv(_path: &str) -> Result<DataSet> {
    // TODO: implement CSV parsing
    Ok(DataSet {
        name: "unnamed".to_string(),
        measurements: Vec::new(),
    })
}
