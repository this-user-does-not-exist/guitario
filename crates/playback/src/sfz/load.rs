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
        let load = |rel: &str| {
            let p = confined(base, rel).ok_or_else(|| SfzError::Io(format!("{rel}: path leaves the library")))?;
            Sample::decode(&read_limited(&p, MAX_SAMPLE_BYTES)?, rel)
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
}
