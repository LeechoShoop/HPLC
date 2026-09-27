//! JSON parser using `serde_json`.

use anyhow::Result;
use crate::model::DataSet;

/// Parse a JSON file into a [`DataSet`].
pub fn parse_json(_path: &str) -> Result<DataSet> {
    // TODO: implement JSON parsing
    Ok(DataSet {
        name: "unnamed".to_string(),
        measurements: Vec::new(),
    })
}
