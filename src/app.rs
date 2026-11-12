//! The egui user interface. It only edits forms, starts rust-mirror processes
//! and shows what they report; it never blocks on the backend.
use crate::config::{ConfigDraft, SelectionDraft, TESTED_HOSTS};
use crate::job::{human_bytes, human_duration, Health, Job, LogKind};
use crate::ops::{self, Op, SyncMode};
use crate::protocol::BackendVersion;
use crate::settings::{self, Settings};
use eframe::egui::{self, Color32, RichText};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

mod network_page;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Backend,
    Network,
    Selections,
    Plan,
    Sync,
    Generations,
    Publish,
}

enum Backend {
    Checking(Receiver<Result<BackendVersion, String>>),
    Ok(BackendVersion),
    Err(String),
}

const GREEN: Color32 = Color32::from_rgb(26, 127, 55);
const AMBER: Color32 = Color32::from_rgb(154, 103, 0);
const RED: Color32 = Color32::from_rgb(207, 34, 46);
const BLUE: Color32 = Color32::from_rgb(9, 105, 218);

pub struct App {
    settings: Settings,
    saved: Settings,
    last_save: Instant,
    page: Page,
    backend: Backend,
    draft: ConfigDraft,
    selected: usize,
    save_path: String,
    replace_existing: bool,
    save_msg: Option<(bool, String)>,
    sync_mode: SyncMode,
    force_confirmed: bool,
    password: String,
    confirm_fresh: bool,
    confirm_partial: bool,
    job: Option<(Op, Job)>,
    start_error: Option<String>,
    show_stderr: bool,
    harvested: bool,
    gen_next: String,
    gen_as_of: String,
    policy: RetentionForm,
    gen_confirm: String,
    catalog: Option<serde_json::Value>,
    prune_plan: Option<serde_json::Value>,
    /// Proxy password: memory only, sent to the worker over stdin.
    proxy_password: String,
    net_env: Option<serde_json::Value>,
    net_check: Option<serde_json::Value>,
    ca_info: Option<(String, serde_json::Value)>,
    pending_ca: String,
    adding_ca: bool,
    inspect_pending: bool,
    releases: Option<serde_json::Value>,
    releases_tried: bool,
}

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        let settings = settings::load();
        let mut app = Self {
            saved: settings.clone(),
            save_path: if settings.config_path.is_empty() {
                "mirror.toml".into()
            } else {
                settings.config_path.clone()
            },
            settings,
            last_save: Instant::now(),
            page: Page::Selections,
            backend: Backend::Err(String::new()),
            draft: ConfigDraft::default(),
            selected: 0,
            replace_existing: false,
            save_msg: None,
            sync_mode: SyncMode::New,
            force_confirmed: false,
            password: String::new(),
            confirm_fresh: false,
            confirm_partial: false,
            job: None,
            start_error: None,
            show_stderr: true,
            harvested: false,
            gen_next: String::new(),
            gen_as_of: today(),
            policy: RetentionForm::default(),
            gen_confirm: String::new(),
            catalog: None,
            prune_plan: None,
            proxy_password: String::new(),
            net_env: None,
            net_check: None,
            ca_info: None,
            pending_ca: String::new(),
            adding_ca: false,
            inspect_pending: false,
            releases: None,
            releases_tried: false,
        };
        app.check_backend(ctx);
        app
    }

    fn check_backend(&mut self, ctx: &egui::Context) {
        let program = std::path::PathBuf::from(ops::worker_program());
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::job::discover(&program, Duration::from_secs(10)));
            ctx.request_repaint();
        });
        self.backend = Backend::Checking(rx);
    }

    fn backend_ok(&self) -> bool {
        matches!(self.backend, Backend::Ok(_))
    }

    fn job_running(&self) -> bool {
        self.job.as_ref().is_some_and(|(_, j)| !j.finished())
    }

    fn start(&mut self, ctx: &egui::Context, op: Op, spec: crate::job::Spec) {
        self.start_error = None;
        let wake = ctx.clone();
        match Job::start(spec, move || wake.request_repaint()) {
            Ok(job) => {
                self.job = Some((op, job));
                self.harvested = false;
            }
            Err(e) => self.start_error = Some(format!("Could not start rust-mirror: {e}")),
        }
        self.persist(true);
    }

    fn persist(&mut self, now: bool) {
        if self.settings != self.saved && (now || self.last_save.elapsed() > Duration::from_secs(2))
        {
            let _ = settings::save(&self.settings);
            self.saved = self.settings.clone();
            self.last_save = Instant::now();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Backend::Checking(rx) = &self.backend {
            if let Ok(result) = rx.try_recv() {
                self.backend = match result {
                    Ok(v) => Backend::Ok(v),
                    Err(e) => Backend::Err(e),
                };
            }
        }
        if let Some((_, job)) = &mut self.job {
            job.poll();
        }
        self.harvest();
        if self.job_running() || matches!(self.backend, Backend::Checking(_)) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        self.persist(false);

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui, &ctx));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.small(format!("rust-mirror-gui {}", env!("CARGO_PKG_VERSION")));
                ui.separator();
                ui.small("Mirroring work runs in a separate worker process using the built-in rust-mirror library.");
            });
        });
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(200.0)
            .show(ui, |ui| self.nav(ui));
        egui::CentralPanel::default_margins().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| match self.page {
                    Page::Backend => self.page_backend(ui, &ctx),
                    Page::Selections => self.page_selections(ui, &ctx),
                    Page::Plan => self.page_plan(ui, &ctx),
                    Page::Sync => self.page_sync(ui, &ctx),
                    Page::Generations => self.page_generations(ui, &ctx),
                    Page::Network => self.page_network(ui, &ctx),
                    Page::Publish => self.page_publish(ui, &ctx),
                });
        });
    }

    fn on_exit(&mut self) {
        self.persist(true);
    }
}

