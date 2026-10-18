//! Loading an `.sfz` file and its samples from disk (not available on wasm).

use super::instrument::Instrument;
use super::sample::{Sample, SfzError};
use std::path::{Component, Path, PathBuf};

const MAX_SFZ_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SAMPLE_BYTES: u64 = 256 * 1024 * 1024;

/// Joins `rel` onto `base`, refusing absolute paths and paths that climb more than
/// [`LIBRARY_ASCENT`] folders above `base`, so an SFZ file cannot read arbitrary files.
fn confined(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut depth = 0i32;
    let mut out = base.to_path_buf();
    for c in Path::new(&rel.replace('\\', "/")).components() {
        match c {
            Component::Normal(p) => {
                depth += 1;
                out.push(p);
            }
            Component::ParentDir => {
                depth -= 1;
                if depth < -LIBRARY_ASCENT {
                    return None;
                }
                out.pop();
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// Libraries keep programs in `Programs/` and audio in `Samples/` next to it, so a program may
/// climb out of its own folder (and one more for nested maps), but no further.
const LIBRARY_ASCENT: i32 = 2;

fn read_limited(path: &Path, max: u64) -> Result<Vec<u8>, SfzError> {
    let meta = std::fs::metadata(path).map_err(|e| SfzError::Io(format!("{}: {e}", path.display())))?;
    if meta.len() > max {
        return Err(SfzError::Io(format!("{} is too large", path.display())));
    }
    std::fs::read(path).map_err(|e| SfzError::Io(format!("{}: {e}", path.display())))
}

impl Instrument {
    /// Loads `path` (an `.sfz` file). With `keys`, decodes only the samples those keys need.
    pub fn load(path: &Path, keys: Option<&[u8]>) -> Result<Instrument, SfzError> {
        Instrument::load_switched(path, keys, None)
    }

    /// [`load`](Self::load) with `switch` as the active keyswitch.
    pub fn load_switched(path: &Path, keys: Option<&[u8]>, switch: Option<u8>) -> Result<Instrument, SfzError> {
        let base = path.parent().unwrap_or(Path::new("."));
        let text = String::from_utf8_lossy(&read_limited(path, MAX_SFZ_BYTES)?).into_owned();
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("instrument").to_string();
        let mut include = |rel: &str| {
            let p = confined(base, rel)?;
            read_limited(&p, MAX_SFZ_BYTES).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
        };
        // A long sample keeps its first second in memory; the rest is decoded from the file again
        // when a note reaches it (`stream.rs`).
        let load = |rel: &str| {
            let p = confined(base, rel).ok_or_else(|| SfzError::Io(format!("{rel}: path leaves the library")))?;
            let sample = Sample::decode(&read_limited(&p, MAX_SAMPLE_BYTES)?, rel)?;
            let name = rel.to_string();
            Ok(sample.streamed(Box::new(move || Sample::decode(&read_limited(&p, MAX_SAMPLE_BYTES)?, &name))))
        };
        Instrument::build_switched(&name, &text, &mut include, &load, keys, switch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_cannot_escape() {
        let b = Path::new("/lib/Programs");
        assert_eq!(confined(b, "../Samples/a.wav"), Some(PathBuf::from("/lib/Samples/a.wav")));
        assert!(confined(b, "/etc/passwd").is_none());
        assert!(confined(b, "../../../../../etc/passwd").is_none());
        assert!(confined(b, "a/../../b").is_some());
        assert!(confined(b, "../../../x").is_none());
    }

    #[test]
    fn loads_a_library_from_disk() {
        let dir = std::env::temp_dir().join(format!("gc-sfz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Programs")).unwrap();
        std::fs::create_dir_all(dir.join("Samples")).unwrap();
        let spec = hound::WavSpec { channels: 1, sample_rate: 8000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(dir.join("Samples/a.wav"), spec).unwrap();
        for n in 0..800 {
            w.write_sample((n % 100) as i16 * 100).unwrap();
        }
        w.finalize().unwrap();
        std::fs::write(dir.join("Programs/k.sfz"), "<control> default_path=../Samples/\n#include \"r.sfz\"").unwrap();
        std::fs::write(dir.join("Programs/r.sfz"), "<region> sample=a.wav key=40\n<region> sample=missing.wav key=41").unwrap();
        let i = Instrument::load(&dir.join("Programs/k.sfz"), None).unwrap();
        assert_eq!(i.region_count(), 1);
        assert_eq!(i.warnings.len(), 1);
        assert!(Instrument::load(&dir.join("Programs/none.sfz"), None).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_long_sample_keeps_its_first_second_and_reads_the_rest_from_its_file() {
        let dir = std::env::temp_dir().join(format!("gc-sfz-stream-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let spec = hound::WavSpec { channels: 1, sample_rate: 8000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(dir.join("long.wav"), spec).unwrap();
        for n in 0..24_000i32 {
            w.write_sample((n % 2000) as i16 * 10).unwrap();
        }
        w.finalize().unwrap();
        std::fs::write(dir.join("k.sfz"), "<region> sample=long.wav key=60").unwrap();
        let i = Instrument::load(&dir.join("k.sfz"), None).unwrap();
        assert_eq!(i.memory(&mut Default::default()), 8000 * 2, "only the first second is decoded into memory");
        let hits = i.select(60, 100, 0, 0.5, false);
        let sample = hits.first().unwrap().sample;
        assert_eq!(sample.frames(), 24_000);
        let stream = sample.stream.clone().unwrap();
        let tail = stream.tail().unwrap();
        assert!(tail.get(0).0 == 0.0 && (tail.get(1).0 - 10.0 / 32_768.0).abs() < 1e-6, "frames 8000 and 8001 of the file");
        assert!((tail.get(15_999).0 - 1999.0 * 10.0 / 32_768.0).abs() < 1e-4);
        // A file that disappears after loading plays its head; the tail fails once, not every note.
        std::fs::remove_dir_all(&dir).unwrap();
        let i = Instrument::build(
            "x",
            "<region> sample=a.wav key=60",
            &mut |_| None,
            &|_| {
                let x = vec![0.25f32; 24_000];
                let path = std::env::temp_dir().join("gc-sfz-stream-missing/a.wav");
                Ok(Sample::from_f32(8000, &x, None).streamed(Box::new(move || Sample::decode(&read_limited(&path, MAX_SAMPLE_BYTES)?, "a.wav"))))
            },
            None,
        )
        .unwrap();
        let hits = i.select(60, 100, 0, 0.5, false);
        let stream = hits.first().unwrap().sample.stream.clone().unwrap();
        assert!(stream.tail().is_none() && stream.failed());
    }
}
