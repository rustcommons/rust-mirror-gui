//! Editable draft of a rust-mirror configuration file (schema 1).
//!
//! The GUI only writes the TOML text. rust-mirror itself validates the file when
//! a command reads it; these checks just catch empty fields early.

/// Hosts qualified by rust-mirror 0.1.x (USER-GUIDE "Choose versions, platforms").
pub const TESTED_HOSTS: &[(&str, &str)] = &[
    ("x86_64-pc-windows-msvc", "Windows x86-64 (MSVC)"),
    ("x86_64-unknown-linux-gnu", "Linux x86-64 (glibc)"),
    ("aarch64-apple-darwin", "macOS Apple Silicon"),
    ("x86_64-apple-darwin", "macOS Intel"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionDraft {
    pub name: String,
    pub toolchain: String,
    pub hosts: Vec<String>,
    pub extra_hosts: String,
    pub minimal: bool,
    pub components: String,
    pub targets: String,
}

impl SelectionDraft {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            toolchain: "1.90.0".into(),
            hosts: vec!["x86_64-pc-windows-msvc".into()],
            extra_hosts: String::new(),
            minimal: true,
            components: String::new(),
            targets: String::new(),
        }
    }

    pub fn all_hosts(&self) -> Vec<String> {
        let mut out = self.hosts.clone();
        for h in split_list(&self.extra_hosts) {
            if !out.contains(&h) {
                out.push(h);
            }
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigDraft {
    pub rustup_version: String,
    pub budget_gib: u32,
    pub selections: Vec<SelectionDraft>,
}

impl Default for ConfigDraft {
    fn default() -> Self {
        Self {
            rustup_version: "1.28.2".into(),
            budget_gib: 10,
            selections: vec![SelectionDraft::new("windows-dev")],
        }
    }
}

pub fn split_list(text: &str) -> Vec<String> {
    text.split([',', ' ', '\n', '\t'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// TOML basic string with escapes.
fn s(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn list(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|v| s(v)).collect();
    format!("[{}]", items.join(", "))
}

impl ConfigDraft {
    /// Problems that make the file certainly unusable. rust-mirror performs the
    /// real validation.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        if self.rustup_version.trim().is_empty() {
            p.push("Rustup version is empty.".into());
        }
        if self.selections.is_empty() {
            p.push("Add at least one selection.".into());
        }
        for (i, sel) in self.selections.iter().enumerate() {
            let label = if sel.name.trim().is_empty() {
                format!("Selection {}", i + 1)
            } else {
                sel.name.clone()
            };
            if sel.name.trim().is_empty() {
                p.push(format!("{label}: name is empty."));
            }
            if sel.toolchain.trim().is_empty() {
                p.push(format!("{label}: toolchain is empty."));
            }
            if sel.all_hosts().is_empty() {
                p.push(format!("{label}: choose at least one host."));
            }
            if self
                .selections
                .iter()
                .filter(|o| o.name == sel.name)
                .count()
                > 1
            {
                p.push(format!("{label}: names must be unique."));
            }
        }
        p.dedup();
        p
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "# Written by rust-mirror-gui. rust-mirror validates this file when it reads it.\n",
        );
        out.push_str("schema = 1\n");
        out.push_str(&format!(
            "rustup_version = {}\n",
            s(self.rustup_version.trim())
        ));
        out.push_str(&format!(
            "max_download_bytes = {}\n",
            u64::from(self.budget_gib) * 1024 * 1024 * 1024
        ));
        for sel in &self.selections {
            out.push_str("\n[[selections]]\n");
            out.push_str(&format!("name = {}\n", s(sel.name.trim())));
            out.push_str(&format!("toolchain = {}\n", s(sel.toolchain.trim())));
            out.push_str(&format!("hosts = {}\n", list(&sel.all_hosts())));
            out.push_str(&format!(
                "profile = {}\n",
                s(if sel.minimal { "minimal" } else { "default" })
            ));
            let components = split_list(&sel.components);
            if !components.is_empty() {
                out.push_str(&format!("components = {}\n", list(&components)));
            }
            let targets = split_list(&sel.targets);
            if !targets.is_empty() {
                out.push_str(&format!("targets = {}\n", list(&targets)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_expected_toml() {
        let mut c = ConfigDraft::default();
        c.selections[0].components = "rust-src, clippy".into();
        c.selections[0].extra_hosts = "aarch64-pc-windows-msvc".into();
        let text = c.to_toml();
        assert!(text.contains("max_download_bytes = 10737418240\n"));
        assert!(text.contains(r#"hosts = ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"]"#));
        assert!(text.contains(r#"components = ["rust-src", "clippy"]"#));
        assert!(!text.contains("targets"));
        assert!(c.problems().is_empty());
    }

    #[test]
    fn escapes_strings() {
        let mut c = ConfigDraft::default();
        c.selections[0].name = "a\"b\\c".into();
        assert!(c.to_toml().contains(r#"name = "a\"b\\c""#));
    }

    #[test]
    fn reports_obvious_problems() {
        let mut c = ConfigDraft::default();
        c.selections.push(SelectionDraft::new("windows-dev"));
        c.selections[0].hosts.clear();
        let p = c.problems().join(" ");
        assert!(p.contains("unique") && p.contains("host"));
    }
}
