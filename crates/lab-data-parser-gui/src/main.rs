//! GUI binary for lab-data-parser.
//!
//! Built with egui/eframe (immediate-mode, pure Rust, no extra runtime
//! dependencies).  All parsing, peak detection, and export logic is
//! provided by `lab-data-parser-core`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console on Windows release

use std::path::PathBuf;

use eframe::egui::{self, Align, Color32, Layout, RichText, Stroke, Vec2};
use lab_data_parser_core::{
    export::{export_csv, export_json, export_xlsx},
    model::Chromatogram,
    parsers::parser_for_format,
    peaks::detect_peaks,
};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Lab Data Parser")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Lab Data Parser",
        options,
        Box::new(|cc| Ok(Box::new(LabParserApp::new(cc)))),
    )
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct LabParserApp {
    /// Currently loaded chromatogram (None until a file is opened).
    chromatogram: Option<Chromatogram>,
    /// Path of the file that was loaded.
    loaded_path: Option<PathBuf>,
    /// Format string used to parse (hplc / mzml).
    format: String,
    /// Peak detection threshold (AU above baseline).
    threshold: f64,
    /// Minimum peak width (FWHM, minutes).
    min_width: f64,
    /// Status / error message shown in the status bar.
    status: String,
    /// Whether the status is an error (red) or info (green).
    status_is_error: bool,
}

fn setup_custom_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();

    let bg_color = Color32::from_rgb(13, 17, 23); // #0d1117
    let panel_bg = Color32::from_rgb(22, 27, 34); // #161b22
    let text_primary = Color32::from_rgb(201, 209, 217); // #c9d1d9
    let text_muted = Color32::from_rgb(139, 148, 158); // #8b949e
    let accent = Color32::from_rgb(88, 166, 255); // #58a6ff

    visuals.panel_fill = panel_bg;
    visuals.window_fill = panel_bg;
    visuals.faint_bg_color = bg_color;
    visuals.extreme_bg_color = bg_color;

    visuals.override_text_color = Some(text_primary);

    visuals.widgets.noninteractive.bg_fill = bg_color;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text_muted);
    
    visuals.widgets.inactive.bg_fill = panel_bg;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text_primary);

    visuals.widgets.hovered.bg_fill = bg_color;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, accent);
    
    visuals.widgets.active.bg_fill = accent;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);

    visuals.selection.bg_fill = accent;
    visuals.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);

    let rounding = egui::CornerRadius::same(6);
    visuals.widgets.noninteractive.corner_radius = rounding;
    visuals.widgets.inactive.corner_radius = rounding;
    visuals.widgets.hovered.corner_radius = rounding;
    visuals.widgets.active.corner_radius = rounding;
    visuals.widgets.open.corner_radius = rounding;
    visuals.window_corner_radius = rounding;
    visuals.menu_corner_radius = rounding;

    style.visuals = visuals;

    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(8.0, 6.0);

    for (_, font_id) in style.text_styles.iter_mut() {
        font_id.family = egui::FontFamily::Monospace;
    }

    ctx.set_style(style);
}

