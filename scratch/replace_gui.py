import sys
import re

path = 'crates/lab-data-parser-gui/src/main.rs'
with open(path, 'rb') as f:
    content = f.read()

new_func = b"""fn draw_chromatogram(ui: &mut egui::Ui, chrom: &Chromatogram) {
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
}"""

# Replace from 'fn draw_chromatogram' to the end of the file.
new_content = re.sub(br'fn draw_chromatogram\(ui: &mut egui::Ui, chrom: &Chromatogram\) \{.*\}', new_func, content, flags=re.DOTALL)

with open(path, 'wb') as f:
    f.write(new_content)

print('Replaced draw_chromatogram successfully.')
