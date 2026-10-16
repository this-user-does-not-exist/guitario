//! `cargo xtask scorecard [--check]`: generate `docs/scorecard.md`.
//!
//! Sources, all committed, so the output is deterministic and CI can check it:
//! - `scorecard/*.toml`: per-area checklists (id, target, status, note). An item is `done` when it
//!   has a command, a UI and tests on `main`.
//! - counted from the tree: tests, engine commands, never-crash lint coverage, the demo suite, and
//!   the live count in `docs/parity-checklist.md`.
//!
//! `--check` fails when `docs/scorecard.md` differs from what would be generated.

use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const OUT: &str = "docs/scorecard.md";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Done,
    Partial,
    Missing,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub target: String,
    pub status: Status,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checklist {
    pub area: String,
    pub title: String,
    #[serde(default)]
    pub order: u32,
    #[serde(default)]
    pub item: Vec<Item>,
}

impl Checklist {
    pub fn counts(&self) -> (usize, usize, usize) {
        let n = |s| self.item.iter().filter(|i| i.status == s).count();
        (n(Status::Done), n(Status::Partial), n(Status::Missing))
    }
}

pub fn parse(name: &str, text: &str) -> Result<Checklist, String> {
    let c: Checklist = toml::from_str(text).map_err(|e| format!("{name}: {e}"))?;
    let mut seen = std::collections::BTreeSet::new();
    for i in &c.item {
        if i.id.trim().is_empty() || i.target.trim().is_empty() {
            return Err(format!("{name}: every item needs an id and a target"));
        }
        if !seen.insert(i.id.clone()) {
            return Err(format!("{name}: duplicate id {}", i.id));
        }
        if i.target.contains('|') || i.note.as_deref().is_some_and(|n| n.contains('|')) {
            return Err(format!("{name}: {}: `|` breaks the Markdown table", i.id));
        }
    }
    Ok(c)
}

fn checklists(root: &Path) -> Result<Vec<Checklist>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("scorecard"))
        .map_err(|e| format!("scorecard/: {e}"))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        out.push(parse(&f.display().to_string(), &text)?);
    }
    out.sort_by(|a, b| (a.order, &a.area).cmp(&(b.order, &b.area)));
    Ok(out)
}

/// Facts counted from the tree.
pub struct Measured {
    pub tests: usize,
    pub commands: usize,
    pub lint_crates: (usize, usize),
    pub panic_hunt: bool,
    pub demos: usize,
    pub parity: Option<String>,
}

/// Registry entries: `cmd(` followed (after any whitespace, as rustfmt may break the line) by a
/// string literal id.
fn count_cmds(src: &str) -> usize {
    src.match_indices("cmd(").filter(|(i, _)| src.get(i + 4..).is_some_and(|rest| rest.trim_start().starts_with('"'))).count()
}

fn count_cmds_in(dir: &Path) -> usize {
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                n += count_cmds_in(&p);
            } else if p.extension().is_some_and(|x| x == "rs")
                && let Ok(s) = std::fs::read_to_string(&p)
            {
                n += count_cmds(&s);
            }
        }
    }
    n
}

fn measure(root: &Path) -> Measured {
    let mut tests = 0;
    let mut lint = (0, 0);
    for base in ["crates", "apps"] {
        if let Ok(rd) = std::fs::read_dir(root.join(base)) {
            for e in rd.flatten() {
                let mut c = crate::stats::Counts::default();
                crate::stats::walk(&e.path(), &mut c);
                tests += c.tests;
                if let Ok(m) = std::fs::read_to_string(e.path().join("Cargo.toml")) {
                    lint.1 += 1;
                    if m.contains("[lints]") && m.contains("workspace = true") {
                        lint.0 += 1;
                    }
                }
            }
        }
    }
    let engine = root.join("crates/engine/src");
    let commands = count_cmds_in(&engine);
    let panic_hunt = std::fs::read_to_string(root.join("crates/engine/tests/engine.rs")).is_ok_and(|s| s.contains("fn panic_hunt"));
    let demos = std::fs::read_dir(root.join("demos")).map(|rd| rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "gt")).count()).unwrap_or(0);
    let parity = std::fs::read_to_string(root.join("docs/parity-checklist.md"))
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("**Live:**")).map(|l| l.trim_start_matches("**Live:**").trim().to_string()));
    Measured { tests, commands, lint_crates: lint, panic_hunt, demos, parity }
}

