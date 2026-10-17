//! Mutation fuzzing of every Guitar Pro reader on stable Rust, run with the other tests: real
//! files (the demos written as `.gp5` and `.gp`, the same XML bare and inside Guitar Pro 6
//! containers) are damaged in seeded, repeatable ways (bit flips, hostile bytes, cuts, splices,
//! repeats; in XML, numbers swapped for extremes and elements dropped or doubled) and imported. A
//! reader may refuse any of them; it may never panic, and what it accepts must validate.
//!
//! With `GUITARIO_GP_CORPUS` set, real files from that local folder are damaged too (the only
//! way the Guitar Pro 3 and 4 readers are fuzzed in depth).
//! `GUITARIO_FUZZ_CASES=20000 cargo test --release -p guitario-gp --test fuzz -- --nocapture`
//! runs deeper; `GUITARIO_FUZZ_SEED` starts elsewhere. A failing case is written to
//! `target/fuzz-failures/` and named by its seed, to replay with `GUITARIO_FUZZ_SEED`.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

/// xorshift64*: small, seeded, the same on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

fn cases() -> usize {
    std::env::var("GUITARIO_FUZZ_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(400)
}

fn first_seed() -> u64 {
    std::env::var("GUITARIO_FUZZ_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(0x5eed_6175_6974_6172)
}

/// Bytes that make readers trip: lengths of zero, sign bits, the largest values.
const HOSTILE: [u8; 8] = [0x00, 0x01, 0x7f, 0x80, 0xfe, 0xff, 0x40, 0x10];

/// One to four byte-level damages.
fn mutate_bytes(rng: &mut Rng, input: &[u8]) -> Vec<u8> {
    let mut b = input.to_vec();
    for _ in 0..=rng.below(4) {
        let len = b.len();
        match rng.below(6) {
            0 if len > 0 => {
                let i = rng.below(len);
                b[i] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let i = rng.below(len);
                b[i] = HOSTILE[rng.below(HOSTILE.len())];
            }
            // A run of 0xff or 0x00: a length or count field gone huge or empty.
            2 if len > 4 => {
                let i = rng.below(len - 4);
                let v = if rng.below(2) == 0 { 0xff } else { 0x00 };
                b[i..i + 4].fill(v);
            }
            3 => b.truncate(rng.below(len + 1)),
            // A slice of the file repeated elsewhere.
            4 if len > 1 => {
                let (from, to) = (rng.below(len), rng.below(len));
                let n = rng.below(64.min(len - from)) + 1;
                let chunk = b[from..from + n].to_vec();
                b.splice(to..to, chunk);
            }
            // A slice removed.
            _ if len > 1 => {
                let from = rng.below(len);
                let n = rng.below(64.min(len - from)) + 1;
                b.drain(from..from + n);
            }
            _ => {}
        }
    }
    b
}

/// XML damaged as text, so the parser (not the ZIP checksum) sees it.
fn mutate_xml(rng: &mut Rng, xml: &str) -> String {
    const EXTREMES: [&str; 9] = ["-1", "0", "4294967295", "-2147483648", "99999999999999999999", "NaN", "1e308", "", "3.5"];
    let mut s = xml.to_string();
    for _ in 0..=rng.below(3) {
        match rng.below(4) {
            // A number replaced by an extreme.
            0 => {
                let digits: Vec<usize> =
                    s.char_indices().filter(|(i, c)| c.is_ascii_digit() && (*i == 0 || !s.as_bytes()[i - 1].is_ascii_digit())).map(|(i, _)| i).collect();
                if let Some(&at) = digits.get(rng.below(digits.len())) {
                    let end = s[at..].find(|c: char| !c.is_ascii_digit()).map_or(s.len(), |n| at + n);
                    s.replace_range(at..end, EXTREMES[rng.below(EXTREMES.len())]);
                }
            }
            // An element dropped, or doubled.
            1 | 2 => {
                let opens: Vec<usize> = s.match_indices('<').map(|(i, _)| i).filter(|i| s[*i + 1..].starts_with(|c: char| c.is_ascii_alphabetic())).collect();
                if let Some(&at) = opens.get(rng.below(opens.len())) {
                    let name: String = s[at + 1..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                    let close = format!("</{name}>");
                    if let Some(end) = s[at..].find(&close).map(|n| at + n + close.len()) {
                        let element = s[at..end].to_string();
                        if rng.below(2) == 0 {
                            s.replace_range(at..end, "");
                        } else {
                            s.insert_str(end, &element);
                        }
                    }
                }
            }
            // A cut anywhere, on a character boundary.
            _ => {
                let mut at = rng.below(s.len() + 1);
                while !s.is_char_boundary(at) {
                    at -= 1;
                }
                s.truncate(at);
            }
        }
    }
    s
}

/// Imports one damaged input: refusing is fine, panicking or accepting an invalid score is not.
/// Returns whether it was accepted.
fn check(kind: &str, seed: u64, input: &[u8], import: &dyn Fn(&[u8]) -> Result<guitario_gp::Import, guitario_gp::GpError>) -> bool {
    let outcome = catch_unwind(AssertUnwindSafe(|| import(input)));
    let accepted = matches!(outcome, Ok(Ok(_)));
    let problem = match outcome {
        Err(_) => Some("panicked".to_string()),
        Ok(Ok(imp)) => imp.score.validate().err().map(|e| format!("accepted an invalid score: {e}")),
        Ok(Err(_)) => None,
    };
    if let Some(problem) = problem {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fuzz-failures");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join(format!("{kind}-{seed}.bin"));
        let _ = std::fs::write(&file, input);
        panic!("{kind} reader {problem} on case seed {seed} (input saved to {})", file.display());
    }
    accepted
}

/// Damage that is always refused never reaches the reader's depths: some must get through.
fn report(kind: &str, accepted: usize, total: usize, at_least: f64) {
    let share = accepted as f64 / total.max(1) as f64;
    println!("{kind}: {accepted} of {total} damaged inputs imported ({:.1}%)", share * 100.0);
    assert!(total < 100 || share >= at_least, "{kind}: only {accepted} of {total} damaged inputs got through the reader");
}

/// Runs `cases()` damaged copies of the seeds through `import`.
fn fuzz(
    kind: &str,
    seeds: &[Vec<u8>],
    mutate: &dyn Fn(&mut Rng, &[u8]) -> Vec<u8>,
    import: &dyn Fn(&[u8]) -> Result<guitario_gp::Import, guitario_gp::GpError>,
) -> usize {
    assert!(!seeds.is_empty());
    // The seeds themselves import.
    for s in seeds {
        assert!(import(s).is_ok(), "{kind}: a seed does not import");
    }
    let start = first_seed();
    let mut accepted = 0;
    for case in 0..cases() as u64 {
        let seed = start.wrapping_add(case.wrapping_mul(0x9E37_79B9_7F4A_7C15)) | 1;
        let mut rng = Rng(seed);
        let pick = rng.below(seeds.len());
        let input = mutate(&mut rng, &seeds[pick]);
        accepted += usize::from(check(kind, seed, &input, import));
    }
    accepted
}

/// Up to 64 files with these extensions from the local corpus `GUITARIO_GP_CORPUS` names, those
/// that import; none when it is unset.
fn corpus(extensions: &[&str]) -> Vec<Vec<u8>> {
    let Some(dir) = std::env::var_os("GUITARIO_GP_CORPUS") else { return vec![] };
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.contains(&e.to_ascii_lowercase().as_str())))
        .collect();
    paths.sort();
    paths.iter().filter_map(|p| std::fs::read(p).ok()).filter(|b| guitario_gp::import(b).is_ok()).take(64).collect()
}

fn gp5_seeds() -> Vec<Vec<u8>> {
    let mut seeds: Vec<Vec<u8>> = common::demos().iter().map(|(_, s)| guitario_gp::export_gp5(s).bytes).collect();
    seeds.extend(corpus(&["gp5"]));
    seeds
}

fn gpif_seeds() -> Vec<String> {
    let mut out = vec![];
    for (_, s) in common::demos().iter().take(6) {
        let bytes = guitario_gp::export_gp(s).unwrap().bytes;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("Content/score.gpif").unwrap(), &mut xml).unwrap();
        out.push(xml);
    }
    out
}