impl App {
    fn top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("rust-mirror").strong());
            ui.label(
                RichText::new("GUI")
                    .color(Color32::from_rgb(183, 65, 14))
                    .strong(),
            );
            ui.separator();
            match &self.backend {
                Backend::Checking(_) => {
                    ui.spinner();
                    ui.label("Starting the operation worker…");
                }
                Backend::Ok(v) => {
                    ui.label(
                        RichText::new(format!(
                            "✔ rust-mirror {} built in · event protocol {}",
                            v.library.as_deref().unwrap_or("?"),
                            v.protocol
                        ))
                        .color(GREEN),
                    );
                }
                Backend::Err(_) => {
                    ui.label(RichText::new("✖ operation worker not working").color(RED));
                    if ui.small_button("Details").clicked() {
                        self.page = Page::Backend;
                    }
                }
            }
            if let Some((op, job)) = &self.job {
                ui.separator();
                let (text, color) = health_text(*op, job);
                ui.label(RichText::new(format!("{}: {text}", job.label)).color(color));
                if !job.finished() && ui.small_button("Show").clicked() {
                    self.page = match op {
                        Op::Publish => Page::Publish,
                        Op::Plan => Page::Plan,
                        Op::CheckNetwork | Op::NetworkEnv | Op::InspectCa => Page::Network,
                        Op::Releases => Page::Selections,
                        Op::Discover | Op::RetentionPreview | Op::RetentionApply => {
                            Page::Generations
                        }
                        _ => Page::Sync,
                    };
                }
            }
            let _ = ctx;
        });
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        let mut item = |ui: &mut egui::Ui, page: Page, text: &str, note: Option<&str>| {
            let label = match note {
                Some(n) => format!("{text}  ({n})"),
                None => text.to_string(),
            };
            if ui.selectable_label(self.page == page, label).clicked() {
                self.page = page;
            }
        };
        ui.small("PREPARE (connected machine)");
        item(ui, Page::Network, "Network settings", None);
        item(ui, Page::Selections, "1  Selections", None);
        item(ui, Page::Plan, "2  Plan preview", None);
        item(ui, Page::Sync, "3  Sync & finish", None);
        ui.add_space(8.0);
        ui.small("MANAGE");
        item(ui, Page::Generations, "Generations & retention", None);
        ui.add_space(8.0);
        ui.small("DELIVER (offline network)");
        item(ui, Page::Publish, "4  Publish", None);
        ui.add_space(8.0);
        ui.small("HELP");
        item(ui, Page::Backend, "About the engine", None);
    }

    fn page_backend(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("About the engine");
        ui.label(
            "The rust-mirror library is compiled into this program. No separate rust-mirror \
             installation is needed, and the window and engine versions always match.",
        );
        ui.add_space(8.0);
        match &self.backend {
            Backend::Checking(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Starting the operation worker…");
                });
            }
            Backend::Ok(v) => banner(
                ui,
                GREEN,
                &format!(
                    "Ready. rust-mirror-gui {} with rust-mirror library {}; event protocol {}.",
                    v.tool,
                    v.library.as_deref().unwrap_or("(not reported)"),
                    v.protocol
                ),
            ),
            Backend::Err(e) if e.is_empty() => {}
            Backend::Err(e) => {
                banner(ui, RED, e);
                ui.label("Security software may block this program from starting a copy of itself. Allow rust-mirror-gui to run child processes, then check again.");
            }
        }
        if ui.button("Check again").clicked() {
            self.check_backend(ctx);
        }
        ui.add_space(12.0);
        ui.label(RichText::new("How operations run").strong());
        ui.label(
            "Each operation runs in a separate copy of this program (a hidden worker process) that calls \
             the rust-mirror library. The worker reports stages, files, byte counts and a heartbeat every \
             second. If the heartbeat stops, this window tells you, and you can stop or end the worker \
             without losing the window. Each page also shows the equivalent rust-mirror command.",
        );
    }

    fn page_selections(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut refresh_releases = false;
        if !self.releases_tried && !self.job_running() {
            // Load the cached list once, without network access.
            self.releases_tried = true;
            // Only when a cache exists: a first run has none and must not
            // report a failure; Refresh creates it.
            if let Some(cache) = settings::release_cache_dir().filter(|c| c.is_dir()) {
                let spec = ops::releases(
                    &ops::worker_program(),
                    &cache,
                    false,
                    &self.settings.network,
                    "",
                );
                self.start(ctx, Op::Releases, spec);
            }
        }
        ui.heading("Selections");
        ui.label("Choose which Rust toolchains, platforms and components the mirror contains. This writes a configuration file that rust-mirror (GUI or command line) reads.");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let l = ui.label("Rustup version");
            ui.add(egui::TextEdit::singleline(&mut self.draft.rustup_version).desired_width(90.0))
                .labelled_by(l.id);
            ui.add_space(16.0);
            let l = ui.label("Download budget (GiB)");
            ui.add(egui::DragValue::new(&mut self.draft.budget_gib).range(1..=100))
                .labelled_by(l.id);
        });
        ui.add_space(8.0);
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            ui.label(RichText::new("Selections").strong());
            let mut remove = None;
            for (i, sel) in self.draft.selections.iter().enumerate() {
                ui.horizontal(|ui| {
                    let text = format!(
                        "{} — {} · {} host(s)",
                        sel.name,
                        sel.toolchain,
                        sel.all_hosts().len()
                    );
                    if ui.selectable_label(self.selected == i, text).clicked() {
                        self.selected = i;
                    }
                    if self.draft.selections.len() > 1 && ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.draft.selections.remove(i);
                self.selected = self.selected.min(self.draft.selections.len() - 1);
            }
            if ui.button("+ Add selection").clicked() && self.draft.selections.len() < 32 {
                let n = self.draft.selections.len() + 1;
                self.draft
                    .selections
                    .push(SelectionDraft::new(&format!("selection-{n}")));
                self.selected = self.draft.selections.len() - 1;
            }

            let ui = &mut cols[1];
            let sel = &mut self.draft.selections[self.selected];
            ui.label(RichText::new("Selected").strong());
            text_field(ui, "Name", &mut sel.name, "windows-dev");
            text_field(
                ui,
                "Toolchain",
                &mut sel.toolchain,
                "1.90.0, stable or stable-2025-09-18",
            );
            ui.horizontal(|ui| {
                ui.add_space(158.0);
                let choices = release_choices(self.releases.as_ref());
                egui::ComboBox::from_id_salt("toolchain-pick")
                    .selected_text("Pick from release list")
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        if choices.is_empty() {
                            ui.label("No release list yet. Press Refresh.");
                        }
                        for (value, label) in &choices {
                            if ui
                                .selectable_label(sel.toolchain == *value, label)
                                .clicked()
                            {
                                sel.toolchain = value.clone();
                            }
                        }
                    });
                if ui
                    .small_button("Refresh")
                    .on_hover_text("Download the current release list (uses Network settings)")
                    .clicked()
                {
                    refresh_releases = true;
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(158.0);
                ui.label(
                    RichText::new(release_status(self.releases.as_ref()))
                        .weak()
                        .small(),
                );
            });
            ui.label(RichText::new("Host platforms (where the compiler runs)").strong());
            for (triple, label) in TESTED_HOSTS {
                let mut on = sel.hosts.iter().any(|h| h == triple);
                if ui
                    .checkbox(&mut on, format!("{label}   {triple}"))
                    .changed()
                {
                    if on {
                        sel.hosts.push(triple.to_string());
                    } else {
                        sel.hosts.retain(|h| h != triple);
                    }
                }
            }
            text_field(
                ui,
                "Other hosts",
                &mut sel.extra_hosts,
                "advertised but not qualified, comma separated",
            );
            ui.horizontal(|ui| {
                ui.label("Profile");
                ui.radio_value(&mut sel.minimal, true, "minimal (rustc, cargo, rust-std)");
                ui.radio_value(&mut sel.minimal, false, "default (+ clippy, rustfmt, docs)");
            });
            text_field(
                ui,
                "Extra components",
                &mut sel.components,
                "rust-src, rust-analyzer",
            );
            text_field(
                ui,
                "Extra targets",
                &mut sel.targets,
                "wasm32-unknown-unknown",
            );
        });
        ui.add_space(10.0);
        ui.separator();
        let problems = self.draft.problems();
        if problems.is_empty() {
            ui.label(
                RichText::new(
                    "✔ No obvious problems. rust-mirror checks the file fully when it reads it.",
                )
                .color(GREEN),
            );
        } else {
            for p in &problems {
                ui.label(RichText::new(format!("✖ {p}")).color(RED));
            }
        }
        text_field(ui, "Save as", &mut self.save_path, "mirror.toml");
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            ui.checkbox(&mut self.replace_existing, "Replace the file if it exists");
            if ui
                .add_enabled(problems.is_empty(), egui::Button::new("Save configuration"))
                .clicked()
            {
                self.save_msg = Some(save_config(
                    &self.save_path,
                    &self.draft.to_toml(),
                    self.replace_existing,
                ));
                if self.save_msg.as_ref().is_some_and(|m| m.0) {
                    self.settings.config_path = self.save_path.trim().into();
                }
            }
        });
        if let Some((ok, msg)) = &self.save_msg {
            ui.label(RichText::new(msg).color(if *ok { GREEN } else { RED }));
        }
        egui::CollapsingHeader::new("Show the file contents").show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(self.draft.to_toml()).monospace()).selectable(true),
            );
        });
        if refresh_releases {
            if self.job_running() {
                self.start_error = Some(
                    "Another operation is running; refresh the release list when it finishes."
                        .into(),
                );
            } else if let Some(cache) = settings::release_cache_dir() {
                let spec = ops::releases(
                    &ops::worker_program(),
                    &cache,
                    true,
                    &self.settings.network,
                    &self.proxy_password,
                );
                self.start(ctx, Op::Releases, spec);
            }
        }
        if let Some(e) = &self.start_error {
            banner(ui, RED, e);
        }
        // A successful release-list load needs no job panel; show failures.
        let quiet = self.job.as_ref().is_some_and(|(op, j)| {
            *op == Op::Releases && j.finished() && j.progress.result.as_ref().is_some_and(|r| r.ok)
        });
        if !quiet {
            self.job_view(ui, &[Op::Releases]);
        }
    }

    fn page_sync(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Sync & finish");
        ui.label("Download and verify everything in the configuration into a new local generation, then add the Rustup installers. Run this on the connected machine.");
        ui.add_space(8.0);
        let s = &mut self.settings;
        text_field(
            ui,
            "Configuration file",
            &mut s.config_path,
            "mirror.toml (from the Selections page)",
        );
        text_field(
            ui,
            "Output folder",
            &mut s.sync_output,
            "a new folder, e.g. D:\\mirrors\\run-001",
        );
        text_field(
            ui,
            "Previous mirror",
            &mut s.sync_previous,
            "optional: an earlier run's \\mirror folder to reuse verified files",
        );
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            ui.radio_value(&mut self.sync_mode, SyncMode::New, "New run");
            ui.radio_value(
                &mut self.sync_mode,
                SyncMode::Resume,
                "Resume an interrupted run",
            );
            ui.radio_value(&mut self.sync_mode, SyncMode::Force, "Start over");
        });
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            ui.checkbox(
                &mut s.measure_sizes,
                "Measure download sizes first (shows percent and time left; estimates)",
            );
        });
        if self.sync_mode == SyncMode::Force {
            ui.horizontal(|ui| {
                ui.add_space(158.0);
                ui.checkbox(
                    &mut self.force_confirmed,
                    RichText::new(
                        "Delete this output folder's progress and mirror, without a backup",
                    )
                    .color(RED),
                );
            });
        }
        let ready = !s.config_path.trim().is_empty()
            && !s.sync_output.trim().is_empty()
            && (self.sync_mode != SyncMode::Force || self.force_confirmed);
        let mut spec = ops::sync(
            &ops::worker_program(),
            &s.config_path,
            &s.sync_output,
            &s.sync_previous,
            self.sync_mode,
        );
        if s.measure_sizes {
            spec.args.push("--estimate-sizes".into());
        }
        let spec = ops::with_network(spec, &s.network, &self.proxy_password);
        if self.start_row(ui, ready, "Start sync", &spec) {
            self.start(ctx, Op::Sync, spec);
            self.force_confirmed = false;
        }

        ui.add_space(8.0);
        egui::CollapsingHeader::new(RichText::new("Finish with Rustup installers").strong())
            .default_open(true)
            .show(ui, |ui| {
                let s = &mut self.settings;
                if s.rustup_snapshot.is_empty() && !s.sync_output.trim().is_empty() {
                    s.rustup_snapshot = format!(
                        "{}{}mirror",
                        s.sync_output.trim(),
                        std::path::MAIN_SEPARATOR
                    );
                }
                text_field(
                    ui,
                    "Synced mirror",
                    &mut s.rustup_snapshot,
                    "the sync output's \\mirror folder",
                );
                text_field(ui, "Rustup version", &mut s.rustup_version, "1.28.2");
                text_field(
                    ui,
                    "Output folder",
                    &mut s.rustup_output,
                    "a new folder, e.g. D:\\mirrors\\complete-001",
                );
                text_field(
                    ui,
                    "Promote channels",
                    &mut s.rustup_promote,
                    "optional, e.g. stable-2025-09-18",
                );
                let ready = !s.rustup_snapshot.trim().is_empty()
                    && !s.rustup_version.trim().is_empty()
                    && !s.rustup_output.trim().is_empty();
                let spec = ops::with_network(
                    ops::rustup(
                        &ops::worker_program(),
                        &s.rustup_snapshot,
                        &s.rustup_version,
                        &s.rustup_output,
                        &s.rustup_promote,
                    ),
                    &s.network,
                    &self.proxy_password,
                );
                if self.start_row(ui, ready, "Start Rustup finish", &spec) {
                    self.start(ctx, Op::Rustup, spec);
                }
            });
        ui.add_space(8.0);
        self.job_view(ui, &[Op::Sync, Op::Rustup]);
    }

    fn page_publish(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Publish to Nexus");
        ui.label("Upload a completed generation into a fresh, empty Nexus Raw hosted repository (or another static host), then read every file back and check it.");
        ui.add_space(8.0);
        let s = &mut self.settings;
        text_field(
            ui,
            "Generation folder",
            &mut s.publish_source,
            "the completed \\mirror folder, e.g. E:\\complete-001\\mirror",
        );
        text_field(
            ui,
            "Base URL",
            &mut s.publish_url,
            "https://nexus.example.local/repository/rust-generation-001",
        );
        text_field(
            ui,
            "CA bundle (PEM)",
            &mut s.publish_ca,
            "optional; empty uses the system trust",
        );
        text_field(
            ui,
            "Username",
            &mut s.publish_user,
            "publisher account; leave both empty for anonymous",
        );
        ui.horizontal(|ui| {
            let l = ui.add_sized([150.0, 20.0], egui::Label::new("Password"));
            ui.add(
                egui::TextEdit::singleline(&mut self.password)
                    .password(true)
                    .desired_width(f32::INFINITY),
            )
            .labelled_by(l.id);
        });
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            ui.label(RichText::new("Kept in memory for this app session only; passed to rust-mirror as NEXUS_USERNAME/NEXUS_PASSWORD. Never saved or shown.").weak());
        });
        ui.add_space(6.0);
        ui.checkbox(
            &mut self.confirm_fresh,
            "The repository is new, disallows redeploy, and is not yet served to clients.",
        );
        ui.checkbox(
            &mut self.confirm_partial,
            "I understand a failed upload can leave partial files; there is no rollback.",
        );
        let s = &self.settings;
        let creds_ok = s.publish_user.is_empty() == self.password.is_empty();
        if !creds_ok {
            ui.label(RichText::new("Enter both username and password, or neither.").color(AMBER));
        }
        let ready = !s.publish_source.trim().is_empty()
            && !s.publish_url.trim().is_empty()
            && self.confirm_fresh
            && self.confirm_partial
            && creds_ok;
        let check_ready =
            !s.publish_source.trim().is_empty() && !s.publish_url.trim().is_empty() && creds_ok;
        let check = ops::publish_preflight(
            &ops::worker_program(),
            &s.publish_source,
            &s.publish_url,
            &s.publish_ca,
            &s.publish_user,
            &self.password,
        );
        if self.start_row(ui, check_ready, "Check destination (no writes)", &check) {
            self.start(ctx, Op::Publish, check);
        }
        let s = &self.settings;
        let spec = ops::publish(
            &ops::worker_program(),
            &s.publish_source,
            &s.publish_url,
            &s.publish_ca,
            &s.publish_user,
            &self.password,
        );
        if self.start_row(ui, ready, "Publish", &spec) {
            self.start(ctx, Op::Publish, spec);
            self.confirm_partial = false;
        }
        banner(ui, AMBER, "Publishing does not switch clients. After it succeeds, a Nexus administrator adds the repository to the group clients use. This app never activates, deletes or reconfigures Nexus.");
        ui.add_space(8.0);
        self.job_view(ui, &[Op::Publish]);
    }

    /// Keep results that later pages need once a job finishes successfully.
    fn harvest(&mut self) {
        if self.harvested {
            return;
        }
        let Some((op, job)) = &self.job else { return };
        if !job.finished() {
            return;
        }
        self.harvested = true;
        let report = job
            .progress
            .result
            .as_ref()
            .filter(|r| r.ok)
            .and_then(|r| r.report.clone());
        let failure_report = job
            .progress
            .result
            .as_ref()
            .and_then(|r| r.error.as_ref())
            .and_then(|e| e.report.clone());
        match op {
            Op::CheckNetwork => self.net_check = report.or(failure_report),
            Op::NetworkEnv => self.net_env = report,
            Op::Releases if report.is_some() => self.releases = report,
            Op::InspectCa => {
                let file = if self.inspect_pending {
                    self.pending_ca.clone()
                } else {
                    self.settings.network.extra_ca_file.clone()
                };
                self.ca_info = report.map(|r| (file, r));
            }
            Op::Discover => {
                // rust-mirror's `generations` report is the catalog itself.
                self.catalog = report.map(|r| r.get("catalog").cloned().unwrap_or(r));
                self.prune_plan = None;
            }
            Op::RetentionPreview => {
                self.prune_plan = report.and_then(|r| r.get("plan").cloned());
                self.gen_confirm.clear();
            }
            Op::RetentionApply => {
                self.prune_plan = None;
                self.catalog = None;
                self.gen_confirm.clear();
            }
            _ => {}
        }
    }

    fn page_generations(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Generations & retention");
        ui.label("List the generations in one workspace folder, preview what a retention policy would remove, and only then delete. Only local folders are affected; nothing on Nexus is ever deleted.");
        ui.add_space(8.0);
        text_field(
            ui,
            "Workspace folder",
            &mut self.settings.generations_dir,
            "the folder that holds your run-NNN / complete-NNN folders",
        );
        let program = ops::worker_program();
        let ready = !self.settings.generations_dir.trim().is_empty();
        let spec = ops::generations(&program, &self.settings.generations_dir);
        if self.start_row(ui, ready, "Discover", &spec) {
            self.start(ctx, Op::Discover, spec);
        }
        let mut choices = Vec::new();
        if let Some(catalog) = &self.catalog {
            ui.add_space(6.0);
            choices = show_catalog(ui, catalog);
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Retention policy").strong());
        ui.horizontal(|ui| {
            let l = ui.add_sized([150.0, 20.0], egui::Label::new("Next sync's previous"));
            egui::ComboBox::from_id_salt("next-previous")
                .width(ui.available_width() - 10.0)
                .selected_text(if self.gen_next.is_empty() {
                    "choose the generation your next sync will reuse"
                } else {
                    self.gen_next.as_str()
                })
                .show_ui(ui, |ui| {
                    for c in &choices {
                        ui.selectable_value(&mut self.gen_next, c.clone(), c.as_str());
                    }
                })
                .response
                .labelled_by(l.id);
        });
        let f = &mut self.policy;
        egui::Grid::new("policy")
            .num_columns(4)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                let l = ui.label("Newest stable releases to keep");
                ui.add(egui::DragValue::new(&mut f.latest_stable).range(0..=256))
                    .labelled_by(l.id);
                let l = ui.label("Previous generations to keep");
                ui.add(egui::DragValue::new(&mut f.previous_generations).range(0..=31))
                    .labelled_by(l.id);
                ui.end_row();
                let l = ui.label("Nightly days to keep");
                ui.add(egui::DragValue::new(&mut f.nightly_days).range(0..=36_600))
                    .labelled_by(l.id);
                let l = ui.label("Always keep newest folders");
                ui.add(egui::DragValue::new(&mut f.keep_newest).range(0..=32))
                    .labelled_by(l.id);
                ui.end_row();
            });
        text_field(
            ui,
            "Pinned releases",
            &mut f.pinned,
            "always keep, e.g. 1.89.0, stable-2025-09-18",
        );
        text_field(
            ui,
            "As of (date)",
            &mut self.gen_as_of,
            "YYYY-MM-DD; nightly ages count from this date",
        );
        let policy_json = self.policy.to_json();
        let ready = ready && !self.gen_next.is_empty() && !self.gen_as_of.trim().is_empty();
        let spec = ops::retention_preview(
            &program,
            &self.settings.generations_dir,
            &self.gen_next,
            &self.gen_as_of,
            &policy_json,
        );
        if self.start_row(ui, ready, "Preview removal", &spec) {
            self.start(ctx, Op::RetentionPreview, spec);
        }
        if let Some(plan) = self.prune_plan.clone() {
            ui.add_space(8.0);
            let n = show_prune_plan(ui, &plan);
            if n > 0 {
                let phrase = format!("delete {n}");
                ui.horizontal(|ui| {
                    let l = ui.label(format!("Type “{phrase}” to enable deletion:"));
                    ui.add(egui::TextEdit::singleline(&mut self.gen_confirm).desired_width(160.0))
                        .labelled_by(l.id);
                    let enabled = self.gen_confirm.trim() == phrase
                        && self.backend_ok()
                        && !self.job_running();
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(
                                RichText::new(format!("Delete {n} generation(s)")).color(RED),
                            ),
                        )
                        .clicked()
                    {
                        match write_plan_file(&plan) {
                            Ok(path) => {
                                let spec = ops::retention_apply(&program, &path);
                                self.start(ctx, Op::RetentionApply, spec);
                            }
                            Err(e) => self.start_error = Some(e),
                        }
                    }
                });
                ui.label(RichText::new("rust-mirror re-checks every file against this preview before deleting and refuses if anything changed. There is no undo.").weak());
            }
        }
        ui.add_space(8.0);
        self.job_view(
            ui,
            &[Op::Discover, Op::RetentionPreview, Op::RetentionApply],
        );
    }

    fn page_plan(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Plan preview");
        ui.label("Read upstream metadata for every selection and list exactly which files a sync would download. Nothing large is downloaded and nothing is written.");
        ui.add_space(8.0);
        text_field(
            ui,
            "Configuration file",
            &mut self.settings.config_path,
            "mirror.toml (from the Selections page)",
        );
        let ready = !self.settings.config_path.trim().is_empty();
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            ui.checkbox(
                &mut self.settings.measure_sizes,
                "Measure download sizes (HEAD requests; estimates)",
            );
        });
        let spec = ops::with_network(
            ops::plan(
                &ops::worker_program(),
                &self.settings.config_path,
                self.settings.measure_sizes,
            ),
            &self.settings.network,
            &self.proxy_password,
        );
        if self.start_row(ui, ready, "Preview plan", &spec) {
            self.start(ctx, Op::Plan, spec);
        }
        banner(ui, BLUE, "Upstream manifests don't record download sizes. Tick \u{201c}Measure download sizes\u{201d} to ask the server for each file's size first (estimates, not verified). A sync resolves metadata again, so a moving channel such as `stable` can change between this preview and the sync; the sync report shows what was actually frozen.");
        let plans = self
            .job
            .as_ref()
            .filter(|(op, _)| *op == Op::Plan)
            .and_then(|(_, j)| j.progress.result.as_ref())
            .and_then(|r| r.report.as_ref())
            .and_then(|r| r.get("plans"))
            .and_then(|p| p.as_array())
            .cloned();
        if let Some(plans) = plans {
            ui.add_space(8.0);
            show_plans(ui, &plans);
        }
        ui.add_space(8.0);
        self.job_view(ui, &[Op::Plan]);
    }

    /// Start button with the reason it is disabled and the equivalent command.
    fn start_row(
        &mut self,
        ui: &mut egui::Ui,
        ready: bool,
        label: &str,
        spec: &crate::job::Spec,
    ) -> bool {
        let busy = self.job_running();
        let ok = self.backend_ok();
        let mut clicked = false;
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            clicked = ui
                .add_enabled(
                    ready && ok && !busy,
                    egui::Button::new(RichText::new(label).strong()),
                )
                .clicked();
            let why = if !ok {
                "The operation worker is not working; see About the engine."
            } else if busy {
                "Another operation is running."
            } else if !ready {
                "Fill in the required fields."
            } else {
                ""
            };
            ui.label(RichText::new(why).weak());
        });
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            ui.label(RichText::new("Same as:").weak());
            ui.add(
                egui::Label::new(RichText::new(spec.display()).monospace().small())
                    .selectable(true)
                    .wrap(),
            );
        });
        if let Some(e) = &self.start_error {
            banner(ui, RED, e);
        }
        clicked
    }

    fn job_view(&mut self, ui: &mut egui::Ui, ops_here: &[Op]) {
        let Some((op, job)) = &mut self.job else {
            return;
        };
        if !ops_here.contains(op) {
            return;
        }
        let op = *op;
        ui.separator();
        ui.horizontal(|ui| {
            ui.heading(&job.label);
            ui.label(
                RichText::new(format!(
                    "started {} ago",
                    human_duration(job.started.elapsed())
                ))
                .weak(),
            );
        });
        let (text, color) = health_text(op, job);
        banner(ui, color, &text);
        let health = job.health();
        match &health {
            Health::Failed { kind, .. } => {
                ui.label(ops::advice(op, kind));
            }
            Health::ExitedWithoutResult(_) => {
                ui.label(ops::advice(op, "no_result"));
            }
            Health::Killed => {
                ui.label(ops::advice(op, "killed"));
            }
            Health::Stopping(_) | Health::Stalled(_) if op != Op::Publish => {
                ui.label(ops::advice(op, "cancelled"));
            }
            _ => {}
        }

        egui::Grid::new("job-facts")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                let p = &job.progress;
                ui.label("Stage");
                ui.label(match &p.stage {
                    Some(s) => {
                        let n = match (s.index, s.total) {
                            (Some(i), Some(t)) => format!("  ({i} of {t})"),
                            _ => String::new(),
                        };
                        format!(
                            "{}{n}{}",
                            s.name,
                            s.detail
                                .as_deref()
                                .map(|d| format!(" — {d}"))
                                .unwrap_or_default()
                        )
                    }
                    None => "waiting for the first event".into(),
                });
                ui.end_row();
                ui.label("Last event");
                if job.exit.is_some() {
                    ui.label(format!(
                        "process ended after {}",
                        human_duration(
                            job.exited_at
                                .unwrap_or(job.last_event)
                                .duration_since(job.started)
                        )
                    ));
                } else {
                    ui.label(format!(
                        "{} ago (heartbeat expected every {})",
                        human_duration(job.last_event.elapsed()),
                        human_duration(p.heartbeat_interval())
                    ));
                    ui.end_row();
                    ui.label("Activity");
                    let idle = job.last_progress.elapsed();
                    let doing = p
                        .heartbeat
                        .as_ref()
                        .map(|(_, op, path)| match path {
                            Some(path) => format!("{op}: {}", short_path(path)),
                            None => op.clone(),
                        })
                        .unwrap_or_default();
                    if idle >= Duration::from_secs(30) {
                        ui.label(
                            RichText::new(format!(
                                "alive, but no new progress for {} ({doing}). A slow network read can look like this.",
                                human_duration(idle)
                            ))
                            .color(AMBER),
                        );
                    } else {
                        ui.label(format!("progressing — {doing}"));
                    }
                }
                ui.end_row();
                if !p.files.is_empty() {
                    ui.label("Transferred");
                    let done = p.files.values().filter(|f| f.done).count();
                    ui.label(format!(
                        "{} over the network · {} processed locally · {} of {} files done",
                        human_bytes(p.bytes_total),
                        human_bytes(p.local_bytes),
                        done,
                        p.files.len()
                    ));
                    ui.end_row();
                }
                // Scalar fields of a successful report (e.g. mirror path, files,
                // verified bytes), shown as reported by the library.
                if let Some(report) = p
                    .result
                    .as_ref()
                    .filter(|r| r.ok)
                    .and_then(|r| r.report.as_ref())
                    .and_then(|r| r.as_object())
                {
                    // Sync nests its counters under `report`; show both levels.
                    let nested = report.get("report").and_then(|r| r.as_object());
                    for (key, value) in report.iter().chain(nested.into_iter().flatten()) {
                        let shown = match value {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => match n.as_u64() {
                                Some(b) if key.ends_with("bytes") || key.ends_with("bytes_verified") => {
                                    format!("{} ({b})", human_bytes(b))
                                }
                                _ => n.to_string(),
                            },
                            serde_json::Value::Bool(b) => b.to_string(),
                            _ => continue,
                        };
                        ui.label(RichText::new(key.replace('_', " ")).weak());
                        ui.add(egui::Label::new(RichText::new(shown).monospace()).selectable(true));
                        ui.end_row();
                    }
                }
                if p.protocol_warnings > 0 {
                    ui.label("Protocol");
                    ui.label(
                        RichText::new(format!(
                            "{} unreadable line(s); see activity",
                            p.protocol_warnings
                        ))
                        .color(AMBER),
                    );
                    ui.end_row();
                }
            });

        let (known, total): (u64, u64) = job
            .progress
            .files
            .values()
            .filter_map(|f| f.total.map(|t| (f.bytes.min(t), t)))
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        let summary = job
            .progress
            .summary
            .as_ref()
            .and_then(|t| t.expected_bytes.filter(|e| *e > 0).map(|e| (t, e)));
        if let Some((t, expected)) = summary {
            // Whole-operation estimate from rust-mirror's transfer_summary.
            let fraction = (t.completed_bytes as f64 / expected as f64).min(1.0);
            let eta = match (job.exit, t.eta_seconds) {
                (None, Some(s)) if s.is_finite() && s >= 0.0 => {
                    format!(
                        " · about {} left",
                        human_duration(Duration::from_secs(s.ceil() as u64))
                    )
                }
                _ => String::new(),
            };
            let rate = t
                .bytes_per_second
                .filter(|r| r.is_finite() && *r > 0.0)
                .map(|r| format!(" · {}/s", human_bytes(r as u64)))
                .unwrap_or_default();
            ui.add(egui::ProgressBar::new(fraction as f32).text(format!(
                "{:.0}% · {} of about {}{rate}{eta} (estimate)",
                fraction * 100.0,
                human_bytes(t.completed_bytes),
                human_bytes(expected)
            )));
        } else if total > 0 {
            ui.add(
                egui::ProgressBar::new(known as f32 / total as f32).text(format!(
                    "{} of {} (known sizes)",
                    human_bytes(known),
                    human_bytes(total)
                )),
            );
        }

        let mut clear = false;
        ui.horizontal(|ui| {
            let running = job.exit.is_none();
            if running
                && job.stop_requested.is_none()
                && ui
                    .button("Stop")
                    .on_hover_text("Ask rust-mirror to stop at the next safe point")
                    .clicked()
            {
                job.request_stop();
            }
            let offer_kill = running
                && (job
                    .stop_requested
                    .is_some_and(|t| t.elapsed() > Duration::from_secs(5))
                    || matches!(health, Health::Stalled(_)));
            if offer_kill
                && ui
                    .button(RichText::new("End process now").color(RED))
                    .on_hover_text("Terminate rust-mirror immediately")
                    .clicked()
            {
                job.kill();
            }
            if job.finished() && ui.button("Clear").clicked() {
                clear = true;
            }
        });
        if clear {
            self.job = None;
            return;
        }

        let active: Vec<_> = job
            .progress
            .files
            .iter()
            .filter(|(_, f)| !f.done)
            .rev()
            .take(6)
            .collect();
        if !active.is_empty() {
            ui.label(RichText::new("Current files").strong());
            egui::Grid::new("files")
                .num_columns(3)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for (path, f) in active {
                        ui.label(&f.action);
                        ui.label(RichText::new(short_path(path)).monospace());
                        match (f.hashing, f.total) {
                            (Some((c, Some(t))), _) if t > 0 => ui.add(
                                egui::ProgressBar::new(c.min(t) as f32 / t as f32)
                                    .desired_width(220.0)
                                    .text(format!(
                                        "hashing {} / {}",
                                        human_bytes(c),
                                        human_bytes(t)
                                    )),
                            ),
                            (Some((c, _)), _) => ui.label(format!("hashing {}", human_bytes(c))),
                            (None, Some(t)) if t > 0 => ui.add(
                                egui::ProgressBar::new(f.bytes.min(t) as f32 / t as f32)
                                    .desired_width(220.0)
                                    .text(format!("{} / {}", human_bytes(f.bytes), human_bytes(t))),
                            ),
                            (None, _) => ui.label(human_bytes(f.bytes)),
                        };
                        ui.end_row();
                    }
                });
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("Activity").strong());
            ui.checkbox(&mut self.show_stderr, "include program text output");
        });
        let show_stderr = self.show_stderr;
        egui::Frame::new()
            .fill(Color32::from_gray(30))
            .corner_radius(6)
            .inner_margin(8)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("activity")
                    .max_height(260.0)
                    .stick_to_bottom(true)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for line in job
                            .log
                            .iter()
                            .filter(|l| show_stderr || l.kind != LogKind::Stderr)
                        {
                            let color = match line.kind {
                                LogKind::Stage => Color32::from_rgb(121, 192, 255),
                                LogKind::Artifact => Color32::from_rgb(126, 231, 135),
                                LogKind::Info => Color32::from_gray(210),
                                LogKind::Warn => Color32::from_rgb(227, 179, 65),
                                LogKind::Error => Color32::from_rgb(255, 123, 114),
                                LogKind::Stderr => Color32::from_gray(140),
                            };
                            ui.label(
                                RichText::new(format!(
                                    "{:>7}  {}",
                                    format!("{:.1}s", line.at.as_secs_f32()),
                                    line.text
                                ))
                                .monospace()
                                .color(color),
                            );
                        }
                    });
            });
        ui.label(
            RichText::new(format!("Same as: {}", job.command_line))
                .monospace()
                .small()
                .weak(),
        );
    }
}

