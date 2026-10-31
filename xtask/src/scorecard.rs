//! `cargo xtask scorecard [--check]`: generate `docs/scorecard.md`.
//!
//! Sources, all committed, so the output is deterministic and CI can check it:
//! - `scorecard/*.toml`: per-area checklists (id, target, status, note). An item is `done` when it
//!   has a command, a UI and tests on `main`.
//! - counted from the tree: tests, engine commands, never-crash lint coverage, the demo suite, and
//!   the live count in `docs/parity-checklist.md`.
//!
//! `--check` fails when `docs/scorecard.md` differs from what would be generated. Both modes fail
//! when a summary that quotes the menu count ([`SUMMARIES`]) disagrees with the checklist.

use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const OUT: &str = "docs/scorecard.md";
/// Hand-written pages that quote the live menu count.
const SUMMARIES: [&str; 2] = ["README.md", "ROADMAP.md"];

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Done,
    Partial,
    Missing,
}

/// The release an unfinished item is needed for (see `docs/roadmap.md`, "Stages").
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Stage {
    #[serde(rename = "beta")]
    Beta,
    #[serde(rename = "1.0")]
    V1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub target: String,
    pub status: Status,
    pub stage: Option<Stage>,
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

    /// Items not yet done that a stage needs: (beta, 1.0).
    pub fn open_by_stage(&self) -> (usize, usize) {
        let n = |g| self.item.iter().filter(|i| i.status != Status::Done && i.stage == Some(g)).count();
        (n(Stage::Beta), n(Stage::V1))
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
        if i.status != Status::Done && i.stage.is_none() {
            return Err(format!("{name}: {} is not done, so it needs a `stage` (\"beta\" or \"1.0\")", i.id));
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

fn stage_word(s: Option<Stage>) -> &'static str {
    match s {
        Some(Stage::Beta) => "beta",
        Some(Stage::V1) => "1.0",
        None => "",
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
    s.push_str("## Summary\n\n| Area | Done | Partial | Missing | Open for beta | Open for 1.0 | Key numbers |\n|---|---:|---:|---:|---:|---:|---|\n");
    let mut totals = (0, 0, 0, 0, 0);
    for c in lists {
        let (d, p, x) = c.counts();
        let (b, v) = c.open_by_stage();
        totals = (totals.0 + d, totals.1 + p, totals.2 + x, totals.3 + b, totals.4 + v);
        s.push_str(&format!("| [{}](#{}) | {d} | {p} | {x} | {b} | {v} | {} |\n", c.title, anchor(&c.title), key_numbers(&c.area, m)));
    }
    s.push_str(&format!("| **Total** | **{}** | **{}** | **{}** | **{}** | **{}** | |\n", totals.0, totals.1, totals.2, totals.3, totals.4));
    s.push_str("\n\"Open for beta\" counts the partial and missing items the [beta gate](roadmap.md#beta-gate) needs; \"open for 1.0\" those that can wait for full parity. Every item that is not done carries a stage.\n");
    s.push_str("\nPerformance is measured by `cargo xtask perf` against `perf/budgets.toml`; see gap G9 in [`gaps.md`](gaps.md).\n");
    for c in lists {
        s.push_str(&format!("\n## {}\n\n| Id | Target | Status | Stage | Note |\n|---|---|---|---|---|\n", c.title));
        for i in &c.item {
            s.push_str(&format!("| {} | {} | {} | {} | {} |\n", i.id, i.target, status_word(i.status), stage_word(i.stage), i.note.as_deref().unwrap_or("")));
        }
    }
    s
}

/// Every "N of M" on a line that mentions menus.
fn menu_counts(text: &str) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for line in text.lines().filter(|l| l.to_ascii_lowercase().contains("menu")) {
        let words: Vec<&str> = line.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
        for w in words.windows(3) {
            if let [n, "of", m] = w
                && let (Ok(n), Ok(m)) = (n.parse(), m.parse())
            {
                out.push((n, m));
            }
        }
    }
    out
}

/// The summaries whose menu count differs from `live` (the checklist's "N of M menu items").
fn stale_summaries(root: &Path, live: &str) -> Vec<String> {
    let Some(&(n, m)) = menu_counts(live).first() else { return vec![format!("docs/parity-checklist.md: no live count in {live:?}")] };
    let mut stale = Vec::new();
    for page in SUMMARIES {
        let text = std::fs::read_to_string(root.join(page)).unwrap_or_default();
        for (a, b) in menu_counts(&text).into_iter().filter(|&c| c != (n, m)) {
            stale.push(format!("{page} says {a} of {b} menu items live; docs/parity-checklist.md says {n} of {m}"));
        }
    }
    stale
}

pub fn run(root: &Path, check: bool) -> Result<(), String> {
    let lists = checklists(root)?;
    let measured = measure(root);
    let text = render(&lists, &measured);
    let out = root.join(OUT);
    let stale = stale_summaries(root, measured.parity.as_deref().unwrap_or(""));
    if !stale.is_empty() {
        return Err(stale.join("\n"));
    }
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
        let c = parse("t", "area = \"x\"\ntitle = \"X\"\n[[item]]\nid = \"A-1\"\ntarget = \"t\"\nstatus = \"partial\"\nstage = \"beta\"\n").unwrap();
        assert_eq!(c.counts(), (0, 1, 0));
        assert_eq!(c.open_by_stage(), (1, 0));
        // an unfinished item must say which release needs it
        assert!(parse("t", "area = \"x\"\ntitle = \"X\"\n[[item]]\nid = \"A\"\ntarget = \"a\"\nstatus = \"missing\"\n").is_err());
        assert!(parse("t", "area = \"x\"\ntitle = \"X\"\n[[item]]\nid = \"A\"\ntarget = \"a\"\nstatus = \"missing\"\nstage = \"1.0\"\n").is_ok());
        assert!(parse("t", "area = \"x\"\ntitle = \"X\"\n[[item]]\nid = \"A\"\ntarget = \"a|b\"\nstatus = \"done\"\n").is_err());
        assert!(parse("t", "area = \"x\"\ntitle = \"X\"\n[[item]]\nid = \"A\"\ntarget = \"a\"\nstatus = \"done\"\n[[item]]\nid = \"A\"\ntarget = \"b\"\nstatus = \"done\"\n").is_err());
        assert!(parse("t", "area = \"x\"\ntitle = \"X\"\n[[item]]\nid = \"A\"\ntarget = \"a\"\nstatus = \"kinda\"\n").is_err());
    }

    #[test]
    fn counts_commands_across_line_breaks() {
        assert_eq!(count_cmds("cmd(\"a\", 1)\ncmd(\n    \"b\",\n)\nconst fn cmd(id: &str)"), 2);
    }

    #[test]
    fn finds_menu_counts_only_on_menu_lines() {
        let page = "| **Menus** | 172 of 180 Guitar Pro menu items live |\n0 of 8 workflows\n([menu parity](x.md): 161 of 177 live)";
        assert_eq!(menu_counts(page), vec![(172, 180), (161, 177)]);
        assert_eq!(menu_counts("173 of 181 menu items (96%)"), vec![(173, 181)]);
    }

    #[test]
    fn anchors_match_github() {
        assert_eq!(anchor("Notation and engraving"), "notation-and-engraving");
        assert_eq!(anchor("Files & exchange"), "files--exchange");
    }
}
