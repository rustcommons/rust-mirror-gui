//! Crate retention page (rust-mirror 0.4.0 `crates::retention`): list crate
//! generations, set the policy, preview exactly what would be removed and
//! why the rest is kept, then delete only after a typed confirmation. The
//! library re-previews before deleting and refuses a stale plan.
use super::{
    banner, show_catalog, show_prune_plan, text_field, today, write_plan_file, App, AMBER, BLUE,
    RED,
};
use crate::ops::{self, Op};
use eframe::egui::{self, RichText};
use serde_json::{json, Value};

pub(super) struct CrateRetention {
    catalog: Option<Value>,
    listing: Option<Value>,
    next_previous: String,
    as_of: String,
    keep_newest: usize,
    previous_generations: usize,
    pinned: Vec<String>,
    plan: Option<Value>,
    confirm: String,
    removed: Option<Value>,
}

impl Default for CrateRetention {
    fn default() -> Self {
        Self {
            catalog: None,
            listing: None,
            next_previous: String::new(),
            as_of: today(),
            keep_newest: 2,
            previous_generations: 1,
            pinned: Vec::new(),
            plan: None,
            confirm: String::new(),
            removed: None,
        }
    }
}

fn temp_json(v: &Value) -> Result<std::path::PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "rust-mirror-gui-catalog-{}-{nanos}.json",
        std::process::id()
    ));
    std::fs::write(&path, v.to_string())
        .map_err(|e| format!("Could not prepare the catalog: {e}"))?;
    Ok(path)
}