fn health_text(op: Op, job: &Job) -> (String, Color32) {
    let stage = job
        .progress
        .stage
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "starting".into());
    match job.health() {
        Health::Running(q) => (format!("Running — {stage}. Last event {} ago.", human_duration(q)), BLUE),
        Health::Stalled(q) => (
            format!("No events for {} (stage: {stage}). rust-mirror may be stuck. You can wait, Stop, or end the process.", human_duration(q)),
            AMBER,
        ),
        Health::Stopping(t) => (
            format!(
                "Stopping — asked {} ago; last event {} ago. Waiting for rust-mirror to reach a safe point.{}",
                human_duration(t),
                human_duration(job.last_event.elapsed()),
                if op == Op::Sync { " Progress is kept for Resume." } else { "" }
            ),
            AMBER,
        ),
        Health::Succeeded => ("Finished successfully.".into(), GREEN),
        Health::Failed { kind, message } if kind == "cancelled" => (
            if message == "stopped by user" {
                "Stopped at your request.".to_string()
            } else {
                format!("Stopped: {message}")
            },
            AMBER,
        ),
        Health::Failed { kind, message } => (format!("Failed [{kind}]: {message}"), RED),
        Health::ExitedWithoutResult(how) => (format!("rust-mirror ended without reporting a result ({how})."), RED),
        Health::Killed => ("Process ended by you.".into(), AMBER),
    }
}

