//! Fit the graphics device request to what the adapter offers.
//!
//! egui-wgpu asks for wgpu's default limits on Vulkan, Metal and Direct3D. Some
//! software drivers offer less: Ubuntu 20.04's lavapipe (no GPU) allows two
//! colour attachments where the default asks for eight, so the device request
//! failed before the window opened. egui needs far less than the defaults, so
//! each requested limit is lowered to what the adapter offers, but never below
//! [`egui_minimum`].
use eframe::egui_wgpu::wgpu::Limits;

/// What egui needs: egui-wgpu's own request on OpenGL (wgpu's WebGL2 limits),
/// with one colour attachment (egui draws to a single target) and a 2048-pixel
/// texture side (egui's font atlas).
pub fn egui_minimum() -> Limits {
    Limits {
        max_color_attachments: 1,
        max_texture_dimension_2d: 2048,
        ..Limits::downlevel_webgl2_defaults()
    }
}

/// A device request that fits the adapter.
#[derive(Debug, PartialEq)]
pub struct Fitted {
    /// What to request: `wanted`, lowered where the adapter offers less.
    pub limits: Limits,
    /// The limits that were lowered: (name, wanted, offered).
    pub lowered: Vec<(&'static str, u64, u64)>,
}

/// Lower each limit in `wanted` to what the adapter `offers`.
///
/// Uses wgpu's own `Limits::or_worse_values_from`. Fails, naming every limit,
/// if the adapter offers less than [`egui_minimum`].
pub fn fit_limits(
    wanted: &Limits,
    offered: &Limits,
) -> Result<Fitted, Vec<(&'static str, u64, u64)>> {
    let mut short = Vec::new();
    egui_minimum().check_limits_with_fail_fn(offered, false, |name, need, has| {
        short.push((name, need, has))
    });
    if !short.is_empty() {
        return Err(short);
    }
    let mut lowered = Vec::new();
    wanted.check_limits_with_fail_fn(offered, false, |name, want, has| {
        lowered.push((name, want, has))
    });
    let limits = wanted.clone().or_worse_values_from(offered);
    Ok(Fitted { limits, lowered })
}

/// The message for an adapter that offers less than egui needs.
pub fn shortfall_message(adapter: &str, short: &[(&'static str, u64, u64)]) -> String {
    let list: Vec<String> = short
        .iter()
        .map(|(name, need, has)| format!("{name} (needs {need}, offers {has})"))
        .collect();
    format!(
        "the graphics adapter \"{adapter}\" offers less than the window needs: {}. \
         Update the graphics driver, or try another backend with WGPU_BACKEND=gl or WGPU_BACKEND=vulkan.",
        list.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The request egui-wgpu makes on Vulkan, Metal and Direct3D.
    fn egui_request() -> Limits {
        Limits {
            max_texture_dimension_2d: 8192,
            ..Limits::default()
        }
    }

    #[test]
    fn lavapipe_on_ubuntu_20_04_fits() {
        // The failure seen on Ubuntu 20.04 without a GPU.
        let offered = Limits {
            max_color_attachments: 2,
            ..Limits::default()
        };
        let fitted = fit_limits(&egui_request(), &offered).unwrap();
        assert_eq!(fitted.lowered, vec![("max_color_attachments", 8, 2)]);
        assert_eq!(fitted.limits.max_color_attachments, 2);
        assert_eq!(fitted.limits.max_texture_dimension_2d, 8192);
        assert!(fitted.limits.check_limits(&offered));
    }

    #[test]
    fn a_capable_adapter_gets_the_request_unchanged() {
        let offered = Limits {
            max_texture_dimension_2d: 16384,
            max_color_attachments: 8,
            ..Limits::default()
        };
        let fitted = fit_limits(&egui_request(), &offered).unwrap();
        assert_eq!(
            fitted,
            Fitted {
                limits: egui_request(),
                lowered: vec![]
            }
        );
    }

    #[test]
    fn the_webgl2_baseline_is_enough() {
        let offered = egui_minimum();
        let fitted = fit_limits(&egui_request(), &offered).unwrap();
        assert!(fitted.limits.check_limits(&offered));
        assert!(egui_minimum().check_limits(&fitted.limits));
    }

    #[test]
    fn below_what_egui_needs_is_an_error_naming_each_limit() {
        let offered = Limits {
            max_color_attachments: 0,
            max_texture_dimension_2d: 1024,
            ..Limits::default()
        };
        let short = fit_limits(&egui_request(), &offered).unwrap_err();
        assert_eq!(
            short,
            vec![
                ("max_texture_dimension_2d", 2048, 1024),
                ("max_color_attachments", 1, 0)
            ]
        );
        let message = shortfall_message("llvmpipe", &short);
        assert!(
            message.contains("max_texture_dimension_2d (needs 2048, offers 1024)"),
            "{message}"
        );
        assert!(
            message.contains("max_color_attachments (needs 1, offers 0)"),
            "{message}"
        );
    }
}
