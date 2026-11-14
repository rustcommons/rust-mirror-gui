//! Network settings page: proxy, NO_PROXY, extra company CA, timeouts, and the
//! connection test. Settings are per machine (the app's config dir), never in
//! the shared mirror configuration; the proxy password is memory only.
use super::{banner, text_field, App, AMBER, GREEN, RED};
use crate::ops::{self, Op};
use crate::settings::ProxyMode;
use eframe::egui::{self, RichText};
use serde_json::Value;

/// Plain-English fix for a failed connection-test step.
pub(super) fn step_fix(name: &str) -> &'static str {
    match name {
        "dns" => "This computer cannot look up static.rust-lang.org. If your company requires a proxy, choose a proxy option above; otherwise ask IT to check name resolution.",
        "connect" => "The connection was refused or timed out. Check the proxy address and port, or ask IT to allow static.rust-lang.org on port 443.",
        "proxy_auth" => "The proxy needs a login. Choose Manual proxy and enter the proxy username and password, or ask IT for proxy access.",
        "tls" => "The server's certificate was not trusted. If your company inspects HTTPS traffic, add the company root certificate (PEM) from IT. Never add a certificate you cannot verify.",
        "download" => "Connected, but the test download failed. Ask IT whether the proxy blocks downloads from static.rust-lang.org.",
        _ => "See the message above.",
    }
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Certificate details table (diagnostic only; never changes trust).
pub(super) fn show_certificates(ui: &mut egui::Ui, id: &str, certs: &[Value]) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            // Table cells keep their width; long cells wrap explicitly.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            for (i, c) in certs.iter().enumerate() {
                if i > 0 {
                    ui.end_row();
                }
                for (label, key) in [
                    ("Subject", "subject"),
                    ("Issuer", "issuer"),
                    ("Expires", "not_after"),
                    ("SHA-256", "sha256"),
                ] {
                    ui.label(RichText::new(label).weak());
                    ui.add(
                        egui::Label::new(RichText::new(s(c, key)).monospace())
                            .selectable(true)
                            .wrap(),
                    );
                    ui.end_row();
                }
            }
        });
}

