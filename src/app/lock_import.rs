//! Cargo.lock import (rust-mirror 0.5.0 `crates import-lock` / `remove-lock`,
//! docs/LOCK-IMPORT.md): add one or more lock files as named sources, review
//! the import summary (pins added/removed, path and git entries skipped, other
//! registries refused, line-numbered problems), then write. Each source is
//! shown as a group in the catalog with a remove action. Parsing, validation
//! and the catalog write all happen in the rust-mirror worker; the window
//! keeps only the file paths and the reports.
use super::{banner, date_of, App, AMBER, GREEN, RED};
use crate::ops::{self, Op};
use eframe::egui::{self, RichText};
use serde_json::Value;

/// What the last preview was for, so "write" repeats exactly that request.
#[derive(Clone, PartialEq)]
enum Request {
    Import(Vec<(String, String)>),
    Remove(Vec<String>),
}

#[derive(Default)]
pub(super) struct LockState {
    /// Lock files chosen for import: (source name, path).
    inputs: Vec<(String, String)>,
    new_path: String,
    /// The request and its report (accepted or refused).
    preview: Option<(Request, Value)>,
    /// The written report and, per source, whether the file changed after
    /// the preview.
    written: Option<(Value, Vec<String>)>,
    pending: Option<Request>,
}

#[cfg(test)]
impl LockState {
    /// Layout tests: show `report` as the result of previewing `inputs`.
    pub(super) fn show_test_preview(&mut self, inputs: Vec<(String, String)>, report: Value) {
        self.inputs = inputs.clone();
        self.preview = Some((Request::Import(inputs), report));
    }
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}
fn arr(v: &Value, k: &str) -> Vec<Value> {
    v.get(k)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The library's source-name rule (1–128 ASCII letters, digits, `_`, `-`,
/// `.`, but not `.` or `..`).
pub(super) fn valid_name(name: &str) -> bool {
    rust_mirror::crates::lockfile::validate_source_name(name).is_ok()
}

/// Suggest a source name from the lock file's folder (project-a/Cargo.lock
/// becomes "project-a"), unique among `taken`.
pub(super) fn suggest_name(path: &str, taken: &[String]) -> String {
    let p = std::path::Path::new(path.trim());
    let folder = p
        .parent()
        .and_then(|d| d.file_name())
        .or_else(|| p.file_stem())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut base: String = folder
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '-'
            }
        })
        .take(120)
        .collect();
    if !valid_name(&base) {
        base = "lock".into();
    }
    let mut name = base.clone();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

/// Plain-English hint for a diagnostic kind.
fn hint(kind: &str) -> &'static str {
    match kind {
        "syntax" => "This file is not valid Cargo.lock TOML.",
        "unsupported_version" => "This lock file format is not supported. Open the project with a current Cargo so it records a supported format.",
        "invalid_package" => "A [[package]] entry is incomplete or malformed.",
        "invalid_checksum" | "missing_checksum" | "conflicting_checksum" => "Every crates.io package needs one correct SHA-256 checksum. The file may be damaged or edited by hand; take it again from the project.",
        "duplicate_package" => "The same package and version appear more than once.",
        "alternate_registry" => "Only crates.io packages can be mirrored. Packages from other registries refuse the whole import.",
        "ambiguous_dependency" | "missing_dependency" => "A dependency entry does not match exactly one package in this file.",
        "unknown_field" => "This file has a field rust-mirror does not understand.",
        "budget" => "This import is larger than rust-mirror's limits.",
        _ => "This entry cannot be used.",
    }
}

fn pin_list(pins: &[Value]) -> String {
    pins.iter()
        .map(|p| format!("{} {}", s(p, "name"), s(p, "version")))
        .collect::<Vec<_>>()
        .join(", ")
}

impl App {
    /// Store results of lock operations; returns true when handled.
    pub(super) fn harvest_locks(
        &mut self,
        op: Op,
        report: Option<Value>,
        failure: Option<Value>,
    ) -> bool {
        let l = &mut self.locks;
        match op {
            Op::LockPreview => {
                let req = l.pending.take();
                l.written = None;
                l.preview = req.zip(report.or(failure));
            }
            Op::LockWrite => {
                let req = l.pending.take();
                if let Some(r) = report.filter(|r| r.get("written") == Some(&Value::Bool(true))) {
                    // Compare file digests with the reviewed preview.
                    let before = l
                        .preview
                        .as_ref()
                        .map(|(_, p)| arr(p, "sources"))
                        .unwrap_or_default();
                    let changed = arr(&r, "sources")
                        .iter()
                        .filter(|src| {
                            before.iter().any(|b| {
                                s(b, "source_name") == s(src, "source_name")
                                    && s(b, "sha256") != s(src, "sha256")
                            })
                        })
                        .map(|src| s(src, "source_name"))
                        .collect();
                    l.written = Some((r, changed));
                    l.preview = None;
                    if matches!(req, Some(Request::Import(_))) {
                        l.inputs.clear();
                    }
                    self.crates_reload();
                } else if let (Some(req), Some(f)) = (req, failure) {
                    // Refused at write time: show why, keep the request.
                    l.preview = Some((req, f));
                }
            }
            _ => return false,
        }
        true
    }

