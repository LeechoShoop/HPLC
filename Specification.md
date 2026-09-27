# Lab Data Parser - Project Specification

## 1. Overview
`lab-data-parser` is a Rust-based toolset designed to parse, analyze, visualize, and export laboratory chromatography and mass spectrometry data. It supports handling files in both text-based (HPLC CSV/TXT) and XML-based (mzML/mzXML) formats.

## 2. Project Architecture
The project is structured as a Cargo workspace with three primary crates:

### 2.1 `lab-data-parser-core` (Library)
The core engine containing all data models, parsers, algorithms, and visualization logic. It acts as the backbone for the CLI and GUI applications and has no dependency on `eframe` or other UI-specific libraries.
* **`model/`**: Contains the unified `Chromatogram`, `Peak`, and `Metadata` structs.
* **`parsers/`**: Contains format-specific parsers (e.g., `csv_parser`, `json_parser`) and a unified interface.
* **`peaks/`**: Implements the peak detection algorithms (`detect_peaks`), allowing configurable threshold and minimum width.
* **`viz/`**: Houses the `plotters`-based rendering functionality (`render_chromatogram`) used to generate standalone static PNG charts.
* **`export/`**: Implements functionality to export chromatogram data to JSON, CSV, and XLSX formats.

### 2.2 `lab-data-parser-cli` (Binary)
A command-line interface providing fast, scriptable access to the core library's functionality. Suitable for batch processing and automated pipelines.

### 2.3 `lab-data-parser-gui` (Binary)
A cross-platform native graphical user interface built using `egui` and `eframe`. 
* **Capabilities**:
  * Native file picking via `rfd`.
  * Interactive peak tuning (live debounced sliders for threshold and min peak width).
  * Interactive data visualization (zoomable, draggable plots with hover tooltips) using `egui_plot`.
  * Sortable, editable peaks table to allow manual peak labeling and adjustment.
  * Direct exports to JSON, CSV, XLSX, and PNG formats.
  * Runs as a standalone native binary without external dependencies.

## 3. Tech Stack
* **Language**: Rust (Edition 2024)
* **GUI Framework**: `eframe` / `egui` / `egui_plot`
* **Plotting (Static)**: `plotters`
* **Serialization**: `serde`, `serde_json`
* **File Dialogs**: `rfd`
* **Build Profiles**: Release optimized for speed and binary size reduction (`opt-level = 3`, `lto = true`, `strip = true`).
