//! Public library API for the `lab-data-parser` crate.
//!
//! This crate root exists so that integration tests (in `tests/`) and
//! downstream consumers can `use lab_data_parser::…` without depending on
//! the binary entry point in `main.rs`.

pub mod export;
pub mod model;
pub mod parsers;
pub mod peaks;
pub mod viz;