/// A Guitar Pro 6 file system (BCFS) holding `score.gpif`: a header sector, the file's entry, then
/// its data sectors.
fn bcfs(content: &[u8]) -> Vec<u8> {
    const SECTOR: usize = 0x1000;
    let sectors = content.len().div_ceil(SECTOR);
    let mut entry = vec![0u8; SECTOR];
    entry[..4].copy_from_slice(&2u32.to_le_bytes());
    entry[4..4 + 10].copy_from_slice(b"score.gpif");
    entry[0x8C..0x90].copy_from_slice(&(content.len() as u32).to_le_bytes());
    for i in 0..sectors {
        let at = 0x94 + 4 * i;
        entry[at..at + 4].copy_from_slice(&((i + 2) as u32).to_le_bytes());
    }
    let mut fs = b"BCFS".to_vec();
    fs.extend(vec![0u8; SECTOR]);
    fs.extend(entry);
    let mut data = content.to_vec();
    data.resize(sectors * SECTOR, 0);
    fs.extend(data);
    fs
}

/// The same, compressed as BCFZ with literal chunks only.
fn bcfz(fs: &[u8]) -> Vec<u8> {
    let mut bits: Vec<u8> = Vec::new();
    for chunk in fs.chunks(3) {
        bits.push(0);
        let n = chunk.len();
        bits.extend([(n & 1) as u8, ((n >> 1) & 1) as u8]);
        for byte in chunk {
            bits.extend((0..8).rev().map(|i| (byte >> i) & 1));
        }
    }
    let mut out = b"BCFZ".to_vec();
    out.extend((fs.len() as i32).to_le_bytes());
    out.extend(bits.chunks(8).map(|c| c.iter().enumerate().fold(0u8, |acc, (i, b)| acc | (b << (7 - i)))));
    out
}

