//! Crate mirroring pages (rust-mirror crate MVP, docs/CRATES.md): catalog
//! editor with paste, per-crate rules, frozen preview with one whole-preview
//! approval, "why included", sync, and client setup. The window only edits the
//! catalog's JSON form and displays reports; parsing, validation, resolution,
//! downloads and TOML writing all happen in the rust-mirror worker.
use super::{banner, text_field, App, AMBER, BLUE, GREEN, RED};
use crate::config::{split_list, TESTED_HOSTS};
use crate::job::{human_bytes, human_duration};
use crate::ops::{self, Op};
use eframe::egui::{self, RichText};
use serde_json::{json, Map, Value};
use std::time::Duration;

/// Preview older than this gets a stale warning.
const STALE_AFTER: Duration = Duration::from_secs(24 * 3600);
/// Rows rendered at once in the preview table (filter to narrow).
const MAX_ROWS: usize = 400;

#[derive(Default)]
pub(super) struct CrateState {
    /// Editable catalog JSON exactly as the library serializes it.
    catalog: Option<Value>,
    dirty: bool,
    selected: Option<String>,
    new_alias: String,
    new_version: String,
    replace: bool,
    reload_after_save: bool,
    paste_text: String,
    paste_report: Option<Value>,
    /// `needs_approval` sync report (with the frozen preview).
    preview: Option<Value>,
    completed: Option<Value>,
    filter: String,
    explain: Option<(String, String, Value)>,
    client_config: Option<String>,
    message: Option<(bool, String)>,
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}
fn u(v: &Value, k: &str) -> Option<u64> {
    v.get(k).and_then(Value::as_u64)
}
fn list(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
fn obj<'a>(v: &'a mut Value, k: &str) -> &'a mut Map<String, Value> {
    if !v.get(k).is_some_and(Value::is_object) {
        v[k] = json!({});
    }
    v[k].as_object_mut().expect("object")
}
fn temp_file(stem: &str, ext: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!(
        "rust-mirror-gui-{stem}-{}-{nanos}.{ext}",
        std::process::id()
    ))
}

/// Names of the Cargo.lock sources that require a preview row (0.5.0 `sources`).
fn lock_sources(row: &Value) -> Vec<String> {
    row.get("sources")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|x| s(x, "kind") == "lock")
                .map(|x| s(x, "name"))
                .collect()
        })
        .unwrap_or_default()
}

/// True when the row is also wanted by the manual catalog (or the plan
/// predates `sources`).
fn has_manual_source(row: &Value) -> bool {
    row.get("sources")
        .and_then(Value::as_array)
        .is_none_or(|a| a.iter().any(|x| s(x, "kind") == "manual"))
}

/// A new, valid-shaped catalog; the library fills omitted policy defaults.
fn new_catalog() -> Value {
    json!({
        "crates": {},
        "crate-rules": {},
        "crate-policy": {
            "features": "declared",
            "resolver": "2",
            "hosts": ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"],
            "targets": ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"],
            "max-versions": 3000,
            "max-files": 10000,
            "max-metadata-bytes": 268435456u64,
            "max-file-bytes": 536870912u64,
            "max-final-bytes": 10737418240u64,
            "max-transfer-bytes": 10737418240u64
        },
        "crate-output": { "base-url": "" },
        "crate-network": { "contact": "", "concurrency": 4, "attempts": 4, "max-wait-seconds": 60 }
    })
}

/// Plain-English hint for a paste refusal kind.
fn paste_hint(kind: &str) -> &'static str {
    match kind {
        "whole_manifest" => "That looks like a whole Cargo.toml. Paste only the lines of its [dependencies] section.",
        "git" => "Git dependencies cannot be mirrored. Use a crates.io version instead.",
        "path" => "Local path dependencies cannot be mirrored.",
        "workspace" => "\u{201c}workspace = true\u{201d} needs the workspace file. Copy the real version from the workspace's [workspace.dependencies].",
        "alternate_registry" => "Only crates.io is supported.",
        "unknown_field" => "This setting is not supported in a mirror catalog.",
        "syntax" => "This line is not valid TOML.",
        _ => "This entry cannot be used.",
    }
}

