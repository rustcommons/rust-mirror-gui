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
        // The bundled fonts cover every symbol the app shows; never look up
        // system fonts (on Linux that needed fontconfig and an installed font).
        system_font_fallback: false,
        ..Default::default()
    };
    prefer_direct3d12_on_windows(&mut options);
    fit_limits_to_adapter(&mut options);
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

/// Never ask the graphics driver for more than it offers (see `gpu`).
fn fit_limits_to_adapter(options: &mut eframe::NativeOptions) {
    use eframe::egui_wgpu::WgpuSetup;
    use rust_mirror_gui::gpu;
    use std::sync::Arc;
    let mut setup = match &options.wgpu_options.wgpu_setup {
        WgpuSetup::CreateNew(setup) => setup.clone(),
        WgpuSetup::Existing(_) => return,
    };
    let requested = setup.device_descriptor.clone();
    setup.device_descriptor = Arc::new(move |adapter| {
        let mut descriptor = requested(adapter);
        let info = adapter.get_info();
        match gpu::fit_limits(&descriptor.required_limits, &adapter.limits()) {
            Ok(fitted) => {
                if !fitted.lowered.is_empty() {
                    let names: Vec<_> = fitted
                        .lowered
                        .iter()
                        .map(|(name, want, has)| format!("{name} {want}->{has}"))
                        .collect();
                    eprintln!("rust-mirror-gui: {} ({:?}) offers less than the default request; lowered {}", info.name, info.backend, names.join(", "));
                }
                descriptor.required_limits = fitted.limits;
            }
            Err(short) => {
                eprintln!(
                    "rust-mirror-gui: {}",
                    gpu::shortfall_message(&info.name, &short)
                );
                // Still ask for what egui needs, so the device request fails
                // here instead of the window drawing wrongly.
                descriptor.required_limits = descriptor
                    .required_limits
                    .clone()
                    .or_worse_values_from(&adapter.limits())
                    .or_better_values_from(&gpu::egui_minimum());
            }
        }
        descriptor
    });
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
