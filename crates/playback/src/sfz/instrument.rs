//! A sampled instrument: parsed regions plus the decoded audio they play.

use super::parse::{Region, parse};
use super::sample::{Sample, SfzError};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Instrument {
    pub name: String,
    regions: Vec<(Region, Arc<Sample>)>,
    pub warnings: Vec<String>,
}

/// Which regions can sound for any of `keys`: those whose range holds a key, or, for a key no
/// region covers, those nearest to it.
fn needed_regions(regions: &[Region], keys: &[u8]) -> Vec<bool> {
    let distance = |r: &Region, k: u8| if k < r.lokey { r.lokey - k } else { k.saturating_sub(r.hikey) };
    let mut needed = vec![false; regions.len()];
    for &k in keys {
        let nearest = regions.iter().map(|r| distance(r, k)).min();
        for (flag, r) in needed.iter_mut().zip(regions) {
            if Some(distance(r, k)) == nearest {
                *flag = true;
            }
        }
    }
    needed
}

/// What a note-on should start.
pub struct Hit<'a> {
    pub region: &'a Region,
    pub sample: &'a Arc<Sample>,
}

impl Instrument {
    /// Builds an instrument from SFZ `text`. `include` resolves `#include`s and `load` returns the
    /// decoded audio for a sample path. With `keys` set, only regions that can sound for one of
    /// those keys are loaded (a score rarely uses a whole library, so it should not pay for all of
    /// its samples); a key outside every range counts as its nearest region. Regions whose audio is
    /// missing or undecodable are dropped with a warning. Fails only if nothing is left.
    pub fn build(
        name: &str,
        text: &str,
        include: &mut dyn FnMut(&str) -> Option<String>,
        load: &mut dyn FnMut(&str) -> Result<Sample, SfzError>,
        keys: Option<&[u8]>,
    ) -> Result<Instrument, SfzError> {
        let parsed = parse(text, include);
        let mut warnings = parsed.warnings;
        let mut cache: HashMap<String, Option<Arc<Sample>>> = HashMap::new();
        let mut regions = Vec::new();
        let needed = keys.map(|ks| needed_regions(&parsed.regions, ks));
        for (i, r) in parsed.regions.into_iter().enumerate() {
            if needed.as_ref().is_some_and(|n| !n.get(i).copied().unwrap_or(false)) {
                continue;
            }
            let sample = cache
                .entry(r.sample.clone())
                .or_insert_with(|| match load(&r.sample) {
                    Ok(s) => Some(Arc::new(s)),
                    Err(e) => {
                        warnings.push(format!("{}: {e}", r.sample));
                        None
                    }
                })
                .clone();
            if let Some(s) = sample {
                regions.push((r, s));
            }
        }
        if regions.is_empty() {
            return Err(SfzError::Empty(name.to_string()));
        }
        Ok(Instrument { name: name.to_string(), regions, warnings })
    }

    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Regions that sound for `key` at `velocity` (0..=127). `seq` is the instrument's note counter
    /// (round robin) and `rand` a uniform 0..1 draw. A key outside every range borrows the nearest
    /// region and is transposed by the voice.
    pub fn select(&self, key: u8, velocity: u8, seq: u32, rand: f32) -> Vec<Hit<'_>> {
        let by_vel = |r: &Region| velocity >= r.lovel && velocity <= r.hivel && rand >= r.lorand && (rand < r.hirand || r.hirand >= 1.0);
        let round_robin = |r: &Region| r.seq_length <= 1 || seq % r.seq_length + 1 == r.seq_position;
        let hits = |range: &dyn Fn(&Region) -> bool| -> Vec<Hit<'_>> {
            self.regions.iter().filter(|(r, _)| range(r) && by_vel(r) && round_robin(r)).map(|(r, s)| Hit { region: r, sample: s }).collect()
        };
        let exact = hits(&|r| key >= r.lokey && key <= r.hikey);
        if !exact.is_empty() {
            return exact;
        }
        let distance = |r: &Region| if key < r.lokey { r.lokey - key } else { key.saturating_sub(r.hikey) };
        let nearest = self.regions.iter().filter(|(r, _)| by_vel(r)).map(|(r, _)| distance(r)).min();
        match nearest {
            Some(d) => hits(&|r| distance(r) == d),
            None => Vec::new(),
        }
    }

    /// The (lowest, highest) key any region covers.
    pub fn key_range(&self) -> (u8, u8) {
        let lo = self.regions.iter().map(|(r, _)| r.lokey).min().unwrap_or(0);
        let hi = self.regions.iter().map(|(r, _)| r.hikey).max().unwrap_or(127);
        (lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32) -> Sample {
        Sample { rate, left: vec![0.5; 100], right: None }
    }

    fn inst(text: &str) -> Instrument {
        Instrument::build("t", text, &mut |_| None, &mut |_| Ok(tone(44_100)), None).unwrap()
    }

    #[test]
    fn selects_by_key_velocity_and_round_robin() {
        let i = inst(
            "<region> sample=a.wav lokey=40 hikey=50 hivel=60\n<region> sample=b.wav lokey=40 hikey=50 lovel=61\n<region> sample=c.wav key=60 seq_length=2 seq_position=1\n<region> sample=d.wav key=60 seq_length=2 seq_position=2",
        );
        assert_eq!(i.select(45, 30, 0, 0.5).first().map(|h| h.region.sample.as_str()), Some("a.wav"));
        assert_eq!(i.select(45, 100, 0, 0.5).first().map(|h| h.region.sample.as_str()), Some("b.wav"));
        assert_eq!(i.select(60, 100, 0, 0.5).first().map(|h| h.region.sample.as_str()), Some("c.wav"));
        assert_eq!(i.select(60, 100, 1, 0.5).first().map(|h| h.region.sample.as_str()), Some("d.wav"));
        assert_eq!(i.key_range(), (40, 60));
    }

    #[test]
    fn out_of_range_borrows_the_nearest_region() {
        let i = inst("<region> sample=a.wav lokey=40 hikey=50\n<region> sample=b.wav lokey=60 hikey=70");
        assert_eq!(i.select(30, 100, 0, 0.5).first().map(|h| h.region.sample.as_str()), Some("a.wav"));
        assert_eq!(i.select(120, 100, 0, 0.5).first().map(|h| h.region.sample.as_str()), Some("b.wav"));
    }

    #[test]
    fn missing_samples_warn_and_empty_fails() {
        let i = Instrument::build(
            "t",
            "<region> sample=a.wav key=1\n<region> sample=b.wav key=2",
            &mut |_| None,
            &mut |n| if n == "a.wav" { Ok(tone(8000)) } else { Err(SfzError::Io("gone".into())) },
            None,
        )
        .unwrap();
        assert_eq!(i.region_count(), 1);
        assert_eq!(i.warnings.len(), 1);
        assert!(Instrument::build("t", "<region> sample=b.wav key=2", &mut |_| None, &mut |_| Err(SfzError::Io("x".into())), None).is_err());
        assert!(Instrument::build("t", "", &mut |_| None, &mut |_| Ok(tone(8000)), None).is_err());
    }

    #[test]
    fn out_of_range_keys_load_the_nearest_region() {
        let mut loaded = Vec::new();
        let text = "<region> sample=a.wav lokey=40 hikey=50\n<region> sample=b.wav lokey=60 hikey=70";
        let mut load = |n: &str| {
            loaded.push(n.to_string());
            Ok(tone(8000))
        };
        let i = Instrument::build("t", text, &mut |_| None, &mut load, Some(&[30])).unwrap();
        assert_eq!(i.select(30, 100, 0, 0.5).len(), 1);
        assert_eq!(loaded, ["a.wav"]);
    }

    #[test]
    fn unwanted_regions_are_not_loaded() {
        let mut loaded = Vec::new();
        let i = Instrument::build(
            "t",
            "<region> sample=a.wav key=1\n<region> sample=b.wav key=2",
            &mut |_| None,
            &mut |n| {
                loaded.push(n.to_string());
                Ok(tone(8000))
            },
            Some(&[2]),
        )
        .unwrap();
        assert_eq!((i.region_count(), loaded.as_slice()), (1, &["b.wav".to_string()][..]));
    }
}
