#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|a| a == "--worker") {
        return rust_mirror_gui::worker::main(args.collect());
    }
    match gui() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rust-mirror-gui: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn gui() -> eframe::Result {
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
