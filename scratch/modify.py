import sys
import re

path = 'crates/lab-data-parser-gui/src/main.rs'
with open(path, 'rb') as f:
    content = f.read().decode('utf-8')

# 1. Add fields to LabParserApp
struct_replacement = r'''    status_is_error: bool,
    needs_recalc: bool,
    last_recalc_trigger: f64,
    sort_column: usize,
    sort_descending: bool,
}'''
content = re.sub(r'    status_is_error: bool,\n\}', struct_replacement, content)

new_replacement = r'''    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            format: "hplc".into(),
            threshold: 10.0,
            min_width: 0.05,
            status: "Open a file to begin.".into(),
            sort_column: 1, // default to RT
            ..Default::default()
        }
    }'''
content = re.sub(r'    fn new\(_cc: &eframe::CreationContext<''\_>\) -> Self \{[\s\S]*?\}', new_replacement, content)

# 2. Add debounce logic to top of update()
update_start = r'''    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.needs_recalc && ctx.input(|i| i.time) - self.last_recalc_trigger > 0.3 {
            self.rerun_peak_detection();
            self.needs_recalc = false;
        } else if self.needs_recalc {
            ctx.request_repaint();
        }

        // -- Top menu bar ------------------------------------------------------'''
content = re.sub(r'    fn update\(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame\) \{\n        // -- Top menu bar ------------------------------------------------------', update_start, content)

# 3. Update sliders to set needs_recalc
slider_thresh_old = r'''                ui.label\("Peak threshold:"\);\n                ui\.add\(\n                    egui::Slider::new\(&mut self\.threshold, 0\.1\.\.=10_000\.0\)\n                        \.logarithmic\(true\)\n                        \.suffix\(" AU"\),\n                \);'''
slider_thresh_new = r'''                ui.label("Peak threshold:");
                if ui.add(
                    egui::Slider::new(&mut self.threshold, 0.1..=10_000.0)
                        .logarithmic(true)
                        .suffix(" AU"),
                ).changed() {
                    self.needs_recalc = true;
                    self.last_recalc_trigger = ui.input(|i| i.time);
                }'''
content = re.sub(slider_thresh_old, slider_thresh_new, content)

slider_width_old = r'''                ui.label\("Min peak width:"\);\n                ui\.add\(\n                    egui::Slider::new\(&mut self\.min_width, 0\.001\.\.=2\.0\)\n                        \.suffix\(" min"\),\n                \);'''
slider_width_new = r'''                ui.label("Min peak width:");
                if ui.add(
                    egui::Slider::new(&mut self.min_width, 0.001..=2.0)
                        .suffix(" min"),
                ).changed() {
                    self.needs_recalc = true;
                    self.last_recalc_trigger = ui.input(|i| i.time);
                }'''
content = re.sub(slider_width_old, slider_width_new, content)

# 4. Update the peaks table to be sortable and editable
# Note that we use a non-regex string replace for the table to avoid escaping madness.
table_old = r'''            egui::ScrollArea::vertical()
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
                                ui.label(peak.name.as_deref().unwrap_or("-"));
                                ui.label(format!("{:.4}", peak.retention_time));
                                ui.label(format!("{:.1}", peak.height));
                                ui.label(format!("{:.1}", peak.area));
                                ui.label(format!("{:.4}", peak.width));
                                ui.end_row();
                            }
                        });
                });'''

table_new = r'''            // Sort peaks if needed
            let mut peaks = chrom.peaks.clone();
            let sort_col = self.sort_column;
            let sort_desc = self.sort_descending;
            peaks.sort_by(|a, b| {
                let cmp = match sort_col {
                    0 => a.name.as_deref().unwrap_or("").cmp(b.name.as_deref().unwrap_or("")),
                    1 => a.retention_time.partial_cmp(&b.retention_time).unwrap_or(std::cmp::Ordering::Equal),
                    2 => a.height.partial_cmp(&b.height).unwrap_or(std::cmp::Ordering::Equal),
                    3 => a.area.partial_cmp(&b.area).unwrap_or(std::cmp::Ordering::Equal),
                    4 => a.width.partial_cmp(&b.width).unwrap_or(std::cmp::Ordering::Equal),
                    _ => std::cmp::Ordering::Equal,
                };
                if sort_desc { cmp.reverse() } else { cmp }
            });

            egui::ScrollArea::vertical()
                .id_salt("peaks_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("peaks_table")
                        .num_columns(5)
                        .striped(true)
                        .min_col_width(100.0)
                        .show(ui, |ui| {
                            // Header
                            let headers = ["Name", "RT (min)", "Height", "Area", "Width (min)"];
                            for (i, &h) in headers.iter().enumerate() {
                                let mut text = RichText::new(h).strong().underline();
                                if self.sort_column == i {
                                    text = RichText::new(format!("{} {}", h, if self.sort_descending { "▼" } else { "▲" })).strong().underline();
                                }
                                if ui.selectable_label(self.sort_column == i, text).clicked() {
                                    if self.sort_column == i {
                                        self.sort_descending = !self.sort_descending;
                                    } else {
                                        self.sort_column = i;
                                        self.sort_descending = false;
                                    }
                                }
                            }
                            ui.end_row();

                            let mut modified = false;
                            for mut peak in peaks {
                                let mut peak_name = peak.name.clone().unwrap_or_default();
                                if ui.text_edit_singleline(&mut peak_name).changed() {
                                    peak.name = if peak_name.is_empty() { None } else { Some(peak_name) };
                                    modified = true;
                                }
                                ui.label(format!("{:.4}", peak.retention_time));
                                ui.label(format!("{:.1}", peak.height));
                                ui.label(format!("{:.1}", peak.area));
                                ui.label(format!("{:.4}", peak.width));
                                ui.end_row();

                                if modified {
                                    // Update the real chromatogram peaks
                                    if let Some(ref mut c) = self.chromatogram {
                                        if let Some(p) = c.peaks.iter_mut().find(|p| (p.retention_time - peak.retention_time).abs() < 1e-6) {
                                            p.name = peak.name.clone();
                                        }
                                    }
                                    modified = false;
                                }
                            }
                        });
                });'''
# the previous implementation used "-" for empty names, wait, it used "?". No, I fixed encoding so it's probably "-".
# Let's replace using simple string replacement for the table part.
if table_old in content:
    content = content.replace(table_old, table_new)
else:
    # Try regex if exact match fails
    content = re.sub(r'            egui::ScrollArea::vertical\(\)[\s\S]*?\}\);\n                \}\);', table_new, content)

with open(path, 'wb') as f:
    f.write(content.encode('utf-8'))

print('Replaced stuff!')