    /// "Cargo.lock sources" section of the Crate catalog page.
    pub(super) fn crate_locks(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, program: &str) {
        let cat_path = self.settings.crate_catalog_path.trim().to_string();
        // Lock files dropped on the window are added to the import list.
        let dropped: Vec<String> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_string_lossy().into_owned())
                .filter(|p| !p.is_empty())
                .collect()
        });
        for path in dropped {
            self.add_lock_input(path);
        }
        let sources = self
            .crates_catalog()
            .and_then(|c| c.get("crate-locks"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let header = if sources.is_empty() {
            "Cargo.lock sources".to_string()
        } else {
            format!("Cargo.lock sources ({})", sources.len())
        };
        egui::CollapsingHeader::new(RichText::new(header).strong())
            .default_open(true)
            .show(ui, |ui| {
                ui.label("Import a project's Cargo.lock to mirror exactly the crates.io versions it uses, with their checksums. Each file becomes a named source; path and git entries are skipped and reported. Nothing is downloaded until you preview and approve a mirror.");
                let busy = self.job_running();
                let dirty = self.crates_dirty();
                if dirty {
                    ui.label(RichText::new("Save the catalog first: import and remove work on the saved file.").color(AMBER));
                }
                let usable = !busy && !dirty && !cat_path.is_empty();
                self.lock_groups(ui, ctx, program, &sources, usable, &cat_path);
                ui.add_space(6.0);
                self.lock_inputs(ui, ctx, program, &sources, usable, &cat_path);
                self.lock_report(ui, ctx, program, usable, &cat_path);
            });
    }

    fn add_lock_input(&mut self, path: String) {
        if self.locks.inputs.iter().any(|(_, p)| p == &path) {
            return;
        }
        let taken: Vec<String> = self.locks.inputs.iter().map(|(n, _)| n.clone()).collect();
        let name = suggest_name(&path, &taken);
        self.locks.inputs.push((name, path));
    }

    fn lock_groups(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        program: &str,
        sources: &serde_json::Map<String, Value>,
        usable: bool,
        cat_path: &str,
    ) {
        if sources.is_empty() {
            ui.label(RichText::new("No lock files imported yet.").weak());
            return;
        }
        let mut remove = None;
        egui::Grid::new("lock-sources")
            .striped(true)
            .num_columns(6)
            .spacing([12.0, 3.0])
            .show(ui, |ui| {
                // Table cells keep their width; long cells wrap explicitly.
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                for h in [
                    "Source",
                    "File",
                    "Pinned versions",
                    "Imported",
                    "SHA-256",
                    "",
                ] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (name, src) in sources {
                    ui.label(RichText::new(name).strong());
                    let v = src.get("lock-version").and_then(Value::as_u64);
                    ui.label(format!(
                        "{}{}",
                        s(src, "file-name"),
                        v.map(|v| format!(" (format {v})")).unwrap_or_default()
                    ));
                    ui.label(arr(src, "pins").len().to_string());
                    ui.label(
                        src.get("imported-at")
                            .and_then(Value::as_u64)
                            .map(date_of)
                            .unwrap_or_default(),
                    );
                    let sha = s(src, "sha256");
                    ui.label(RichText::new(sha.get(..12).unwrap_or(&sha)).monospace())
                        .on_hover_text(&sha);
                    if ui
                        .add_enabled(usable, egui::Button::new("Remove…").small())
                        .on_hover_text(
                            "Preview removing this source; mirrored files are never deleted",
                        )
                        .clicked()
                    {
                        remove = Some(name.clone());
                    }
                    ui.end_row();
                }
            });
        if let Some(name) = remove {
            let req = Request::Remove(vec![name]);
            self.lock_run(ctx, program, cat_path, req, false);
        }
    }

    fn lock_inputs(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        program: &str,
        sources: &serde_json::Map<String, Value>,
        usable: bool,
        cat_path: &str,
    ) {
        ui.label(RichText::new("Import Cargo.lock files").strong());
        ui.horizontal(|ui| {
            let l = ui.label("Cargo.lock path");
            ui.add(
                egui::TextEdit::singleline(&mut self.locks.new_path)
                    .hint_text("e.g. C:\\src\\service-a\\Cargo.lock (or drop files on the window)")
                    .desired_width(ui.available_width() - 70.0),
            )
            .labelled_by(l.id);
            let p = self.locks.new_path.trim().to_string();
            if ui
                .add_enabled(!p.is_empty(), egui::Button::new("Add"))
                .clicked()
            {
                self.add_lock_input(p);
                self.locks.new_path.clear();
            }
        });
        let mut drop_row = None;
        let mut names_ok = true;
        for (i, (name, path)) in self.locks.inputs.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let l = ui.label("Source name");
                ui.add(egui::TextEdit::singleline(name).desired_width(160.0))
                    .labelled_by(l.id);
                ui.label(RichText::new(path.as_str()).monospace());
                if ui
                    .small_button("✖")
                    .on_hover_text("Leave this file out")
                    .clicked()
                {
                    drop_row = Some(i);
                }
            });
            if !valid_name(name) {
                names_ok = false;
                ui.label(RichText::new("   Use 1–128 letters, digits, \u{201c}_\u{201d}, \u{201c}-\u{201d} or \u{201c}.\u{201d}.").color(RED));
            } else if sources.contains_key(name.as_str()) {
                ui.label(RichText::new(format!("   Replaces the existing source \u{201c}{name}\u{201d}; the summary shows what changed.")).weak());
            }
        }
        if let Some(i) = drop_row {
            self.locks.inputs.remove(i);
        }
        let names: Vec<&String> = self.locks.inputs.iter().map(|(n, _)| n).collect();
        let unique = names
            .iter()
            .enumerate()
            .all(|(i, n)| !names[..i].contains(n));
        if !unique {
            ui.label(RichText::new("Two files have the same source name.").color(RED));
        }
        let ready = usable && !self.locks.inputs.is_empty() && names_ok && unique;
        if ui
            .add_enabled(
                ready,
                egui::Button::new(format!(
                    "Preview import of {} file(s)",
                    self.locks.inputs.len()
                )),
            )
            .clicked()
        {
            let req = Request::Import(self.locks.inputs.clone());
            self.lock_run(ctx, program, cat_path, req, false);
        }
    }

    fn lock_run(
        &mut self,
        ctx: &egui::Context,
        program: &str,
        cat: &str,
        req: Request,
        write: bool,
    ) {
        let spec = match &req {
            Request::Import(inputs) => ops::crates_import_lock(program, cat, inputs, write),
            Request::Remove(names) => ops::crates_remove_lock(program, cat, names, write),
        };
        self.locks.pending = Some(req);
        self.start(
            ctx,
            if write {
                Op::LockWrite
            } else {
                Op::LockPreview
            },
            spec,
        );
    }

    fn lock_report(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        program: &str,
        usable: bool,
        cat: &str,
    ) {
        if let Some((r, changed)) = self.locks.written.clone() {
            ui.add_space(6.0);
            banner(
                ui,
                GREEN,
                "Catalog updated. Preview a crate mirror to see what will be downloaded.",
            );
            for name in changed {
                banner(ui, AMBER, &format!("\u{201c}{name}\u{201d}: the file changed after the preview. The summary below is what was written."));
            }
            show_report(ui, &r);
        }
        let Some((req, r)) = self.locks.preview.clone() else {
            return;
        };
        ui.add_space(6.0);
        let accepted = r.get("accepted") == Some(&Value::Bool(true));
        if !accepted {
            banner(ui, RED, "Refused: nothing was changed. Fix or leave out the files with problems, then preview again.");
        }
        show_report(ui, &r);
        if accepted {
            let label = match &req {
                Request::Import(i) => format!("Write {} source(s) to the catalog", i.len()),
                Request::Remove(n) => {
                    format!("Remove \u{201c}{}\u{201d} from the catalog", n.join(", "))
                }
            };
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(usable, egui::Button::new(RichText::new(label).strong()))
                    .clicked()
                {
                    self.lock_run(ctx, program, cat, req.clone(), true);
                }
                if ui.button("Cancel").clicked() {
                    self.locks.preview = None;
                }
            });
            if matches!(req, Request::Remove(_)) {
                ui.label(RichText::new("Removing a source only stops protecting its versions in future mirrors. Existing mirrors and their files are not deleted.").weak());
            }
        }
    }
}