impl App {
    /// Store results of crate operations; returns true when handled.
    pub(super) fn harvest_crates(&mut self, op: Op, report: Option<Value>) -> bool {
        let c = &mut self.crates;
        match op {
            Op::CrateLoad => {
                if let Some(r) = report {
                    c.catalog = Some(r);
                    c.dirty = false;
                    c.selected = None;
                }
            }
            Op::CratePaste => c.paste_report = report,
            Op::CrateSave => {
                if report.is_some() {
                    c.message = Some((
                        true,
                        "Saved. Reloaded the file as rust-mirror wrote it.".into(),
                    ));
                    c.reload_after_save = true;
                }
            }
            Op::CratePreview => {
                if let Some(r) = report.filter(|r| s(r, "status") == "needs_approval") {
                    c.preview = Some(r);
                    c.completed = None;
                    c.explain = None;
                }
            }
            Op::CrateSync => {
                if let Some(r) = report.filter(|r| s(r, "status") == "completed") {
                    let mirror = s(&r, "mirror");
                    if !mirror.is_empty() {
                        self.settings.crate_generation = mirror;
                    }
                    c.completed = Some(r);
                    c.client_config = None;
                }
            }
            Op::CrateExplain => {
                if let (Some(r), Some((n, v, _))) = (report, c.explain.take()) {
                    c.explain = Some((n, v, r));
                }
            }
            Op::CrateClientConfig => {
                c.client_config = report.map(|r| s(&r, "config"));
            }
            _ => return false,
        }
        true
    }

    pub(super) fn crates_catalog(&self) -> Option<&Value> {
        self.crates.catalog.as_ref()
    }

    pub(super) fn crates_dirty(&self) -> bool {
        self.crates.dirty
    }

    /// Reload the catalog file after rust-mirror rewrote it.
    pub(super) fn crates_reload(&mut self) {
        self.crates.reload_after_save = true;
    }