impl LabParserApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_style(&cc.egui_ctx);
        Self {
            format: "hplc".into(),
            threshold: 10.0,
            min_width: 0.05,
            status: "Open a file to begin.".into(),
            ..Default::default()
        }
    }

    /// Try to open a file picker, parse the selected file, and detect peaks.
    fn open_file(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("Open laboratory data file")
            .add_filter("HPLC CSV", &["csv", "txt"])
            .add_filter("mzML / mzXML", &["mzml", "mzxml", "xml"])
            .add_filter("All files", &["*"]);

        if let Some(ref p) = self.loaded_path {
            if let Some(dir) = p.parent() {
                dialog = dialog.set_directory(dir);
            }
        }

        if let Some(path) = dialog.pick_file() {
            self.load_file(path);
        }
    }

    fn load_file(&mut self, path: PathBuf) {
        // Auto-detect format from extension if not set manually.
        let fmt = {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match ext.as_str() {
                "mzml" | "mzxml" | "xml" => "mzml",
                _ => "hplc",
            }
        };
        self.format = fmt.into();

        match parser_for_format(&self.format) {
            Err(e) => self.set_error(format!("Unknown format '{}': {e}", self.format)),
            Ok(parser) => match parser.parse(&path) {
                Err(e) => self.set_error(format!("Parse error: {e:#}")),
                Ok(mut chrom) => {
                    // Run peak detection immediately.
                    chrom.peaks = detect_peaks(&chrom.time_series, self.threshold, self.min_width);
                    let n_pts  = chrom.time_series.len();
                    let n_pk   = chrom.peaks.len();
                    self.chromatogram = Some(chrom);
                    self.loaded_path  = Some(path.clone());
                    self.set_ok(format!(
                        "Loaded {} — {n_pts} points, {n_pk} peaks detected.",
                        path.display()
                    ));
                }
            },
        }
    }

    fn rerun_peak_detection(&mut self) {
        if let Some(ref mut chrom) = self.chromatogram {
            chrom.peaks = detect_peaks(&chrom.time_series, self.threshold, self.min_width);
            let n = chrom.peaks.len();
            self.set_ok(format!("Peak detection re-run — {n} peaks found."));
        }
    }

    fn export_as(&mut self, kind: &str) {
        let Some(ref chrom) = self.chromatogram else {
            self.set_error("No chromatogram loaded.".into());
            return;
        };
        let (filter_name, exts, default_name) = match kind {
            "json" => ("JSON", vec!["json"], "chromatogram.json"),
            "csv"  => ("CSV",  vec!["csv"],  "chromatogram.csv"),
            "xlsx" => ("XLSX", vec!["xlsx"], "chromatogram.xlsx"),
            _      => return,
        };

        let dialog = rfd::FileDialog::new()
            .set_title(format!("Save {filter_name}"))
            .set_file_name(default_name)
            .add_filter(filter_name, &exts);

        if let Some(path) = dialog.save_file() {
            let result = match kind {
                "json" => export_json(chrom, &path),
                "csv"  => export_csv(chrom, &path),
                "xlsx" => export_xlsx(chrom, &path),
                _      => return,
            };
            match result {
                Ok(()) => self.set_ok(format!("Exported to {}", path.display())),
                Err(e) => self.set_error(format!("Export failed: {e:#}")),
            }
        }
    }

    fn set_ok(&mut self, msg: String) {
        self.status = msg;
        self.status_is_error = false;
    }

    fn set_error(&mut self, msg: String) {
        self.status = msg;
        self.status_is_error = true;
    }
}

// ---------------------------------------------------------------------------
// egui UI
// ---------------------------------------------------------------------------

