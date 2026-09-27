import sys
import re

path = 'crates/lab-data-parser-gui/src/main.rs'
with open(path, 'rb') as f:
    content = f.read().decode('utf-8')

# Import render_chromatogram
import_old = r'''    peaks::detect_peaks,
};'''
import_new = r'''    peaks::detect_peaks,
    viz::plot::render_chromatogram,
};'''
if 'viz::plot::render_chromatogram' not in content:
    content = content.replace(import_old, import_new)

# Add png to export_as definition
export_tuple_old = r'''            "xlsx" => ("XLSX", vec!["xlsx"], "chromatogram.xlsx"),
            _      => return,
        };'''
export_tuple_new = r'''            "xlsx" => ("XLSX", vec!["xlsx"], "chromatogram.xlsx"),
            "png"  => ("PNG",  vec!["png"],  "chromatogram.png"),
            _      => return,
        };'''
content = content.replace(export_tuple_old, export_tuple_new)

export_match_old = r'''                "xlsx" => export_xlsx(chrom, &path),
                _      => return,
            };'''
export_match_new = r'''                "xlsx" => export_xlsx(chrom, &path),
                "png"  => {
                    if let Err(e) = render_chromatogram(chrom, &path) {
                        return Err(anyhow::anyhow!("Render failed: {}", e));
                    }
                    Ok(())
                },
                _      => return,
            };'''
content = content.replace(export_match_old, export_match_new)

# Add png button in side panel
buttons_old = r'''                ui.horizontal(|ui| {
                    if ui.button("JSON").clicked() { self.export_as("json"); }
                    if ui.button("CSV").clicked()  { self.export_as("csv"); }
                    if ui.button("XLSX").clicked() { self.export_as("xlsx"); }
                });'''
buttons_new = r'''                ui.horizontal(|ui| {
                    if ui.button("JSON").clicked() { self.export_as("json"); }
                    if ui.button("CSV").clicked()  { self.export_as("csv"); }
                    if ui.button("XLSX").clicked() { self.export_as("xlsx"); }
                    if ui.button("PNG").clicked()  { self.export_as("png"); }
                });'''
content = content.replace(buttons_old, buttons_new)

# Add png in top menu
# We will just insert it after the xlsx button closing brace.
xlsx_btn_end = r'''                        self.export_as("xlsx");
                    }'''
xlsx_btn_end_new = r'''                        self.export_as("xlsx");
                    }
                    if ui.button("📈  PNG...").clicked() {
                        ui.close_menu();
                        self.export_as("png");
                    }'''
content = content.replace(xlsx_btn_end, xlsx_btn_end_new, 1) # Only replace the first match which is in the menu

# Modify main() for icon and window title
main_old = r'''fn main() -> eframe::Result {
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
}'''

main_new = r'''fn main() -> eframe::Result {
    let mut icon_data = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let i = (y * 32 + x) * 4;
            if x == 0 || y == 0 || x == 31 || y == 31 {
                icon_data[i] = 20; icon_data[i+1] = 20; icon_data[i+2] = 80; icon_data[i+3] = 255;
            } else {
                icon_data[i] = 100; icon_data[i+1] = 180; icon_data[i+2] = 255; icon_data[i+3] = 255;
            }
        }
    }
    let icon = egui::IconData {
        rgba: icon_data,
        width: 32,
        height: 32,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("lab-data-parser")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 560.0])
            .with_icon(std::sync::Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native(
        "lab-data-parser",
        options,
        Box::new(|cc| Ok(Box::new(LabParserApp::new(cc)))),
    )
}'''
content = content.replace(main_old, main_new)

with open(path, 'wb') as f:
    f.write(content.encode('utf-8'))

print("Modifications applied successfully.")
