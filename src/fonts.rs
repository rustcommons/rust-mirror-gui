//! The app's fonts: egui's built-in fonts plus DejaVu Sans, bundled.
//!
//! egui's built-in fonts lack some symbols the app shows (✔, ✖, ⚠, ●, →).
//! Without a bundled font, eframe looked them up among the system fonts, which
//! on Linux needs fontconfig and an installed font; minimal systems showed
//! empty boxes. DejaVu Sans 2.37 (free license, `assets/fonts/LICENSE-DejaVu.txt`)
//! is added as the last fallback of both families, so egui's own fonts still
//! draw everything they have, and no system font is consulted.
use eframe::egui::{
    self,
    epaint::text::{FontInsert, FontPriority, InsertFontFamily},
    FontData, FontFamily,
};

/// DejaVu Sans 2.37, from the upstream release `dejavu-fonts-ttf-2.37.tar.bz2`.
pub const DEJAVU_SANS: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");

/// Add the bundled font to `ctx`, after egui's own fonts.
pub fn install(ctx: &egui::Context) {
    let last = |family| InsertFontFamily {
        family,
        priority: FontPriority::Lowest,
    };
    ctx.add_font(FontInsert::new(
        "DejaVuSans",
        FontData::from_static(DEJAVU_SANS),
        vec![last(FontFamily::Proportional), last(FontFamily::Monospace)],
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every non-ASCII character in the app's string literals.
    fn ui_symbols() -> String {
        let mut found = std::collections::BTreeSet::new();
        let mut stack = vec![std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src"
        ))];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    for line in std::fs::read_to_string(&path).unwrap().lines() {
                        let code = line.trim_start();
                        if code.starts_with("//") {
                            continue; // comments are not shown
                        }
                        // Text between quotes on this line.
                        for (i, part) in code.split('"').enumerate() {
                            if i % 2 == 1 {
                                found.extend(part.chars().filter(|c| !c.is_ascii()));
                            }
                        }
                    }
                }
            }
        }
        found.into_iter().collect()
    }

    #[test]
    fn every_symbol_the_ui_uses_has_a_glyph() {
        let symbols = ui_symbols();
        assert!(
            symbols.contains('✔') && symbols.contains('⚠'),
            "scan found {symbols:?}"
        );
        let ctx = egui::Context::default();
        install(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            let missing: String = ctx.fonts_mut(|f| {
                symbols
                    .chars()
                    .filter(|&c| !f.has_glyph(&egui::FontId::new(14.0, family.clone()), c))
                    .collect()
            });
            assert!(missing.is_empty(), "{family:?} lacks {missing:?}");
        }
    }

    #[test]
    fn without_the_bundled_font_the_check_mark_is_missing() {
        // Guards the test above: egui's own fonts really lack ✔.
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        assert!(!ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::proportional(14.0), '✔')));
    }
}