impl App {
    /// Store results of crate-retention operations; returns true when handled.
    pub(super) fn harvest_crate_retention(&mut self, op: Op, report: Option<Value>) -> bool {
        let r = &mut self.crate_retention;
        match op {
            Op::CrateDiscover => {
                r.listing = report.map(|v| v.get("catalog").cloned().unwrap_or(v));
                r.plan = None;
            }
            Op::CrateRetentionPreview => {
                r.plan = report.and_then(|v| v.get("plan").cloned());
                r.confirm.clear();
                r.removed = None;
            }
            Op::CrateRetentionApply => {
                // Success or failure, the reviewed plan is used up. Keep the
                // listing on failure so the advice is shown in context.
                r.plan = None;
                r.confirm.clear();
                if report.is_some() {
                    r.removed = report;
                    r.listing = None;
                }
            }
            Op::CrateLoad if self.page == super::Page::CrateRetention => {
                if let Some(cat) = report {
                    if let Some(p) = cat.get("crate-retention").filter(|p| p.is_object()) {
                        r.keep_newest =
                            p.get("keep-newest").and_then(Value::as_u64).unwrap_or(2) as usize;
                        r.previous_generations = p
                            .get("previous-generations")
                            .and_then(Value::as_u64)
                            .unwrap_or(1) as usize;
                        r.pinned = p
                            .get("pinned-generations")
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str().map(str::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default();
                    }
                    r.catalog = Some(cat);
                }
            }
            _ => return false,
        }
        true
    }

    pub(super) fn page_crate_retention(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let program = ops::worker_program();
        ui.heading("Crate retention");
        ui.label("List the crate mirrors in one workspace folder, preview what the policy would remove and why the rest is kept, and only then delete. Only local folders are affected; nothing on Nexus is ever deleted.");
        ui.add_space(8.0);
        text_field(
            ui,
            "Crate catalog",
            &mut self.settings.crate_catalog_path,
            "the catalog these mirrors were made from (its pinned versions are protected)",
        );
        text_field(
            ui,
            "Workspace folder",
            &mut self.settings.crate_generations_dir,
            "the folder that holds your crate run folders",
        );
        let busy = self.job_running();
        let cat_path = self.settings.crate_catalog_path.trim().to_string();
        let dir = self.settings.crate_generations_dir.trim().to_string();
        let mut discover = false;
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            discover = ui
                .add_enabled(
                    !busy && !cat_path.is_empty() && !dir.is_empty(),
                    egui::Button::new("Discover"),
                )
                .clicked();
            ui.label(
                RichText::new(format!(
                    "Same as: rust-mirror crates generations --directory {dir}"
                ))
                .weak()
                .small(),
            );
        });
        if discover {
            // Load the catalog (policy and pins) first, then list the folder.
            self.crate_retention.listing = None;
            self.crate_retention.plan = None;
            self.start(ctx, Op::CrateLoad, ops::crates_load(&program, &cat_path));
            self.crate_retention_pending_discover = true;
        }
        if self.crate_retention_pending_discover
            && !self.job_running()
            && self.crate_retention.catalog.is_some()
        {
            self.crate_retention_pending_discover = false;
            let mut spec = ops::generations(&program, &dir);
            spec.label = "Discover crate generations".into();
            self.start(ctx, Op::CrateDiscover, spec);
        }
        let r = &mut self.crate_retention;
        let mut crate_gens = Vec::new();
        if let Some(listing) = r.listing.clone() {
            ui.add_space(6.0);
            show_catalog(ui, &listing);
            crate_gens = listing
                .get("generations")
                .and_then(Value::as_array)
                .map(|g| {
                    g.iter()
                        .filter(|g| g.get("kind").and_then(Value::as_str) == Some("crates"))
                        .filter_map(|g| g.get("path").and_then(Value::as_str).map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            if crate_gens.is_empty() {
                banner(ui, AMBER, "No completed crate mirrors in this folder.");
            }
        }
        if r.catalog.is_none() || r.listing.is_none() {
            ui.add_space(8.0);
            if let Some(done) = &self.crate_retention.removed {
                let gens = done
                    .get("removed_generations")
                    .and_then(Value::as_array)
                    .map_or(0, |a| a.len());
                banner(
                    ui,
                    BLUE,
                    &format!("Removed {gens} crate mirror(s). Press Discover to refresh the list."),
                );
            }
            self.job_view(
                ui,
                &[Op::CrateLoad, Op::CrateDiscover, Op::CrateRetentionApply],
            );
            return;
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Retention policy").strong());
        ui.horizontal(|ui| {
            let l = ui.add_sized([150.0, 20.0], egui::Label::new("Next sync's previous"));
            egui::ComboBox::from_id_salt("crate-next-previous")
                .width(ui.available_width() - 10.0)
                .selected_text(if r.next_previous.is_empty() {
                    "choose the crate mirror your next crate sync will reuse"
                } else {
                    r.next_previous.as_str()
                })
                .show_ui(ui, |ui| {
                    for g in &crate_gens {
                        ui.selectable_value(&mut r.next_previous, g.clone(), g.as_str());
                    }
                })
                .response
                .labelled_by(l.id);
        });
        ui.horizontal(|ui| {
            let l = ui.label("Always keep newest");
            ui.add(egui::DragValue::new(&mut r.keep_newest).range(0..=32))
                .labelled_by(l.id);
            ui.label("mirrors");
            ui.add_space(16.0);
            let l = ui.label("Previous mirrors to keep");
            ui.add(egui::DragValue::new(&mut r.previous_generations).range(0..=31))
                .labelled_by(l.id);
        });
        if !crate_gens.is_empty() {
            ui.label("Pinned mirrors (always kept):");
            for g in &crate_gens {
                let mut on = r.pinned.contains(g);
                if ui.checkbox(&mut on, RichText::new(g).monospace()).changed() {
                    if on {
                        r.pinned.push(g.clone());
                    } else {
                        r.pinned.retain(|p| p != g);
                    }
                }
            }
        }
        text_field(ui, "As of (date)", &mut r.as_of, "YYYY-MM-DD");
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            ui.label(RichText::new("Versions pinned in the catalog's crate rules keep the mirror that contains them.").weak());
        });
        let policy = json!({
            "keep-newest": r.keep_newest,
            "previous-generations": r.previous_generations,
            "pinned-generations": r.pinned,
        });
        let mut catalog = r.catalog.clone().unwrap_or_default();
        catalog["crate-retention"] = policy;
        let ready = !r.next_previous.is_empty() && !r.as_of.trim().is_empty();
        let mut preview = false;
        let mut save = false;
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            preview = ui.add_enabled(ready && !busy, egui::Button::new(RichText::new("Preview removal").strong())).clicked();
            save = ui
                .add_enabled(!busy && !cat_path.is_empty(), egui::Button::new("Save policy in catalog"))
                .on_hover_text("Writes this policy into the catalog's [crate-retention] so the command line uses it too")
                .clicked();
        });
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            ui.label(
                RichText::new(format!(
                    "Same as: rust-mirror crates prune --config {cat_path} --directory {dir} --next-previous {} --as-of {} --dry-run",
                    r.next_previous,
                    r.as_of.trim()
                ))
                .weak()
                .small(),
            );
        });
        let (np, asof) = (r.next_previous.clone(), r.as_of.clone());
        if preview {
            match temp_json(&catalog) {
                Ok(file) => self.start(
                    ctx,
                    Op::CrateRetentionPreview,
                    ops::crates_retention_preview(&program, &file, &dir, &np, &asof),
                ),
                Err(e) => self.start_error = Some(e),
            }
        }
        if save {
            match temp_json(&catalog) {
                Ok(file) => self.start(
                    ctx,
                    Op::CrateSave,
                    ops::crates_save(&program, &file, &cat_path, true),
                ),
                Err(e) => self.start_error = Some(e),
            }
        }
        if let Some(plan) = self.crate_retention.plan.clone() {
            ui.add_space(8.0);
            let deletion = &plan["deletion"];
            let n = show_prune_plan(ui, deletion);
            if n > 0 {
                let phrase = format!("delete {n}");
                let mut go = false;
                ui.horizontal(|ui| {
                    let l = ui.label(format!("Type \u{201c}{phrase}\u{201d} to enable deletion:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.crate_retention.confirm)
                            .desired_width(160.0),
                    )
                    .labelled_by(l.id);
                    let enabled =
                        self.crate_retention.confirm.trim() == phrase && self.backend_ok() && !busy;
                    go = ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(
                                RichText::new(format!("Delete {n} crate mirror(s)")).color(RED),
                            ),
                        )
                        .clicked();
                });
                ui.label(RichText::new("rust-mirror previews again just before deleting and refuses if anything changed since this preview. There is no undo.").weak());
                if go {
                    match write_plan_file(&plan) {
                        Ok(path) => self.start(
                            ctx,
                            Op::CrateRetentionApply,
                            ops::crates_retention_apply(&program, &path),
                        ),
                        Err(e) => self.start_error = Some(e),
                    }
                }
            }
        }
        if let Some(done) = &self.crate_retention.removed {
            let gens = done
                .get("removed_generations")
                .and_then(Value::as_array)
                .map_or(0, |a| a.len());
            banner(
                ui,
                BLUE,
                &format!("Removed {gens} crate mirror(s). Press Discover to refresh the list."),
            );
        }
        ui.add_space(8.0);
        self.job_view(
            ui,
            &[
                Op::CrateLoad,
                Op::CrateDiscover,
                Op::CrateRetentionPreview,
                Op::CrateRetentionApply,
                Op::CrateSave,
            ],
        );
    }
}
