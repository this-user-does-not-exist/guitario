//! `cargo xtask stats`: Rust files, lines and `#[test]`s per crate.

use std::path::Path;

#[derive(Default, Clone, Copy)]
pub struct Counts {
    pub files: usize,
    pub lines: usize,
    pub tests: usize,
}

pub fn walk(dir: &Path, c: &mut Counts) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if name != "target" && !name.starts_with('.') {
                walk(&p, c);
            }
        } else if name.ends_with(".rs")
            && let Ok(s) = std::fs::read_to_string(&p)
        {
            c.files += 1;
            c.lines += s.lines().count();
            c.tests += s.lines().filter(|l| !l.trim_start().starts_with("//")).map(|l| l.matches("#[test]").count()).sum::<usize>();
        }
    }
}

/// Counts for every workspace package, sorted by name.
pub fn collect() -> Result<Vec<(String, Counts)>, String> {
    let meta = crate::metadata()?;
    let mut rows = Vec::new();
    for p in meta.get("packages").and_then(serde_json::Value::as_array).into_iter().flatten() {
        let name = p.get("name").and_then(serde_json::Value::as_str).unwrap_or("?").to_owned();
        let Some(dir) = p.get("manifest_path").and_then(serde_json::Value::as_str).and_then(|m| Path::new(m).parent().map(Path::to_path_buf)) else { continue };
        let mut c = Counts::default();
        walk(&dir, &mut c);
        rows.push((name, c));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(rows)
}

pub fn run(_root: &Path) -> Result<(), String> {
    let rows = collect()?;
    println!("{:<24} {:>6} {:>8} {:>7}", "crate", "files", "lines", "tests");
    let mut total = Counts::default();
    for (name, c) in &rows {
        println!("{name:<24} {:>6} {:>8} {:>7}", c.files, c.lines, c.tests);
        total.files += c.files;
        total.lines += c.lines;
        total.tests += c.tests;
    }
    println!("{:<24} {:>6} {:>8} {:>7}", "total", total.files, total.lines, total.tests);
    Ok(())
}