impl eframe::App for LabParserApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // -- Top menu bar ------------------------------------------------------
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open…").clicked() {
                        ui.close_menu();
                        self.open_file();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Export", |ui| {
                    if ui.button("JSON…").clicked() {
                        ui.close_menu();
                        self.export_as("json");
                    }
                    if ui.button("CSV…").clicked() {
                        ui.close_menu();
                        self.export_as("csv");
                    }
                    if ui.button("XLSX…").clicked() {
                        ui.close_menu();
                        self.export_as("xlsx");
                    }
                });
            });
        });

        // -- Status bar at the bottom ------------------------------------------
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            let color = if self.status_is_error {
                Color32::from_rgb(230, 80, 80)
            } else {
                Color32::from_rgb(100, 200, 120)
            };
            ui.add_space(4.0);
            ui.label(RichText::new(&self.status).color(color).small());
            ui.add_space(4.0);
        });

        // -- Left settings panel -----------------------------------------------
        egui::SidePanel::left("settings_panel")
            .resizable(false)
            .exact_width(240.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.heading("Settings");
                ui.separator();

                ui.add_space(6.0);
                ui.label("Format:");
                egui::ComboBox::from_id_salt("format_selector")
                    .selected_text(self.format.as_str())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.format, "hplc".into(), "hplc (CSV/TXT)");
                        ui.selectable_value(&mut self.format, "mzml".into(), "mzml (mzML/mzXML)");
                    });

                ui.add_space(10.0);
                ui.label("Peak threshold:");
                ui.add(
                    egui::Slider::new(&mut self.threshold, 0.1..=10_000.0)
                        .logarithmic(true)
                        .suffix(" AU"),
                );

                ui.add_space(6.0);
                ui.label("Min peak width:");
                ui.add(
                    egui::Slider::new(&mut self.min_width, 0.001..=2.0)
                        .suffix(" min"),
                );

                ui.add_space(10.0);
                if ui
                    .add_sized([220.0, 32.0], egui::Button::new("Re-detect peaks"))
                    .clicked()
                {
                    self.rerun_peak_detection();
                }

                ui.add_space(8.0);
                ui.separator();

                if ui
                    .add_sized([220.0, 32.0], egui::Button::new("Open file…"))
                    .clicked()
                {
                    self.open_file();
                }

                ui.add_space(4.0);
                ui.label("Export as:");
                ui.horizontal(|ui| {
                    if ui.button("JSON").clicked() { self.export_as("json"); }
                    if ui.button("CSV").clicked()  { self.export_as("csv"); }
                    if ui.button("XLSX").clicked() { self.export_as("xlsx"); }
                });

                // Metadata block when a file is loaded.
                if let Some(ref chrom) = self.chromatogram {
                    ui.add_space(12.0);
                    ui.separator();
                    ui.heading("Metadata");
                    ui.add_space(4.0);
                    let m = &chrom.metadata;
                    egui::Grid::new("meta_grid")
                        .num_columns(2)
                        .spacing([6.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Instrument").strong());
                            ui.label(&m.instrument);
                            ui.end_row();
                            ui.label(RichText::new("Method").strong());
                            ui.label(&m.method);
                            ui.end_row();
                            ui.label(RichText::new("Sample").strong());
                            ui.label(&m.sample_id);
                            ui.end_row();
                            ui.label(RichText::new("Date").strong());
                            ui.label(&m.date);
                            ui.end_row();
                            ui.label(RichText::new("Detector").strong());
                            ui.label(&m.detector_type);
                            ui.end_row();
                        });
                }
            });

        // -- Central panel: chromatogram + peak table ---------------------------
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.chromatogram.is_none() {
                // Empty state
                ui.with_layout(Layout::centered_and_justified(egui::Direction::TopDown), |ui| {
                    ui.heading(RichText::new("No file loaded").weak());
                    ui.label("Use File > Open or the Open button on the left.");
                });
                return;
            }

            let chrom = self.chromatogram.as_ref().unwrap();

            // Split vertically: top = chromatogram plot, bottom = peaks table.
            let available = ui.available_size();
            let plot_height = available.y * 0.6;

            // -- Chromatogram plot --------------------------------------------
            ui.allocate_ui(Vec2::new(available.x, plot_height), |ui| {
                draw_chromatogram(ui, chrom);
            });

            ui.separator();

            // -- Peaks table ---------------------------------------------------
            ui.heading(format!("Detected Peaks ({})", chrom.peaks.len()));
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("peaks_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("peaks_table")
                        .num_columns(5)
                        .striped(true)
                        .min_col_width(100.0)
                        .show(ui, |ui| {
                            // Header
                            for h in ["Name", "RT (min)", "Height", "Area", "Width (min)"] {
                                ui.label(RichText::new(h).strong().underline());
                            }
                            ui.end_row();

                            for peak in &chrom.peaks {
                                ui.label(peak.name.as_deref().unwrap_or("—"));
                                ui.label(format!("{:.4}", peak.retention_time));
                                ui.label(format!("{:.1}", peak.height));
                                ui.label(format!("{:.1}", peak.area));
                                ui.label(format!("{:.4}", peak.width));
                                ui.end_row();
                            }
                        });
                });
        });
    }
}

// ---------------------------------------------------------------------------
// Chromatogram renderer (pure egui Painter)
// ---------------------------------------------------------------------------

fn draw_chromatogram(ui: &mut egui::Ui, chrom: &Chromatogram) {
    if chrom.time_series.is_empty() {
        return;
    }

    let line_points: egui_plot::PlotPoints = chrom.time_series
        .iter()
        .map(|&(t, v)| [t, v])
        .collect();
    let line = egui_plot::Line::new(line_points)
        .color(Color32::from_rgb(100, 180, 255))
        .width(1.5)
        .name("Signal");

    egui_plot::Plot::new("chromatogram_plot")
        .x_axis_label("Retention time (min)")
        .y_axis_label("Signal")
        .allow_drag(true)
        .allow_zoom(true)
        .show(ui, |plot_ui| {
            plot_ui.line(line);

            for peak in &chrom.peaks {
                let pt = [peak.retention_time, peak.height];
                let points = egui_plot::Points::new(vec![pt])
                    .color(Color32::from_rgb(255, 180, 60))
                    .radius(5.0)
                    .shape(egui_plot::MarkerShape::Down)
                    .name(format!("Peak RT: {:.3} min", peak.retention_time));
                plot_ui.points(points);
            }
        });
}