impl App {
    pub(super) fn page_network(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Network settings");
        ui.label("How this computer reaches static.rust-lang.org for plan previews, syncs, Rustup finish and the release list. Saved for this computer only, never in the mirror configuration. Publishing keeps its own settings.");
        ui.add_space(8.0);
        let program = ops::worker_program();
        let n = &mut self.settings.network;
        ui.label(RichText::new("Connection").strong());
        ui.radio_value(&mut n.mode, ProxyMode::Direct, "Direct (no proxy)");
        ui.radio_value(
            &mut n.mode,
            ProxyMode::Environment,
            "Proxy from environment (HTTPS_PROXY / ALL_PROXY)",
        );
        ui.radio_value(&mut n.mode, ProxyMode::Manual, "Manual proxy");
        let mode = n.mode;
        let mut detect = false;
        match mode {
            ProxyMode::Direct => {}
            ProxyMode::Environment => {
                ui.horizontal(|ui| {
                    ui.add_space(158.0);
                    detect = ui.button("Show detected proxy").clicked();
                });
                if let Some(env) = &self.net_env {
                    let url = s(env, "proxy_url");
                    let no_proxy: Vec<String> = env
                        .get("no_proxy")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default();
                    let creds = env
                        .get("credentials_present")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    egui::Grid::new("env")
                        .num_columns(2)
                        .min_col_width(110.0)
                        .show(ui, |ui| {
                            ui.label("Proxy");
                            ui.label(
                                RichText::new(if url.is_empty() {
                                    "(none set: Direct)".into()
                                } else {
                                    url
                                })
                                .monospace(),
                            );
                            ui.end_row();
                            ui.label("No proxy for");
                            ui.label(
                                RichText::new(if no_proxy.is_empty() {
                                    "(none)".into()
                                } else {
                                    no_proxy.join(", ")
                                })
                                .monospace(),
                            );
                            ui.end_row();
                            ui.label("Login in the variable");
                            ui.label(if creds { "yes (not shown)" } else { "no" });
                            ui.end_row();
                        });
                }
            }
            ProxyMode::Manual => {
                text_field(
                    ui,
                    "Proxy URL",
                    &mut n.proxy_url,
                    "http://proxy.example.com:8080 (no user:password@ in the URL)",
                );
                text_field(ui, "Proxy username", &mut n.proxy_username, "optional");
                ui.horizontal(|ui| {
                    let l = ui.add_sized([150.0, 20.0], egui::Label::new("Proxy password"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.proxy_password)
                            .password(true)
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(l.id);
                });
                ui.horizontal(|ui| {
                    ui.add_space(158.0);
                    ui.vertical(|ui| {
                        ui.set_max_width(ui.available_width());
                        ui.add(egui::Label::new(RichText::new("Kept in memory for this session only and handed to the worker privately. Never saved, logged or shown. Basic proxy logins travel unencrypted to the proxy: use only your company proxy.").weak()).wrap());
                    });
                });
            }
        }
        let n = &mut self.settings.network;
        if mode != ProxyMode::Direct {
            text_field(
                ui,
                "No proxy for",
                &mut n.no_proxy,
                "comma separated, e.g. localhost, .corp.example.com",
            );
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Company certificate").strong());
        text_field(
            ui,
            "Extra CA file (PEM)",
            &mut n.extra_ca_file,
            "optional; added to the built-in trusted roots, never replacing them",
        );
        let ca = n.extra_ca_file.trim().to_string();
        let mut inspect_current = false;
        ui.horizontal(|ui| {
            ui.add_space(158.0);
            inspect_current = ui
                .add_enabled(
                    !ca.is_empty(),
                    egui::Button::new("Show certificate details"),
                )
                .clicked();
        });
        if let Some((file, info)) = &self.ca_info {
            if *file == ca && !ca.is_empty() {
                let certs = info
                    .get("certificates")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                show_certificates(ui, "ca-current", &certs);
            }
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Timeouts").strong());
        ui.horizontal(|ui| {
            let l = ui.label("Connect (s)");
            ui.add(egui::DragValue::new(&mut n.connect_timeout_seconds).range(1..=3600))
                .labelled_by(l.id);
            ui.add_space(16.0);
            let l = ui.label("Read (s)");
            ui.add(egui::DragValue::new(&mut n.read_timeout_seconds).range(1..=3600))
                .labelled_by(l.id);
        });
        let manual_ok = mode != ProxyMode::Manual || !n.proxy_url.trim().is_empty();
        ui.add_space(8.0);
        let check = ops::check_network(&program, &self.settings.network, &self.proxy_password);
        if self.start_row(ui, manual_ok, "Test connection", &check) {
            self.net_check = None;
            self.start(ctx, Op::CheckNetwork, check);
        }
        if detect {
            self.start(ctx, Op::NetworkEnv, ops::network_env(&program));
        }
        if inspect_current {
            self.inspect_pending = false;
            self.start(ctx, Op::InspectCa, ops::inspect_ca(&program, &ca));
        }
        if let Some(report) = self.net_check.clone() {
            self.show_check(ui, ctx, &report);
        }
        ui.add_space(8.0);
        self.job_view(ui, &[Op::CheckNetwork, Op::NetworkEnv, Op::InspectCa]);
    }

    fn show_check(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, report: &Value) {
        let ok = report.get("ok").and_then(Value::as_bool).unwrap_or(false);
        banner(
            ui,
            if ok { GREEN } else { RED },
            if ok {
                "Connection works: this computer can download from static.rust-lang.org with these settings."
            } else {
                "Connection failed. The first failed step below says what to fix."
            },
        );
        let steps = report
            .get("steps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut tls_issuer = None;
        egui::Grid::new("check-steps")
            .num_columns(3)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                // Table cells keep their width; long cells wrap explicitly.
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                for st in &steps {
                    let name = s(st, "name");
                    let (mark, color) = match s(st, "status").as_str() {
                        "passed" => ("✔ passed", GREEN),
                        "failed" => ("✖ failed", RED),
                        _ => ("– skipped", ui.visuals().weak_text_color()),
                    };
                    let label = match name.as_str() {
                        "dns" => "Find server (DNS)",
                        "connect" => "Connect",
                        "proxy_auth" => "Proxy login",
                        "tls" => "Secure connection (TLS)",
                        "download" => "Test download",
                        other => other,
                    };
                    ui.label(label);
                    ui.label(RichText::new(mark).color(color));
                    ui.vertical(|ui| {
                        ui.set_max_width(ui.available_width().min(820.0));
                        ui.add(egui::Label::new(s(st, "message")).wrap());
                        if s(st, "status") == "failed" {
                            ui.add(
                                egui::Label::new(RichText::new(step_fix(&name)).color(AMBER))
                                    .wrap(),
                            );
                        }
                    });
                    ui.end_row();
                    if name == "tls" && s(st, "status") == "failed" {
                        let certs = st
                            .get("certificates")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default();
                        // The last presented certificate's issuer names the (company) root.
                        tls_issuer = certs
                            .last()
                            .map(|c| s(c, "issuer"))
                            .filter(|i| !i.is_empty());
                    }
                }
            });
        if let Some(issuer) = tls_issuer {
            ui.add_space(6.0);
            banner(ui, AMBER, &format!("Certificate issued by {issuer}. This usually means your company inspects HTTPS traffic. Ask IT for this root certificate as a PEM file."));
            if !self.adding_ca && ui.button("Add this company certificate…").clicked() {
                self.adding_ca = true;
            }
        }
        if self.adding_ca {
            self.add_ca_flow(ui, ctx);
        }
    }

    /// User-chosen PEM only: show its details, then add it on explicit confirmation.
    fn add_ca_flow(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let program = ops::worker_program();
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("Add a company certificate").strong());
            ui.label("Choose the root certificate file (PEM) you received from IT. Nothing is trusted automatically: check the details, compare the SHA-256 fingerprint with IT, then confirm.");
            text_field(ui, "Certificate file", &mut self.pending_ca, "e.g. C:\\certs\\company-root.pem");
            let file = self.pending_ca.trim().to_string();
            ui.horizontal(|ui| {
                ui.add_space(158.0);
                if ui.add_enabled(!file.is_empty(), egui::Button::new("Show details")).clicked() {
                    self.inspect_pending = true;
                    self.start(ctx, Op::InspectCa, ops::inspect_ca(&program, &file));
                }
                if ui.button("Cancel").clicked() {
                    self.adding_ca = false;
                }
            });
            let shown = self.ca_info.as_ref().filter(|(f, _)| *f == file && !file.is_empty()).map(|(_, i)| i.clone());
            if let Some(info) = shown {
                let certs = info.get("certificates").and_then(Value::as_array).cloned().unwrap_or_default();
                show_certificates(ui, "ca-pending", &certs);
                if !certs.is_empty() && ui.button(RichText::new("Use this certificate").strong()).clicked() {
                    self.settings.network.extra_ca_file = file.clone();
                    self.adding_ca = false;
                    self.net_check = None;
                }
            }
        });
    }
}
