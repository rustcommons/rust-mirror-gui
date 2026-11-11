#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("rust-mirror GUI")
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "rust-mirror GUI",
        options,
        Box::new(|cc| Ok(Box::new(rust_mirror_gui::app::App::new(&cc.egui_ctx)))),
    )
}
