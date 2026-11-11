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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Backend,
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
const PURPLE: Color32 = Color32::from_rgb(111, 60, 195);

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
            page: Page::Backend,
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
        };
        app.check_backend(ctx);
        app
    }

    fn check_backend(&mut self, ctx: &egui::Context) {
        let program = std::path::PathBuf::from(self.settings.backend.trim());
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
            Ok(job) => self.job = Some((op, job)),
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
        if self.job_running() || matches!(self.backend, Backend::Checking(_)) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        self.persist(false);

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui, &ctx));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.small(format!(
                    "rust-mirror-gui {} (prototype)",
                    env!("CARGO_PKG_VERSION")
                ));
                ui.separator();
                ui.small("All mirroring work is done by the rust-mirror program.");
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
                    Page::Selections => self.page_selections(ui),
                    Page::Plan => page_plan(ui),
                    Page::Sync => self.page_sync(ui, &ctx),
                    Page::Generations => page_generations(ui),
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
                    ui.label("Checking rust-mirror…");
                }
                Backend::Ok(v) => {
                    ui.label(
                        RichText::new(format!(
                            "✔ rust-mirror {} · event protocol {}",
                            v.tool, v.protocol
                        ))
                        .color(GREEN),
                    );
                }
                Backend::Err(_) => {
                    ui.label(RichText::new("✖ rust-mirror not usable").color(RED));
                    if ui.small_button("Fix…").clicked() {
                        self.page = Page::Backend;
                    }
                }
            }
            if let Some((op, job)) = &self.job {
                ui.separator();
                let (text, color) = health_text(*op, job);
                ui.label(RichText::new(format!("{}: {text}", job.label)).color(color));
                if !job.finished() && ui.small_button("Show").clicked() {
                    self.page = if *op == Op::Publish {
                        Page::Publish
                    } else {
                        Page::Sync
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
        ui.small("SETUP");
        item(ui, Page::Backend, "rust-mirror program", None);
        ui.add_space(8.0);
        ui.small("PREPARE (connected machine)");
        item(ui, Page::Selections, "1  Selections", None);
        item(ui, Page::Plan, "2  Plan preview", Some("later"));
        item(ui, Page::Sync, "3  Sync & finish", None);
        ui.add_space(8.0);
        ui.small("MANAGE");
        item(ui, Page::Generations, "4  Generations", Some("later"));
        ui.add_space(8.0);
        ui.small("DELIVER (offline network)");
        item(ui, Page::Publish, "5  Publish", None);
    }

    fn page_backend(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("rust-mirror program");
        ui.label("This app runs the rust-mirror command-line program and shows what it reports. Choose which program to use.");
        ui.add_space(8.0);
        let changed = text_field(
            ui,
            "Program path",
            &mut self.settings.backend,
            "rust-mirror, or a full path such as C:\\Users\\me\\.cargo\\bin\\rust-mirror.exe",
        )
        .lost_focus();
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            if ui.button("Check").clicked() || changed {
                self.check_backend(ctx);
            }
            ui.label(RichText::new("A plain name is looked up on PATH.").weak());
        });
        ui.add_space(8.0);
        match &self.backend {
            Backend::Checking(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Asking the program for its version…");
                });
            }
            Backend::Ok(v) => {
                banner(ui, GREEN, &format!("rust-mirror {} is ready. It speaks event protocol {}, which this app supports.", v.tool, v.protocol));
            }
            Backend::Err(e) if e.is_empty() => {}
            Backend::Err(e) => {
                banner(ui, RED, e);
                ui.label("Install rust-mirror 0.2.0 or newer, for example:");
                ui.code("cargo install rust-mirror --version 0.2.0 --locked");
            }
        }
        ui.add_space(12.0);
        ui.label(RichText::new("How this app talks to rust-mirror").strong());
        ui.label(format!(
            "Each operation starts `{} … --events jsonl`. The program reports stages, files, byte counts and a heartbeat every few seconds as JSON lines. \
             If the heartbeat stops, this app tells you so, and you can stop or end the program.",
            self.settings.backend.trim()
        ));
    }

    fn page_selections(&mut self, ui: &mut egui::Ui) {
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
        let spec = ops::sync(
            &s.backend,
            &s.config_path,
            &s.sync_output,
            &s.sync_previous,
            self.sync_mode,
        );
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
                let spec = ops::rustup(
                    &s.backend,
                    &s.rustup_snapshot,
                    &s.rustup_version,
                    &s.rustup_output,
                    &s.rustup_promote,
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
        let spec = ops::publish(
            &s.backend,
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
                "Set up a compatible rust-mirror program first."
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
            ui.label(RichText::new("Command:").weak());
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
                }
                ui.end_row();
                ui.label("Transferred");
                let done = p.files.values().filter(|f| f.done).count();
                ui.label(format!(
                    "{} · {} of {} files done",
                    human_bytes(p.bytes_total),
                    done,
                    p.files.len()
                ));
                ui.end_row();
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
        if total > 0 {
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
                        match f.total {
                            Some(t) if t > 0 => ui.add(
                                egui::ProgressBar::new(f.bytes.min(t) as f32 / t as f32)
                                    .desired_width(220.0)
                                    .text(format!("{} / {}", human_bytes(f.bytes), human_bytes(t))),
                            ),
                            _ => ui.label(human_bytes(f.bytes)),
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
            RichText::new(format!("Command: {}", job.command_line))
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
        Health::Failed { kind, message } if kind == "cancelled" => (format!("Stopped. {message}"), AMBER),
        Health::Failed { kind, message } => (format!("Failed [{kind}]: {message}"), RED),
        Health::ExitedWithoutResult(how) => (format!("rust-mirror ended without reporting a result ({how})."), RED),
        Health::Killed => ("Process ended by you.".into(), AMBER),
    }
}

fn page_plan(ui: &mut egui::Ui) {
    ui.heading("Plan preview");
    banner(ui, PURPLE, "Planned. This page will list every file the next sync downloads, once rust-mirror reports plans through the event protocol. Until then, Sync shows each file as it is processed.");
}

fn page_generations(ui: &mut egui::Ui) {
    ui.heading("Generations & retention");
    banner(ui, PURPLE, "Planned. This page will list local generations and preview what a retention policy would remove, once rust-mirror ships its retention commands. Nothing is ever deleted on Nexus.");
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
