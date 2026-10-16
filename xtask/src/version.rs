//! `cargo xtask version [set X.Y.Z[-pre]]`: the workspace version, the single source of truth in
//! `[workspace.package] version`; every crate inherits it.

use std::path::Path;

fn find_line(manifest: &str) -> Result<usize, String> {
    let mut in_pkg = false;
    for (i, line) in manifest.lines().enumerate() {
        let t = line.trim();
        if t.starts_with('[') {
            in_pkg = t == "[workspace.package]";
        } else if in_pkg && t.starts_with("version") && t.contains('=') {
            return Ok(i);
        }
    }
    Err("no version in [workspace.package]".into())
}

pub fn read(manifest: &str) -> Result<String, String> {
    let i = find_line(manifest)?;
    let line = manifest.lines().nth(i).unwrap_or_default();
    let v = line.split_once('=').map(|(_, v)| v.trim().trim_matches('"').to_string()).unwrap_or_default();
    if v.is_empty() { Err("empty [workspace.package] version".into()) } else { Ok(v) }
}

pub fn validate(v: &str) -> Result<(), String> {
    let bad = || Err(format!("`{v}` is not a version like 1.2.3 or 1.2.3-rc.1"));
    let core = v.split_once('-').map_or(v, |(c, _)| c);
    let nums: Vec<&str> = core.split('.').collect();
    if nums.len() != 3 || nums.iter().any(|n| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) || (n.len() > 1 && n.starts_with('0'))) {
        return bad();
    }
    Ok(())
}

pub fn replace(manifest: &str, new: &str) -> Result<String, String> {
    validate(new)?;
    let idx = find_line(manifest)?;
    Ok(manifest
        .split_inclusive('\n')
        .enumerate()
        .map(|(i, l)| if i == idx { format!("version = \"{new}\"{}", if l.ends_with('\n') { "\n" } else { "" }) } else { l.to_string() })
        .collect())
}

pub fn run(root: &Path, rest: &[&str]) -> Result<(), String> {
    let path = root.join("Cargo.toml");
    let manifest = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    match rest {
        [] => {
            println!("{}", read(&manifest)?);
            Ok(())
        }
        ["set", v] => {
            std::fs::write(&path, replace(&manifest, v)?).map_err(|e| e.to_string())?;
            let status = crate::cargo().args(["update", "--workspace"]).status().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err("cargo update --workspace failed".into());
            }
            println!("version {v}");
            Ok(())
        }
        _ => Err("usage: cargo xtask version [set X.Y.Z[-pre]]".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &str = "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n";

    #[test]
    fn read_and_replace() {
        assert_eq!(read(M).unwrap(), "0.1.0");
        let n = replace(M, "0.2.0-rc.1").unwrap();
        assert_eq!(read(&n).unwrap(), "0.2.0-rc.1");
        assert!(n.contains("edition = \"2024\""));
        assert!(replace(M, "1.2").is_err());
        assert!(replace(M, "01.2.3").is_err());
    }
}
