mod export;
mod model;
mod parsers;
mod peaks;
mod viz;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use parsers::parser_for_format;
use viz::render_chromatogram;
use export::{export_csv, export_json, export_xlsx};

// ---------------------------------------------------------------------------
// Top-level CLI
// ---------------------------------------------------------------------------

/// lab-data-parser — parse and visualise laboratory instrument data files.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Parse a laboratory data file and write a unified Chromatogram JSON.
    Parse(ParseArgs),

    /// Load a saved Chromatogram JSON and render it as a PNG chart.
    Plot(PlotArgs),

    /// Export a Chromatogram JSON to CSV, XLSX, or JSON.
    Export(ExportArgs),
}

// ---------------------------------------------------------------------------
// `parse` subcommand
// ---------------------------------------------------------------------------

/// Arguments for `lab-data-parser parse`.
#[derive(Parser, Debug)]
struct ParseArgs {
    /// Input file (single-file mode) **or** input directory (--batch mode).
    #[arg(short, long)]
    input: PathBuf,

    /// Input format: "hplc" (Agilent/Waters CSV) or "ms" / "mzml" (mzML).
    #[arg(short, long, default_value = "hplc")]
    format: String,

    /// Output JSON file (single-file mode only; ignored in --batch mode).
    ///
    /// In batch mode each input file `<dir>/<stem>.<ext>` produces a sibling
    /// file `<dir>/<stem>.json`.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Process every matching file inside `--input` (which must be a directory).
    ///
    /// The recognised extensions per format are:
    /// * hplc  → .csv  .txt
    /// * ms    → .mzml .mzXML .xml
    #[arg(long, default_value_t = false)]
    batch: bool,
}

// ---------------------------------------------------------------------------
// `plot` subcommand
// ---------------------------------------------------------------------------

/// Arguments for `lab-data-parser plot`.
#[derive(Parser, Debug)]
struct PlotArgs {
    /// Path to a Chromatogram JSON file previously created by `parse`.
    #[arg(short, long)]
    input: PathBuf,

    /// Output PNG file.
    #[arg(short, long)]
    output: PathBuf,
}

// ---------------------------------------------------------------------------
// `export` subcommand
// ---------------------------------------------------------------------------

/// Arguments for `lab-data-parser export`.
#[derive(Parser, Debug)]
struct ExportArgs {
    /// Path to a Chromatogram JSON file (produced by `parse`).
    #[arg(short, long)]
    input: PathBuf,

    /// Output file or base path.
    ///
    /// For CSV this is used as a stem: `<output>_signal.csv` and
    /// `<output>_peaks.csv` are written next to it.
    /// For JSON and XLSX this is used as-is.
    /// For `all`, the stem is reused with appropriate extensions.
    #[arg(short, long)]
    output: PathBuf,

    /// Export format: json | csv | xlsx | all
    ///
    /// `all` writes all three formats in one call.
    #[arg(short, long, default_value = "csv")]
    format: String,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Parse(args)  => run_parse(args),
        Command::Plot(args)   => run_plot(args),
        Command::Export(args) => run_export(args),
    }
}

// ---------------------------------------------------------------------------
// `parse` handler
// ---------------------------------------------------------------------------

fn run_parse(args: ParseArgs) -> Result<()> {
    if args.batch {
        run_parse_batch(&args)
    } else {
        run_parse_single(&args)
    }
}

/// Single-file mode: parse one file and write one JSON.
fn run_parse_single(args: &ParseArgs) -> Result<()> {
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| args.input.with_extension("json"));

    let chrom = parser_for_format(&args.format)?
        .parse(&args.input)
        .with_context(|| format!("failed to parse {}", args.input.display()))?;

    let json = serde_json::to_string_pretty(&chrom)
        .context("failed to serialise Chromatogram to JSON")?;
    std::fs::write(&output, json)
        .with_context(|| format!("failed to write {}", output.display()))?;

    println!(
        "[parse] {} → {} ({} points, {} peaks)",
        args.input.display(),
        output.display(),
        chrom.time_series.len(),
        chrom.peaks.len(),
    );
    Ok(())
}