#[test]
fn guitar_pro_5_reader_survives_damage() {
    let n = fuzz("gp5", &gp5_seeds(), &mutate_bytes, &|b| guitario_gp::import(b));
    report("gp5", n, cases(), 0.01);
}

#[test]
fn guitar_pro_3_and_4_survive_damage() {
    // Real Guitar Pro 3 and 4 files, when a local corpus is given (none can be committed and there
    // is no writer for them). Without one, the same bodies under the older banners still put the
    // version checks and the start of each reader through damage, but not the depths.
    let real = corpus(&["gp3", "gp4"]);
    let seeds: Vec<Vec<u8>> = if real.is_empty() {
        gp5_seeds()
            .into_iter()
            .take(4)
            .flat_map(|b| {
                ["FICHIER GUITAR PRO v3.00", "FICHIER GUITAR PRO v4.06", "FICHIER GUITAR PRO v5.00"].map(|banner| {
                    let mut v = vec![banner.len() as u8];
                    v.extend(banner.as_bytes());
                    v.resize(31, 0);
                    v.extend(&b[31..]);
                    v
                })
            })
            .collect()
    } else {
        real.clone()
    };
    let start = first_seed();
    let mut accepted = 0;
    for case in 0..cases() as u64 {
        let seed = start.wrapping_add(case.wrapping_mul(0xD1B5_4A32_D192_ED03)) | 1;
        let mut rng = Rng(seed);
        let pick = rng.below(seeds.len());
        let input = mutate_bytes(&mut rng, &seeds[pick]);
        accepted += usize::from(check("gp3-4", seed, &input, &|b| guitario_gp::import(b)));
    }
    report("gp3-4", accepted, cases(), if real.is_empty() { 0.0 } else { 0.01 });
}

#[test]
fn guitar_pro_7_xml_reader_survives_damage() {
    let seeds = gpif_seeds();
    let start = first_seed();
    for (k, s) in seeds.iter().enumerate() {
        assert!(guitario_gp::parse_gpif(s).is_ok(), "gpif seed {k}");
    }
    let mut accepted = 0;
    for case in 0..cases() as u64 {
        let seed = start.wrapping_add(case.wrapping_mul(0xA24B_AED4_963E_E407)) | 1;
        let mut rng = Rng(seed);
        let pick = rng.below(seeds.len());
        let xml = mutate_xml(&mut rng, &seeds[pick]);
        accepted += usize::from(check("gpif", seed, xml.as_bytes(), &|b| guitario_gp::parse_gpif(std::str::from_utf8(b).unwrap_or_default())));
    }
    report("gpif", accepted, cases(), 0.05);
}

#[test]
fn guitar_pro_7_archive_survives_damage() {
    let mut seeds: Vec<Vec<u8>> = common::demos().iter().take(4).map(|(_, s)| guitario_gp::export_gp(s).unwrap().bytes).collect();
    seeds.extend(corpus(&["gp"]));
    let n = fuzz("gp", &seeds, &mutate_bytes, &|b| guitario_gp::import(b));
    report("gp", n, cases(), 0.0);
}

#[test]
fn guitar_pro_6_containers_survive_damage() {
    let xml = gpif_seeds();
    let mut seeds: Vec<Vec<u8>> = xml.iter().take(3).flat_map(|x| [bcfs(x.as_bytes()), bcfz(&bcfs(x.as_bytes()))]).collect();
    seeds.extend(corpus(&["gpx"]));
    let n = fuzz("gpx", &seeds, &mutate_bytes, &|b| guitario_gp::import(b));
    report("gpx", n, cases(), 0.01);
}

#[test]
#[should_panic(expected = "reader panicked on case seed")]
fn a_reader_that_panics_is_caught_and_named() {
    // The harness itself: a reader with a planted bug is reported, with the seed to replay it.
    let seeds = vec![b"FICHIER".to_vec()];
    fuzz("planted", &seeds, &mutate_bytes, &|b| {
        if b.len() == 7 {
            guitario_gp::import(b).or(Ok(guitario_gp::Import { score: guitario_score::Score::new(), warnings: vec![] }))
        } else {
            panic!("planted bug")
        }
    });
}