#[derive(Clone, Debug)]
struct RetentionForm {
    latest_stable: usize,
    nightly_days: u32,
    previous_generations: usize,
    keep_newest: usize,
    pinned: String,
}

impl Default for RetentionForm {
    fn default() -> Self {
        Self {
            latest_stable: 2,
            nightly_days: 0,
            previous_generations: 1,
            keep_newest: 2,
            pinned: String::new(),
        }
    }
}

impl RetentionForm {
    /// rust-mirror `retention::Policy` as JSON; the library validates it.
    fn to_json(&self) -> String {
        serde_json::json!({
            "pinned_releases": crate::config::split_list(&self.pinned),
            "latest_stable": self.latest_stable,
            "nightly_days": self.nightly_days,
            "previous_generations": self.previous_generations,
            "keep_newest": self.keep_newest,
            "pinned_generations": [],
        })
        .to_string()
    }
}

/// Today's UTC date as YYYY-MM-DD (civil-from-days).
fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The reviewed plan goes to the worker through a private temporary file.
fn write_plan_file(plan: &serde_json::Value) -> Result<std::path::PathBuf, String> {
    let path = std::env::temp_dir().join(format!(
        "rust-mirror-gui-plan-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::write(&path, plan.to_string())
        .map_err(|e| format!("Could not write the plan file: {e}"))?;
    Ok(path)
}

/// Shows discovered generations and runs; returns completed generation paths.
fn show_catalog(ui: &mut egui::Ui, catalog: &serde_json::Value) -> Vec<String> {
    let s = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    let empty = Vec::new();
    let gens = catalog
        .get("generations")
        .and_then(|g| g.as_array())
        .unwrap_or(&empty);
    let runs = catalog
        .get("runs")
        .and_then(|g| g.as_array())
        .unwrap_or(&empty);
    let issues = catalog
        .get("issues")
        .and_then(|g| g.as_array())
        .unwrap_or(&empty);
    let mut choices = Vec::new();
    ui.label(
        RichText::new(format!(
            "{} generation(s), {} run folder(s)",
            gens.len(),
            runs.len()
        ))
        .strong(),
    );
    egui::Grid::new("gens")
        .striped(true)
        .num_columns(4)
        .spacing([14.0, 3.0])
        .show(ui, |ui| {
            for h in ["Generation", "Toolchains", "Size", "Schema"] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for g in gens {
                let path = s(g, "path");
                choices.push(path.clone());
                ui.label(RichText::new(&path).monospace());
                let tc: Vec<String> = g
                    .get("toolchains")
                    .and_then(|t| t.as_array())
                    .map(|t| t.iter().map(|r| s(r, "pinned_toolchain")).collect())
                    .unwrap_or_default();
                ui.label(tc.join(", "));
                ui.label(human_bytes(
                    g.get("recorded_bytes")
                        .and_then(|b| b.as_u64())
                        .unwrap_or(0),
                ));
                ui.label(g.get("schema").map(|v| v.to_string()).unwrap_or_default());
                ui.end_row();
            }
            for r in runs {
                ui.label(RichText::new(s(r, "path")).monospace());
                ui.label(format!(
                    "{} run: {}",
                    s(r, "kind"),
                    r.get("state").and_then(|x| x.as_str()).unwrap_or("?")
                ));
                ui.label("");
                ui.label("");
                ui.end_row();
            }
        });
    for i in issues {
        ui.label(RichText::new(format!("⚠ {}: {}", s(i, "path"), s(i, "message"))).color(AMBER));
    }
    choices
}

/// Shows a retention preview; returns how many generations would be removed.
fn show_prune_plan(ui: &mut egui::Ui, plan: &serde_json::Value) -> usize {
    let s = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    let empty = Vec::new();
    let folders = plan
        .get("folders")
        .and_then(|f| f.as_array())
        .unwrap_or(&empty);
    let kept = plan
        .get("kept")
        .and_then(|f| f.as_array())
        .unwrap_or(&empty);
    let total = plan
        .get("total_bytes")
        .and_then(|b| b.as_u64())
        .unwrap_or(0);
    if folders.is_empty() {
        banner(ui, GREEN, "Nothing would be removed under this policy.");
    } else {
        banner(
            ui,
            AMBER,
            &format!(
                "{} generation folder(s) would be removed, {} in total.",
                folders.len(),
                human_bytes(total)
            ),
        );
    }
    egui::Grid::new("prune")
        .striped(true)
        .num_columns(2)
        .spacing([14.0, 3.0])
        .show(ui, |ui| {
            for f in folders {
                ui.label(RichText::new("remove").color(RED));
                ui.label(
                    RichText::new(format!(
                        "{}  ({})",
                        s(f, "path"),
                        human_bytes(f.get("bytes").and_then(|b| b.as_u64()).unwrap_or(0))
                    ))
                    .monospace(),
                );
                ui.end_row();
            }
            for k in kept {
                let reasons: Vec<String> = k
                    .get("reasons")
                    .and_then(|r| r.as_array())
                    .map(|r| {
                        r.iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                ui.label(RichText::new("keep").color(GREEN));
                ui.label(format!("{} — {}", s(k, "path"), reasons.join("; ")));
                ui.end_row();
            }
        });
    folders.len()
}

/// Toolchain picker entries (value, label) from a release catalog: stable
/// versions with dates, then beta and nightly by date.
fn release_choices(catalog: Option<&serde_json::Value>) -> Vec<(String, String)> {
    let Some(list) = catalog
        .and_then(|c| c.get("releases"))
        .and_then(|r| r.as_array())
    else {
        return Vec::new();
    };
    let s = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    // rust-mirror's Release: channel, toolchain (numeric stable version or
    // dated channel selector) and date. Stable first, then beta, then nightly.
    let mut out = Vec::new();
    // All stable releases; only the most recent dated beta/nightly builds
    // (the index lists thousands). Older ones can still be typed.
    for (channel, limit) in [("stable", usize::MAX), ("beta", 20), ("nightly", 30)] {
        for r in list
            .iter()
            .filter(|r| s(r, "channel") == channel)
            .take(limit)
        {
            let toolchain = s(r, "toolchain");
            if toolchain.is_empty() {
                continue;
            }
            let label = if channel == "stable" {
                format!("{toolchain}  (stable, {})", s(r, "date"))
            } else {
                toolchain.clone()
            };
            out.push((toolchain, label));
        }
    }
    out
}

fn release_status(catalog: Option<&serde_json::Value>) -> String {
    let Some(c) = catalog else {
        return "release list not loaded".into();
    };
    let age = c.get("age_seconds").and_then(|a| a.as_u64()).unwrap_or(0);
    let when = if age < 120 {
        "just now".to_string()
    } else {
        format!("{} old", human_duration(Duration::from_secs(age)))
    };
    let cached = c.get("cached").and_then(|x| x.as_bool()).unwrap_or(false);
    let warn = c
        .get("warning")
        .filter(|w| !w.is_null())
        .map(|_| " · refresh failed, showing cache")
        .unwrap_or("");
    format!(
        "{} list, {when}{warn}",
        if cached { "cached" } else { "fresh" }
    )
}

fn show_plans(ui: &mut egui::Ui, plans: &[serde_json::Value]) {
    let text = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    let total: usize = plans
        .iter()
        .map(|p| {
            p.get("artifacts")
                .and_then(|a| a.as_array())
                .map_or(0, |a| a.len())
        })
        .sum();
    ui.label(
        RichText::new(format!(
            "{} selection(s) · {} files to download",
            plans.len(),
            total
        ))
        .strong(),
    );
    // Opt-in size estimates (HEAD requests). Never treated as verified sizes.
    let measured: Vec<&serde_json::Value> = plans.iter().filter_map(|p| p.get("sizes")).collect();
    if !measured.is_empty() {
        let known: u64 = measured
            .iter()
            .filter_map(|m| m.get("known_bytes").and_then(|b| b.as_u64()))
            .sum();
        let unknown: u64 = measured
            .iter()
            .filter_map(|m| m.get("unknown_artifacts").and_then(|b| b.as_u64()))
            .sum();
        let budget = plans
            .iter()
            .filter_map(|p| p.get("max_download_bytes").and_then(|b| b.as_u64()))
            .max()
            .unwrap_or(0);
        let over = measured.iter().any(|m| {
            m.get("exceeds_budget")
                .and_then(|b| b.as_bool())
                .unwrap_or(false)
        });
        let text = format!(
            "about {} of {} budget (estimate{})",
            human_bytes(known),
            human_bytes(budget),
            if unknown > 0 {
                format!("; {unknown} file size(s) unknown")
            } else {
                String::new()
            }
        );
        if budget > 0 {
            ui.add(
                egui::ProgressBar::new((known as f64 / budget as f64).min(1.0) as f32).text(text),
            );
        } else {
            ui.label(text);
        }
        if over {
            ui.label(RichText::new("The estimate is larger than the download budget. Raise the budget in Selections or select fewer items.").color(AMBER));
        }
    }
    for (i, plan) in plans.iter().enumerate() {
        let sel = plan.get("selection").cloned().unwrap_or_default();
        let artifacts = plan
            .get("artifacts")
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default();
        let title = format!(
            "{} — requested {} → pinned {} (Rust {}, {}) · {} files",
            text(&sel, "name"),
            text(&sel, "toolchain"),
            text(plan, "pinned_toolchain"),
            text(plan, "resolved_release"),
            text(plan, "date"),
            artifacts.len()
        );
        let sizes: std::collections::HashMap<String, Option<u64>> = plan
            .get("sizes")
            .and_then(|z| z.get("artifacts"))
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .map(|x| {
                        (
                            text(x, "path"),
                            x.get("expected_bytes").and_then(|b| b.as_u64()),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let has_sizes = plan.get("sizes").is_some();
        egui::CollapsingHeader::new(title)
            .id_salt(("plan", i))
            .default_open(i == 0)
            .show(ui, |ui| {
                egui::Grid::new(("plan-grid", i))
                    .striped(true)
                    .num_columns(if has_sizes { 6 } else { 5 })
                    .spacing([14.0, 3.0])
                    .show(ui, |ui| {
                        for h in ["Package", "Target", "Kind", "File", "SHA-256"] {
                            ui.label(RichText::new(h).strong());
                        }
                        if has_sizes {
                            ui.label(RichText::new("Size (est.)").strong());
                        }
                        ui.end_row();
                        for a in &artifacts {
                            ui.label(text(a, "package"));
                            ui.label(RichText::new(text(a, "target")).monospace());
                            ui.label(text(a, "kind"));
                            let path = text(a, "path");
                            ui.label(RichText::new(short_path(&path)).monospace())
                                .on_hover_text(&path);
                            let sha = text(a, "sha256");
                            ui.label(
                                RichText::new(format!("{}…", &sha[..sha.len().min(12)]))
                                    .monospace(),
                            )
                            .on_hover_text(&sha);
                            if has_sizes {
                                match sizes.get(&path).copied().flatten() {
                                    Some(b) => ui.label(human_bytes(b)),
                                    None => ui.label(RichText::new("unknown").weak()),
                                };
                            }
                            ui.end_row();
                        }
                    });
            });
    }
}

fn banner(ui: &mut egui::Ui, color: Color32, text: &str) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(6)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(color));
        });
}

/// Labelled single-line field; the label is linked for screen readers.
fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) -> egui::Response {
    ui.horizontal(|ui| {
        let l = ui.add_sized([150.0, 20.0], egui::Label::new(label));
        ui.add(
            egui::TextEdit::singleline(value)
                .hint_text(hint)
                .desired_width(f32::INFINITY),
        )
        .labelled_by(l.id)
    })
    .inner
}

fn short_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn save_config(path: &str, text: &str, replace: bool) -> (bool, String) {
    let path = std::path::Path::new(path.trim());
    if path.as_os_str().is_empty() {
        return (false, "Enter a file name.".into());
    }
    let result = if replace {
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path))
    } else {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()))
    };
    match result {
        Ok(()) => (true, format!("Saved {}.", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (
            false,
            "The file exists. Tick “Replace the file if it exists” or choose another name.".into(),
        ),
        Err(e) => (false, format!("Could not save: {e}")),
    }
}