    pub(super) fn page_crate_catalog(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let program = ops::worker_program();
        if self.crates.reload_after_save && !self.job_running() {
            self.crates.reload_after_save = false;
            let spec = ops::crates_load(&program, &self.settings.crate_catalog_path);
            self.start(ctx, Op::CrateLoad, spec);
        }
        ui.heading("Crate catalog");
        ui.label("The organization's approved crates.io crates and how many versions of each to mirror. rust-mirror reads and writes this file; the app edits it for you.");
        ui.add_space(6.0);
        text_field(
            ui,
            "Catalog file",
            &mut self.settings.crate_catalog_path,
            "e.g. C:\\mirrors\\crates.toml",
        );
        let path = self.settings.crate_catalog_path.trim().to_string();
        let mut load = false;
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            load = ui
                .add_enabled(
                    !path.is_empty() && !self.job_running(),
                    egui::Button::new("Load"),
                )
                .clicked();
            if ui.button("New catalog").clicked() {
                self.crates.catalog = Some(new_catalog());
                self.crates.dirty = true;
                self.crates.selected = None;
            }
            if self.crates.dirty {
                ui.label(RichText::new("● unsaved changes").color(AMBER));
            }
        });
        if load {
            self.start(ctx, Op::CrateLoad, ops::crates_load(&program, &path));
        }
        if self.crates.catalog.is_none() {
            ui.add_space(8.0);
            self.job_view(ui, &[Op::CrateLoad]);
            return;
        }
        ui.add_space(6.0);
        self.crate_table(ui);
        ui.add_space(6.0);
        self.crate_paste(ui, ctx, &program);
        ui.add_space(6.0);
        self.crate_locks(ui, ctx, &program);
        ui.add_space(6.0);
        self.crate_policy(ui);
        ui.add_space(8.0);
        // Save through the library (canonical TOML).
        banner(ui, AMBER, "Saving rewrites the file in rust-mirror's standard layout: comments and the original order of entries are not kept.");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.crates.replace, "Replace the file if it exists");
            let ready = !path.is_empty() && !self.job_running();
            if ui
                .add_enabled(
                    ready,
                    egui::Button::new(RichText::new("Save catalog").strong()),
                )
                .clicked()
            {
                let file = temp_file("catalog", "json");
                let text =
                    serde_json::to_vec_pretty(self.crates.catalog.as_ref().expect("catalog"));
                match text
                    .map_err(|e| e.to_string())
                    .and_then(|t| std::fs::write(&file, t).map_err(|e| e.to_string()))
                {
                    Ok(()) => {
                        self.crates.dirty = false;
                        let spec = ops::crates_save(&program, &file, &path, self.crates.replace);
                        self.start(ctx, Op::CrateSave, spec);
                    }
                    Err(e) => {
                        self.crates.message =
                            Some((false, format!("Could not prepare the catalog: {e}")))
                    }
                }
            }
        });
        if let Some((ok, m)) = &self.crates.message {
            ui.label(RichText::new(m).color(if *ok { GREEN } else { RED }));
        }
        ui.add_space(8.0);
        self.job_view(ui, &[Op::CrateLoad, Op::CratePaste, Op::CrateSave]);
    }

    fn crate_table(&mut self, ui: &mut egui::Ui) {
        let c = &mut self.crates;
        let cat = c.catalog.as_mut().expect("catalog");
        let crates = cat
            .get("crates")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let rules = cat
            .get("crate-rules")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        ui.label(RichText::new(format!("Crates ({})", crates.len())).strong());
        let mut remove = None;
        egui::Grid::new("crate-list")
            .striped(true)
            .num_columns(7)
            .spacing([12.0, 3.0])
            .show(ui, |ui| {
                // Table cells keep their width; long cells wrap explicitly.
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                for h in [
                    "Name (alias)",
                    "Crate",
                    "Versions",
                    "Features",
                    "Latest",
                    "Pinned",
                    "",
                ] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (alias, dep) in &crates {
                    let rule = rules.get(alias).cloned().unwrap_or(json!({}));
                    let pkg = s(dep, "package");
                    ui.label(alias);
                    ui.label(if pkg.is_empty() { alias.clone() } else { pkg });
                    ui.label(RichText::new(s(dep, "version")).monospace());
                    let mut feats = list(&dep["features"]).join(", ");
                    if dep.get("default-features").and_then(Value::as_bool) == Some(false) {
                        feats = format!(
                            "no defaults{}{feats}",
                            if feats.is_empty() { "" } else { "; " }
                        );
                    }
                    ui.label(feats);
                    ui.label(u(&rule, "latest").unwrap_or(1).to_string());
                    ui.label(list(&rule["pins"]).join(", "));
                    ui.horizontal(|ui| {
                        if ui.small_button("Edit").clicked() {
                            c.selected = Some(alias.clone());
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(alias.clone());
                        }
                    });
                    ui.end_row();
                }
            });
        if let Some(a) = remove {
            obj(cat, "crates").remove(&a);
            obj(cat, "crate-rules").remove(&a);
            if c.selected.as_deref() == Some(a.as_str()) {
                c.selected = None;
            }
            c.dirty = true;
        }
        // Add one crate.
        ui.horizontal(|ui| {
            let l = ui.label("Add crate");
            ui.add(egui::TextEdit::singleline(&mut c.new_alias).hint_text("name, e.g. serde").desired_width(160.0)).labelled_by(l.id);
            ui.add(egui::TextEdit::singleline(&mut c.new_version).hint_text("versions, e.g. 1").desired_width(100.0));
            let alias = c.new_alias.trim().to_string();
            if ui.add_enabled(!alias.is_empty() && !crates.contains_key(&alias), egui::Button::new("Add")).clicked() {
                let version = if c.new_version.trim().is_empty() { "*".to_string() } else { c.new_version.trim().to_string() };
                obj(cat, "crates").insert(alias.clone(), json!({ "version": version, "features": [], "default-features": true, "optional": false }));
                c.selected = Some(alias);
                c.new_alias.clear();
                c.new_version.clear();
                c.dirty = true;
            }
        });
        // Rules for the selected crate.
        let Some(alias) = c.selected.clone().filter(|a| crates.contains_key(a)) else {
            return;
        };
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new(format!("Rules for \u{201c}{alias}\u{201d}")).strong());
            let mut dep = cat["crates"][&alias].clone();
            let mut rule = cat.get("crate-rules").and_then(|r| r.get(&alias)).cloned().unwrap_or(json!({ "latest": 1, "pins": [] }));
            let mut package = s(&dep, "package");
            let mut version = s(&dep, "version");
            let mut features = list(&dep["features"]).join(", ");
            let mut defaults = dep.get("default-features").and_then(Value::as_bool).unwrap_or(true);
            let mut latest = u(&rule, "latest").unwrap_or(1);
            let mut pins = list(&rule["pins"]).join(", ");
            let before = (package.clone(), version.clone(), features.clone(), defaults, latest, pins.clone());
            text_field(ui, "Crate on crates.io", &mut package, "only if different from the name, e.g. two names for rand 0.8 and 0.9");
            text_field(ui, "Versions", &mut version, "Cargo requirement, e.g. 1, ^0.9, >=1.2, <2");
            text_field(ui, "Features", &mut features, "comma separated, e.g. derive");
            ui.horizontal(|ui| {
                ui.add_space(158.0);
                ui.checkbox(&mut defaults, "Default features");
            });
            ui.horizontal(|ui| {
                let l = ui.add_sized([150.0, 20.0], egui::Label::new("Newest versions"));
                ui.add(egui::DragValue::new(&mut latest).range(1..=64)).labelled_by(l.id);
                ui.label(RichText::new("matching versions to keep (yanked versions are skipped)").weak());
            });
            text_field(ui, "Pinned versions", &mut pins, "exact versions always included, e.g. 1.0.228");
            ui.horizontal(|ui| {
                ui.add_space(158.0);
                ui.label(RichText::new("Need two ranges of one crate (e.g. rand 0.8 and 0.9)? Add a second name and set \u{201c}Crate on crates.io\u{201d} to rand.").weak());
            });
            if before != (package.clone(), version.clone(), features.clone(), defaults, latest, pins.clone()) {
                dep["version"] = json!(version.trim());
                match package.trim() {
                    "" => {
                        dep.as_object_mut().map(|m| m.remove("package"));
                    }
                    p => dep["package"] = json!(p),
                }
                dep["features"] = json!(split_list(&features));
                dep["default-features"] = json!(defaults);
                rule["latest"] = json!(latest);
                rule["pins"] = json!(split_list(&pins));
                obj(cat, "crates").insert(alias.clone(), dep);
                let default_rule = latest == 1 && split_list(&pins).is_empty();
                if default_rule {
                    obj(cat, "crate-rules").remove(&alias);
                } else {
                    obj(cat, "crate-rules").insert(alias.clone(), rule);
                }
                c.dirty = true;
            }
            if ui.button("Done").clicked() {
                c.selected = None;
            }
        });
    }

    fn crate_paste(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, program: &str) {
        egui::CollapsingHeader::new(RichText::new("Paste from Cargo.toml").strong())
            .default_open(self.crates.paste_report.is_some())
            .show(ui, |ui| {
                ui.label("Paste the lines of a [dependencies] section. rust-mirror checks every line; nothing is added until you confirm.");
                ui.add(
                    egui::TextEdit::multiline(&mut self.crates.paste_text)
                        .code_editor()
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .hint_text("[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\nanyhow = \"1\""),
                );
                let ready = !self.crates.paste_text.trim().is_empty() && !self.job_running();
                if ui.add_enabled(ready, egui::Button::new("Check pasted lines")).clicked() {
                    let file = temp_file("paste", "toml");
                    match std::fs::write(&file, &self.crates.paste_text) {
                        Ok(()) => {
                            self.crates.paste_report = None;
                            self.start(ctx, Op::CratePaste, ops::crates_paste(program, &file));
                        }
                        Err(e) => self.crates.message = Some((false, format!("Could not prepare the paste: {e}"))),
                    }
                }
                let Some(report) = self.crates.paste_report.clone() else { return };
                let accepted = list(&report["accepted"]);
                let rejected = report.get("rejected").and_then(Value::as_array).cloned().unwrap_or_default();
                if !accepted.is_empty() {
                    ui.label(RichText::new(format!("✔ {} usable: {}", accepted.len(), accepted.join(", "))).color(GREEN));
                }
                for r in &rejected {
                    let at = match (u(r, "line"), u(r, "column")) {
                        (Some(l), Some(c)) => format!("line {l}, column {c}"),
                        (Some(l), None) => format!("line {l}"),
                        _ => "pasted text".into(),
                    };
                    let who = [s(r, "alias"), s(r, "field")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(".");
                    ui.label(RichText::new(format!("✖ {at}{}: {}", if who.is_empty() { String::new() } else { format!(" ({who})") }, s(r, "message"))).color(RED));
                    ui.label(RichText::new(format!("   {}", paste_hint(&s(r, "kind")))).color(AMBER));
                }
                let incoming = report.get("catalog").and_then(|c| c.get("crates")).and_then(Value::as_object).cloned().unwrap_or_default();
                if !incoming.is_empty() {
                    let label = format!("Add {} usable crate(s) to the catalog", incoming.len());
                    if ui.button(label).clicked() {
                        let cat = self.crates.catalog.as_mut().expect("catalog");
                        let crates = obj(cat, "crates");
                        for (alias, dep) in incoming {
                            crates.insert(alias, dep);
                        }
                        self.crates.dirty = true;
                        self.crates.paste_report = None;
                        self.crates.paste_text.clear();
                    }
                }
            });
    }

    fn crate_policy(&mut self, ui: &mut egui::Ui) {
        let c = &mut self.crates;
        let cat = c.catalog.as_mut().expect("catalog");
        let mut changed = false;
        egui::CollapsingHeader::new(RichText::new("Platforms, limits and output").strong()).default_open(true).show(ui, |ui| {
            let pol = obj(cat, "crate-policy");
            let mut hosts: Vec<String> = pol.get("hosts").map(list).unwrap_or_default();
            ui.label("Platforms the mirrored crates must build for");
            for (triple, label) in TESTED_HOSTS {
                let mut on = hosts.iter().any(|h| h == triple);
                if ui.checkbox(&mut on, format!("{label}   {triple}")).changed() {
                    if on {
                        hosts.push(triple.to_string());
                    } else {
                        hosts.retain(|h| h != triple);
                    }
                    pol.insert("hosts".into(), json!(hosts.clone()));
                    pol.insert("targets".into(), json!(hosts.clone()));
                    changed = true;
                }
            }
            let gib = |v: Option<&Value>| v.and_then(Value::as_u64).unwrap_or(0) / (1024 * 1024 * 1024);
            let mut final_gib = gib(pol.get("max-final-bytes"));
            let mut transfer_gib = gib(pol.get("max-transfer-bytes"));
            let mut file_mib = pol.get("max-file-bytes").and_then(Value::as_u64).unwrap_or(0) / (1024 * 1024);
            ui.horizontal(|ui| {
                let l = ui.label("Mirror size limit (GiB)");
                changed |= ui.add(egui::DragValue::new(&mut final_gib).range(1..=4096)).labelled_by(l.id).changed();
                let l = ui.label("Download limit (GiB)");
                changed |= ui.add(egui::DragValue::new(&mut transfer_gib).range(1..=4096)).labelled_by(l.id).changed();
                let l = ui.label("Largest file (MiB)");
                changed |= ui.add(egui::DragValue::new(&mut file_mib).range(1..=512)).labelled_by(l.id).changed();
            });
            pol.insert("max-final-bytes".into(), json!(final_gib * 1024 * 1024 * 1024));
            pol.insert("max-transfer-bytes".into(), json!(transfer_gib * 1024 * 1024 * 1024));
            pol.insert("max-file-bytes".into(), json!(file_mib * 1024 * 1024));
            let out = obj(cat, "crate-output");
            let mut base = out.get("base-url").and_then(Value::as_str).unwrap_or("").to_string();
            if text_field(ui, "Publish address", &mut base, "https://nexus.example.com/repository/crates-001/  (where this mirror will be served)").changed() {
                out.insert("base-url".into(), json!(base.trim()));
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.add_space(158.0);
                ui.label(RichText::new("Cargo downloads crate files from this exact address, so publish the mirror to the same address later.").weak());
            });
            let net = obj(cat, "crate-network");
            let mut contact = net.get("contact").and_then(Value::as_str).unwrap_or("").to_string();
            if text_field(ui, "Contact for crates.io", &mut contact, "e.g. mirror-admin@example.org (sent in the User-Agent)").changed() {
                net.insert("contact".into(), json!(contact.trim()));
                changed = true;
            }
            let mut conc = net.get("concurrency").and_then(Value::as_u64).unwrap_or(4);
            ui.horizontal(|ui| {
                let l = ui.add_sized([150.0, 20.0], egui::Label::new("Parallel downloads"));
                if ui.add(egui::DragValue::new(&mut conc).range(1..=16)).labelled_by(l.id).changed() {
                    net.insert("concurrency".into(), json!(conc));
                    changed = true;
                }
            });
            ui.label(RichText::new("Proxy and company certificate come from Network settings.").weak());
        });
        if changed {
            c.dirty = true;
        }
    }

    pub(super) fn page_crate_sync(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let program = ops::worker_program();
        ui.heading("Crate preview & sync");
        ui.label("1. Preview: rust-mirror reads the crates.io index and lists every crate version the mirror will contain. Nothing large is downloaded. 2. Approve the whole list once. 3. Download and verify.");
        ui.add_space(6.0);
        let st = &mut self.settings;
        text_field(
            ui,
            "Catalog file",
            &mut st.crate_catalog_path,
            "the file from Crate catalog",
        );
        text_field(
            ui,
            "Output folder",
            &mut st.crate_output,
            "a new folder, e.g. D:\\mirrors\\crates-001",
        );
        text_field(
            ui,
            "Previous crate mirror",
            &mut st.crate_previous,
            "optional: the last crate mirror's \\mirror folder (keeps its versions)",
        );
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            ui.checkbox(
                &mut st.measure_sizes,
                "Measure download sizes (HEAD requests; estimates)",
            );
        });
        let ready = !st.crate_catalog_path.trim().is_empty() && !st.crate_output.trim().is_empty();
        let spec = ops::with_network(
            ops::crates_sync(
                &program,
                &st.crate_catalog_path,
                &st.crate_output,
                &st.crate_previous,
                st.measure_sizes,
                None,
                false,
            ),
            &st.network,
            &self.proxy_password,
        );
        if self.start_row(ui, ready, "Preview", &spec) {
            self.crates.preview = None;
            self.crates.completed = None;
            self.start(ctx, Op::CratePreview, spec);
        }
        if let Some(report) = self.crates.preview.clone() {
            self.crate_preview(ui, ctx, &program, &report);
        }
        if let Some(done) = self.crates.completed.clone() {
            let r = &done["report"];
            banner(
                ui,
                GREEN,
                &format!(
                    "Verified crate mirror: {}. {} files; {} downloaded, {} reused.",
                    s(&done, "mirror"),
                    u(r, "files_completed").unwrap_or(0),
                    human_bytes(u(r, "transferred_bytes").unwrap_or(0)),
                    human_bytes(u(r, "reused_bytes").unwrap_or(0))
                ),
            );
        }
        ui.add_space(6.0);
        self.client_setup(ui, ctx, &program);
        ui.add_space(8.0);
        self.job_view(
            ui,
            &[
                Op::CratePreview,
                Op::CrateSync,
                Op::CrateExplain,
                Op::CrateClientConfig,
            ],
        );
    }

    fn crate_preview(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        program: &str,
        report: &Value,
    ) {
        let plan = &report["preview"];
        let plan_id = s(report, "plan_id");
        let plan_path = s(report, "plan_path");
        let counts = &plan["counts"];
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "Preview: {} crates, {} versions, {} files ({} selected directly by the catalog)",
                u(counts, "crates").unwrap_or(0),
                u(counts, "versions").unwrap_or(0),
                u(counts, "files").unwrap_or(0),
                u(counts, "roots").unwrap_or(0)
            ))
            .strong(),
        );
        // Index observation age.
        if let Some(at) = u(plan, "observed_at") {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let age = Duration::from_secs(now.saturating_sub(at));
            let text = format!("crates.io index read {} ago", human_duration(age));
            if age > STALE_AFTER {
                banner(ui, AMBER, &format!("{text}. Newer versions may exist: preview again into a new folder to include them."));
            } else {
                ui.label(RichText::new(text).weak());
            }
        }
        // Size and budgets (computed by the library).
        let size = &plan["size_estimate"];
        let policy = &plan["catalog"]["crate-policy"];
        let known = u(size, "known_archive_bytes").unwrap_or(0);
        let unknown = u(size, "unknown_archives").unwrap_or(0);
        let limit = u(policy, "max-final-bytes").unwrap_or(0);
        let text = format!(
            "about {} of {} mirror limit{}",
            human_bytes(known),
            human_bytes(limit),
            if unknown > 0 {
                format!(
                    " ({unknown} file size(s) unknown{})",
                    if plan["size_estimate"]["estimated"] == json!(true) {
                        ""
                    } else {
                        "; tick Measure download sizes"
                    }
                )
            } else {
                " (estimate)".into()
            }
        );
        if limit > 0 {
            ui.add(
                egui::ProgressBar::new((known as f64 / limit as f64).min(1.0) as f32).text(text),
            );
        } else {
            ui.label(text);
        }
        let b = &plan["budgets"];
        if b["exceeds_final_budget"] == json!(true) {
            banner(ui, RED, "The mirror would be larger than the catalog's size limit. Raise it under Crate catalog, or mirror fewer crates.");
        }
        if b["exceeds_transfer_budget"] == json!(true) {
            banner(
                ui,
                RED,
                "The download would exceed the catalog's download limit.",
            );
        }
        for f in b
            .get("oversized_files")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            ui.label(
                RichText::new(format!(
                    "✖ {} is {} (over the largest-file limit)",
                    s(&f, "path"),
                    human_bytes(u(&f, "expected_bytes").unwrap_or(0))
                ))
                .color(RED),
            );
        }
        for w in list(&plan["warnings"]) {
            ui.label(RichText::new(format!("⚠ {w}")).color(AMBER));
        }
        // Changes against the previous generation.
        let ch = &plan["changes"];
        let n = |k: &str| ch.get(k).and_then(Value::as_array).map_or(0, |a| a.len());
        egui::CollapsingHeader::new(format!(
            "Changes since the previous mirror: {} added, {} kept, {} kept but now yanked, {} kept but no longer selected",
            n("added"),
            n("retained"),
            n("newly_yanked_retained"),
            n("no_longer_selected_retained")
        ))
        .id_salt("crate-changes")
        .show(ui, |ui| {
            for (k, label) in [("added", "Added"), ("newly_yanked_retained", "Now yanked (kept for existing lock files)"), ("no_longer_selected_retained", "No longer selected (kept)")] {
                let ids: Vec<String> = ch.get(k).and_then(Value::as_array).map(|a| a.iter().map(|i| format!("{} {}", s(i, "name"), s(i, "version"))).collect()).unwrap_or_default();
                if !ids.is_empty() {
                    ui.label(RichText::new(label).strong());
                    ui.label(ids.join(", "));
                }
            }
        });
        // Version rows.
        let rows = plan
            .get("versions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        ui.horizontal(|ui| {
            let l = ui.label("Filter");
            ui.add(
                egui::TextEdit::singleline(&mut self.crates.filter)
                    .hint_text("crate or lock source name")
                    .desired_width(200.0),
            )
            .labelled_by(l.id);
        });
        let f = self.crates.filter.trim().to_lowercase();
        let shown: Vec<&Value> = rows
            .iter()
            .filter(|r| {
                f.is_empty()
                    || s(r, "name").contains(&f)
                    || lock_sources(r)
                        .iter()
                        .any(|n| n.to_lowercase().contains(&f))
            })
            .collect();
        let mut explain = None;
        egui::ScrollArea::vertical()
            .id_salt("crate-rows")
            .max_height(320.0)
            .show(ui, |ui| {
                egui::Grid::new("crate-rows-grid")
                    .striped(true)
                    .num_columns(6)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        // Table cells keep their width; long cells wrap explicitly.
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        for h in ["Crate", "Version", "Size", "Flags", "", "Included because"] {
                            ui.label(RichText::new(h).strong());
                        }
                        ui.end_row();
                        for r in shown.iter().take(MAX_ROWS) {
                            ui.label(s(r, "name"));
                            ui.label(RichText::new(s(r, "version")).monospace());
                            match u(r, "expected_bytes") {
                                Some(b) => ui.label(human_bytes(b)),
                                None => ui.label(RichText::new("unknown").weak()),
                            };
                            let mut flags = Vec::new();
                            if r["locked_yanked"] == json!(true) {
                                flags.push("yanked, kept for a Cargo.lock");
                            } else if r["yanked"] == json!(true) {
                                flags.push("yanked");
                            }
                            if r["lock_only"] == json!(true) {
                                flags.push("lock-only");
                            }
                            if r["retained_from_previous"] == json!(true) {
                                flags.push("kept from previous");
                            }
                            // A bounded column, so the reason keeps room.
                            ui.vertical(|ui| {
                                ui.set_max_width(140.0);
                                ui.add(egui::Label::new(flags.join(", ")).wrap());
                            });
                            let roots = list(&r["root_aliases"]);
                            let why = if !roots.is_empty() {
                                format!("listed in catalog ({})", roots.join(", "))
                            } else {
                                let p = &r["primary_reason"];
                                let parent = s(p, "parent");
                                if parent.is_empty() {
                                    "dependency".into()
                                } else {
                                    format!("needed by {parent} ({})", s(p, "requirement"))
                                }
                            };
                            let locks = lock_sources(r);
                            let why = match (locks.is_empty(), has_manual_source(r)) {
                                (true, _) => why,
                                (false, false) => format!("Cargo.lock: {}", locks.join(", ")),
                                (false, true) => format!("{why}; Cargo.lock: {}", locks.join(", ")),
                            };
                            if ui.small_button("Why?").clicked() {
                                explain = Some((s(r, "name"), s(r, "version")));
                            }
                            // The reason is last so it can wrap on narrow windows.
                            ui.add(egui::Label::new(why).wrap());
                            ui.end_row();
                        }
                    });
            });
        if shown.len() > MAX_ROWS {
            ui.label(
                RichText::new(format!(
                    "Showing {MAX_ROWS} of {}; use the filter to find a crate.",
                    shown.len()
                ))
                .weak(),
            );
        }
        if let Some((name, version)) = explain {
            if !self.job_running() {
                self.crates.explain = Some((name.clone(), version.clone(), Value::Null));
                self.start(
                    ctx,
                    Op::CrateExplain,
                    ops::crates_explain(program, std::path::Path::new(&plan_path), &name, &version),
                );
            }
        }
        if let Some((name, version, why)) =
            self.crates.explain.clone().filter(|(_, _, w)| !w.is_null())
        {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(format!("Why {name} {version} is included")).strong());
                for (i, chain) in why
                    .get("chains")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .enumerate()
                {
                    // rust-mirror: { root_alias, steps: [{ identity: {name, version}, reason }] }
                    let mut parts = vec![format!(
                        "catalog entry \u{201c}{}\u{201d}",
                        s(chain, "root_alias")
                    )];
                    for step in chain
                        .get("steps")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                    {
                        let id = &step["identity"];
                        let r = &step["reason"];
                        let mut t = format!("{} {}", s(id, "name"), s(id, "version"));
                        let mut notes = vec![format!("requires {}", s(r, "requirement"))];
                        let feats = list(&r["features"]);
                        if !feats.is_empty() {
                            notes.push(format!("features: {}", feats.join(", ")));
                        }
                        if r["lock_only"] == json!(true) {
                            notes.push("lock file only".into());
                        }
                        if !s(r, "target").is_empty() {
                            notes.push(format!("on {}", s(r, "target")));
                        }
                        t += &format!(" ({})", notes.join("; "));
                        parts.push(t);
                    }
                    ui.add(
                        egui::Label::new(format!("{}. {}", i + 1, parts.join("  \u{2192}  ")))
                            .wrap(),
                    );
                }
                if why["truncated"] == json!(true) {
                    let total = u(&why, "total_chains").map_or("more".into(), |t| t.to_string());
                    ui.label(RichText::new(format!("Showing the first paths of {total}.")).weak());
                }
                if ui.small_button("Close").clicked() {
                    self.crates.explain = None;
                }
            });
        }
        // One whole-preview approval.
        ui.add_space(6.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("Approve and download").strong());
            ui.label(format!(
                "You approve the complete list above, exactly as saved (plan {}…). If you change the catalog or want newer versions, preview again into a new folder.",
                &plan_id[..plan_id.len().min(12)]
            ));
            let st = &self.settings;
            let busy = self.job_running();
            let ok = self.backend_ok();
            let blocked = b["exceeds_final_budget"] == json!(true) || b["exceeds_transfer_budget"] == json!(true);
            let mk = |resume: bool| {
                ops::with_network(
                    ops::crates_sync(&program_of(), &st.crate_catalog_path, &st.crate_output, &st.crate_previous, false, Some(&plan_id), resume),
                    &st.network,
                    &self.proxy_password,
                )
            };
            let approve = mk(false);
            let resume = mk(true);
            let mut start = None;
            ui.horizontal(|ui| {
                if ui.add_enabled(ok && !busy && !blocked, egui::Button::new(RichText::new("Approve this preview and download").strong())).clicked() {
                    start = Some(approve);
                }
                if ui.add_enabled(ok && !busy, egui::Button::new("Resume download")).on_hover_text("Continue an interrupted download of this same approved preview").clicked() {
                    start = Some(resume);
                }
            });
            ui.label(RichText::new(format!("Same as: {}", mk(false).display())).small().weak());
            if let Some(spec) = start {
                self.start(ctx, Op::CrateSync, spec);
            }
        });
        let _ = BLUE;
    }

    fn client_setup(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, program: &str) {
        egui::CollapsingHeader::new(RichText::new("Client setup").strong())
            .default_open(self.crates.completed.is_some() || self.crates.client_config.is_some())
            .show(ui, |ui| {
                ui.label("The Cargo setting that makes a project download crates from your mirror instead of crates.io.");
                text_field(ui, "Crate mirror folder", &mut self.settings.crate_generation, "a completed crate mirror's \\mirror folder");
                let gen = self.settings.crate_generation.trim().to_string();
                ui.horizontal(|ui| {
                    ui.add_space(158.0);
                    if ui.add_enabled(!gen.is_empty() && !self.job_running(), egui::Button::new("Show client setup")).clicked() {
                        self.crates.client_config = None;
                        self.start(ctx, Op::CrateClientConfig, ops::crates_client_config(program, &gen));
                    }
                });
                if let Some(cfg) = self.crates.client_config.clone() {
                    ui.label(RichText::new("Add this to the project's .cargo/config.toml (create the file if needed):").strong());
                    egui::Grid::new("cfg-paths").num_columns(2).min_col_width(110.0).show(ui, |ui| {
                        ui.label("Windows");
                        ui.label(RichText::new("<project>\\.cargo\\config.toml").monospace());
                        ui.end_row();
                        ui.label("Linux / macOS");
                        ui.label(RichText::new("<project>/.cargo/config.toml").monospace());
                        ui.end_row();
                    });
                    let mut text = cfg;
                    ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_rows(5).desired_width(f32::INFINITY).interactive(true));
                    ui.label(RichText::new("If the project already has a [source] section, merge instead of adding a duplicate. Then run cargo fetch --locked and cargo build --locked. Publish the mirror to its publish address first; this does not change any Cargo settings for you.").weak());
                }
            });
    }
}

fn program_of() -> String {
    ops::worker_program()
}