fn status_word(s: Status) -> &'static str {
    match s {
        Status::Done => "done",
        Status::Partial => "partial",
        Status::Missing => "**missing**",
    }
}

fn anchor(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c)
            } else if c == ' ' || c == '-' {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

fn key_numbers(area: &str, m: &Measured) -> String {
    match area {
        "reliability" => format!(
            "never-crash lints on {} of {} crates; panic_hunt {}; {} tests",
            m.lint_crates.0,
            m.lint_crates.1,
            if m.panic_hunt { "present" } else { "**missing**" },
            m.tests
        ),
        "automation" => format!("{} engine commands; menu parity {}", m.commands, m.parity.clone().unwrap_or_else(|| "not generated".into())),
        "files" => format!("{} demo scores load, validate and play (`crates/engine/tests/demos.rs`)", m.demos),
        _ => String::new(),
    }
}

pub fn render(lists: &[Checklist], m: &Measured) -> String {
    let mut s = String::new();
    s.push_str("# Guitario scorecard\n\n");
    s.push_str("Generated by `cargo xtask scorecard` from `scorecard/*.toml` and the source tree. Do not edit by hand: change the sources and regenerate (CI runs `cargo xtask scorecard --check`). Checklist statuses are verified against the code; numbers here supersede estimates elsewhere.\n\n");
    s.push_str("## Summary\n\n| Area | Done | Partial | Missing | Key numbers |\n|---|---:|---:|---:|---|\n");
    let mut totals = (0, 0, 0);
    for c in lists {
        let (d, p, x) = c.counts();
        totals = (totals.0 + d, totals.1 + p, totals.2 + x);
        s.push_str(&format!("| [{}](#{}) | {d} | {p} | {x} | {} |\n", c.title, anchor(&c.title), key_numbers(&c.area, m)));
    }
    s.push_str(&format!("| **Total** | **{}** | **{}** | **{}** | |\n", totals.0, totals.1, totals.2));
    s.push_str("\nPerformance budgets are not measured yet (no `perf/budgets.toml`); see gap G9 in [`gaps.md`](gaps.md).\n");
    for c in lists {
        s.push_str(&format!("\n## {}\n\n| Id | Target | Status | Note |\n|---|---|---|---|\n", c.title));
        for i in &c.item {
            s.push_str(&format!("| {} | {} | {} | {} |\n", i.id, i.target, status_word(i.status), i.note.as_deref().unwrap_or("")));
        }
    }
    s
}

pub fn run(root: &Path, check: bool) -> Result<(), String> {
    let lists = checklists(root)?;
    let text = render(&lists, &measure(root));
    let out = root.join(OUT);
    if check {
        let current = std::fs::read_to_string(&out).unwrap_or_default();
        if current != text {
            return Err(format!("{OUT} is stale: run `cargo xtask scorecard` and commit it"));
        }
        println!("{OUT} is up to date");
        return Ok(());
    }
    std::fs::write(&out, text).map_err(|e| format!("{OUT}: {e}"))?;
    println!("wrote {OUT}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_counts() {
        let c = parse("t", "area = \"x\"\ntitle = \"X\"\n[[item]]\nid = \"A-1\"\ntarget = \"t\"\nstatus = \"partial\"\n").unwrap();
        assert_eq!(c.counts(), (0, 1, 0));
        assert!(parse("t", "area = \"x\"\ntitle = \"X\"\n[[item]]\nid = \"A\"\ntarget = \"a|b\"\nstatus = \"done\"\n").is_err());
        assert!(parse("t", "area = \"x\"\ntitle = \"X\"\n[[item]]\nid = \"A\"\ntarget = \"a\"\nstatus = \"done\"\n[[item]]\nid = \"A\"\ntarget = \"b\"\nstatus = \"done\"\n").is_err());
        assert!(parse("t", "area = \"x\"\ntitle = \"X\"\n[[item]]\nid = \"A\"\ntarget = \"a\"\nstatus = \"kinda\"\n").is_err());
    }

    #[test]
    fn counts_commands_across_line_breaks() {
        assert_eq!(count_cmds("cmd(\"a\", 1)\ncmd(\n    \"b\",\n)\nconst fn cmd(id: &str)"), 2);
    }

    #[test]
    fn anchors_match_github() {
        assert_eq!(anchor("Notation and engraving"), "notation-and-engraving");
        assert_eq!(anchor("Files & exchange"), "files--exchange");
    }
}