/// Batch mode: walk `args.input` (a directory) and process every file whose
/// extension matches the selected format.
fn run_parse_batch(args: &ParseArgs) -> Result<()> {
    let dir = &args.input;
    anyhow::ensure!(
        dir.is_dir(),
        "--batch requires --input to be a directory, got: {}",
        dir.display()
    );

    let extensions = format_extensions(&args.format);
    let parser = parser_for_format(&args.format)?;

    let mut processed = 0_u32;
    let mut errors    = 0_u32;

    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("cannot read directory {}", dir.display()))?
    {
        let entry = entry.context("directory entry error")?;
        let path  = entry.path();

        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if !extensions.contains(&ext.as_str()) {
            continue;
        }

        let output = path.with_extension("json");

        match parser.parse(&path) {
            Ok(chrom) => {
                match serde_json::to_string_pretty(&chrom) {
                    Ok(json) => match std::fs::write(&output, json) {
                        Ok(()) => {
                            println!(
                                "[batch] ✓ {} → {} ({} pts)",
                                path.display(),
                                output.display(),
                                chrom.time_series.len(),
                            );
                            processed += 1;
                        }
                        Err(e) => {
                            eprintln!("[batch] ✗ write {}: {e}", output.display());
                            errors += 1;
                        }
                    },
                    Err(e) => {
                        eprintln!("[batch] ✗ serialise {}: {e}", path.display());
                        errors += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("[batch] ✗ parse {}: {e:#}", path.display());
                errors += 1;
            }
        }
    }

    println!("\n[batch] done — {processed} succeeded, {errors} failed");

    if errors > 0 && processed == 0 {
        anyhow::bail!("all {errors} file(s) failed to parse");
    }
    Ok(())
}

/// File extensions matched for each `--format` value.
fn format_extensions(format: &str) -> &'static [&'static str] {
    match format.to_ascii_lowercase().as_str() {
        "ms" | "mzml" => &["mzml", "mzxml", "xml"],
        _             => &["csv", "txt"],          // hplc default
    }
}

// ---------------------------------------------------------------------------
// `plot` handler
// ---------------------------------------------------------------------------

fn run_plot(args: PlotArgs) -> Result<()> {
    let json = std::fs::read_to_string(&args.input)
        .with_context(|| format!("cannot read {}", args.input.display()))?;

    let chrom: model::Chromatogram = serde_json::from_str(&json)
        .with_context(|| format!("invalid Chromatogram JSON in {}", args.input.display()))?;

    render_chromatogram(&chrom, &args.output)
        .with_context(|| format!("rendering failed for {}", args.output.display()))?;

    println!(
        "[plot] {} → {} ({} points, {} peaks annotated)",
        args.input.display(),
        args.output.display(),
        chrom.time_series.len(),
        chrom.peaks.len(),
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// `export` handler
// ---------------------------------------------------------------------------

fn run_export(args: ExportArgs) -> Result<()> {
    // Load the saved Chromatogram JSON.
    let json = std::fs::read_to_string(&args.input)
        .with_context(|| format!("cannot read {}", args.input.display()))?;
    let chrom: model::Chromatogram = serde_json::from_str(&json)
        .with_context(|| format!("invalid Chromatogram JSON: {}", args.input.display()))?;

    let fmt = args.format.to_ascii_lowercase();

    // Derive a stem from --output for the "all" and "csv" cases.
    let stem = args.output.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("chromatogram");
    let dir = args.output.parent().unwrap_or(std::path::Path::new("."));

    match fmt.as_str() {
        "json" => {
            export_json(&chrom, &args.output)
                .with_context(|| format!("JSON export failed: {}", args.output.display()))?;
            println!("[export] json → {}", args.output.display());
        }
        "csv" => {
            export_csv(&chrom, &args.output)
                .with_context(|| format!("CSV export failed: {}", args.output.display()))?;
            println!(
                "[export] csv  → {}{{_signal,_peaks}}.csv",
                args.output.with_extension("").display()
            );
        }
        "xlsx" => {
            export_xlsx(&chrom, &args.output)
                .with_context(|| format!("XLSX export failed: {}", args.output.display()))?;
            println!("[export] xlsx → {}", args.output.display());
        }
        "all" => {
            let json_path = dir.join(format!("{stem}.json"));
            let csv_base  = dir.join(format!("{stem}.csv"));
            let xlsx_path = dir.join(format!("{stem}.xlsx"));

            export_json(&chrom, &json_path)
                .context("JSON export failed")?;
            println!("[export] json → {}", json_path.display());

            export_csv(&chrom, &csv_base)
                .context("CSV export failed")?;
            println!(
                "[export] csv  → {}{{_signal,_peaks}}.csv",
                csv_base.with_extension("").display()
            );

            export_xlsx(&chrom, &xlsx_path)
                .context("XLSX export failed")?;
            println!("[export] xlsx → {}", xlsx_path.display());
        }
        other => anyhow::bail!(
            "unknown export format {:?}; valid values are: json, csv, xlsx, all",
            other
        ),
    }

    println!(
        "       source: {} ({} points, {} peaks)",
        args.input.display(),
        chrom.time_series.len(),
        chrom.peaks.len(),
    );
    Ok(())
}