/// Render an ImportReport: per-source diff, skipped entries, problems.
fn show_report(ui: &mut egui::Ui, r: &Value) {
    if r.get("external_sources_required") == Some(&Value::Bool(true)) {
        banner(ui, AMBER, "Some packages come from git or local paths. They are not in this mirror: the offline machine needs those sources separately.");
    }
    for src in arr(r, "sources") {
        let (added, removed, unchanged, skipped) = (
            arr(&src, "added"),
            arr(&src, "removed"),
            arr(&src, "unchanged"),
            arr(&src, "skipped"),
        );
        let action = s(&src, "action");
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(s(&src, "source_name")).strong());
                ui.label(RichText::new(action.as_str()).color(match action.as_str() {
                    "added" => GREEN,
                    "removed" => RED,
                    "replaced" => AMBER,
                    _ => ui.visuals().weak_text_color(),
                }));
                ui.label(
                    RichText::new(format!(
                        "{}{}",
                        s(&src, "file_name"),
                        src.get("lock_version")
                            .and_then(Value::as_u64)
                            .map(|v| format!(" · format {v}"))
                            .unwrap_or_default()
                    ))
                    .weak(),
                );
            });
            ui.label(format!(
                "{} pinned version(s) added · {} removed · {} unchanged · {} skipped",
                added.len(),
                removed.len(),
                unchanged.len(),
                skipped.len()
            ));
            if src.get("provenance_changed") == Some(&Value::Bool(true))
                && matches!(action.as_str(), "replaced" | "unchanged")
            {
                ui.label(RichText::new("The file's checksum or import time changed.").weak());
            }
            for (title, pins, color) in [("Added", &added, GREEN), ("Removed", &removed, RED)] {
                if !pins.is_empty() {
                    egui::CollapsingHeader::new(
                        RichText::new(format!("{title} ({})", pins.len())).color(color),
                    )
                    .id_salt((s(&src, "source_name"), title))
                    .show(ui, |ui| {
                        ui.label(RichText::new(pin_list(pins)).monospace());
                    });
                }
            }
            if !skipped.is_empty() {
                egui::CollapsingHeader::new(format!(
                    "Skipped ({}): git and local path entries are not mirrored",
                    skipped.len()
                ))
                .id_salt((s(&src, "source_name"), "skipped"))
                .show(ui, |ui| {
                    for k in &skipped {
                        let kind = match s(k, "kind").as_str() {
                            "git" => "git",
                            "local" => "local path or workspace member",
                            "unused_patch" => "unused [patch]",
                            other => other,
                        }
                        .to_string();
                        ui.label(format!(
                            "line {}: {} {} ({kind})",
                            k.get("line").and_then(Value::as_u64).unwrap_or(0),
                            s(k, "name"),
                            s(k, "version"),
                        ));
                    }
                });
            }
        });
    }
    let diags = arr(r, "diagnostics");
    for d in &diags {
        let at = match (
            d.get("line").and_then(Value::as_u64),
            d.get("column").and_then(Value::as_u64),
        ) {
            (Some(l), Some(c)) => format!(" line {l}, column {c}"),
            (Some(l), None) => format!(" line {l}"),
            _ => String::new(),
        };
        let what = [s(d, "package"), s(d, "field")]
            .into_iter()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(".");
        ui.label(
            RichText::new(format!(
                "✖ {}{at}{}: {}",
                s(d, "source_name"),
                if what.is_empty() {
                    String::new()
                } else {
                    format!(" ({what})")
                },
                s(d, "message")
            ))
            .color(RED),
        );
        let origin = s(d, "registry_origin");
        if !origin.is_empty() {
            ui.label(RichText::new(format!("   registry: {origin}")).weak());
        }
        ui.label(RichText::new(format!("   {}", hint(&s(d, "kind")))).color(AMBER));
    }
    let total = r
        .get("total_diagnostics")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if r.get("truncated") == Some(&Value::Bool(true)) || total > diags.len() as u64 {
        ui.label(RichText::new(format!("Showing {} of {total} problems.", diags.len())).weak());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_contract() {
        assert!(valid_name("service-a"));
        assert!(valid_name("a.b_c-1"));
        assert!(!valid_name(""));
        assert!(!valid_name("."));
        assert!(!valid_name(".."));
        assert!(!valid_name("has space"));
        assert!(!valid_name(&"x".repeat(129)));
    }

    #[test]
    fn suggested_names_come_from_the_folder_and_are_unique() {
        assert_eq!(suggest_name("/src/project-a/Cargo.lock", &[]), "project-a");
        assert_eq!(suggest_name("C:/src/My App/Cargo.lock", &[]), "My-App");
        assert_eq!(
            suggest_name("/x/svc/Cargo.lock", &["svc".into(), "svc-2".into()]),
            "svc-3"
        );
        assert_eq!(suggest_name("Cargo.lock", &[]), "Cargo");
    }
}
