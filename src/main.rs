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
    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("rust-mirror GUI")
            .with_inner_size(rust_mirror_gui::DEFAULT_INNER_SIZE)
            .with_min_inner_size(rust_mirror_gui::MIN_INNER_SIZE),
        ..Default::default()
    };
    prefer_direct3d12_on_windows(&mut options);
    present_mode_override(&mut options);
    eframe::run_native(
        "rust-mirror GUI",
        options,
        Box::new(|cc| Ok(Box::new(rust_mirror_gui::app::App::new(&cc.egui_ctx)))),
    )
}

/// On Windows, render with Direct3D 12 unless `WGPU_BACKEND` is set.
///
/// wgpu's default choice can pick Vulkan, and some Windows Vulkan drivers (seen with
/// an AMD Radeon RX 580) present only a blank window. Direct3D 12 is available on
/// every supported Windows version, including a software fallback without a GPU.
/// Set `WGPU_BACKEND=vulkan` (or `gl`) to override.
#[cfg(windows)]
fn prefer_direct3d12_on_windows(options: &mut eframe::NativeOptions) {
    use eframe::egui_wgpu::{wgpu, WgpuSetup, WgpuSetupCreateNew};
    if std::env::var_os("WGPU_BACKEND").is_some() {
        return;
    }
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::DX12;
    options.wgpu_options.wgpu_setup = WgpuSetup::CreateNew(setup);
}

/// Diagnostics: `RUST_MIRROR_GUI_PRESENT_MODE` (`fifo`, `mailbox`,
/// `immediate`, `auto_vsync` or `auto_no_vsync`) picks the swap-chain present
/// mode, to compare rendering cost on machines that draw in software.
fn present_mode_override(options: &mut eframe::NativeOptions) {
    use eframe::egui_wgpu::wgpu::PresentMode;
    let mode = match std::env::var("RUST_MIRROR_GUI_PRESENT_MODE").as_deref() {
        Ok("fifo") => PresentMode::Fifo,
        Ok("mailbox") => PresentMode::Mailbox,
        Ok("immediate") => PresentMode::Immediate,
        Ok("auto_vsync") => PresentMode::AutoVsync,
        Ok("auto_no_vsync") => PresentMode::AutoNoVsync,
        _ => return,
    };
    options.wgpu_options.surface.present_mode = mode;
}

#[cfg(not(windows))]
fn prefer_direct3d12_on_windows(_options: &mut eframe::NativeOptions) {}
