//! A sampled instrument: parsed regions plus the decoded audio they play.

use super::parse::{Region, Trigger, parse_switched};
use super::sample::{Sample, SfzError};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Instrument {
    pub name: String,
    /// What a note-on can start (attack, first and legato triggers).
    regions: Vec<(Region, Arc<Sample>)>,
    /// What a note-off starts: release noises.
    releases: Vec<(Region, Arc<Sample>)>,
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

/// Decodes `names` with `load` on up to every core, results in the same order. The calling thread
/// works too, so where no thread can be started (wasm) everything still decodes, one by one.
fn decode_all(names: &[&str], load: &(dyn Fn(&str) -> Result<Sample, SfzError> + Sync)) -> Vec<Result<Sample, SfzError>> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Result<Sample, SfzError>>>> = Mutex::new((0..names.len()).map(|_| None).collect());
    let work = || {
        loop {
            let i = next.fetch_add(1, Ordering::Relaxed);
            let Some(name) = names.get(i) else { break };
            let r = load(name);
            if let Some(slot) = results.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get_mut(i) {
                *slot = Some(r);
            }
        }
    };
    let helpers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(names.len()).saturating_sub(1).min(15);
    std::thread::scope(|scope| {
        for _ in 0..helpers {
            if std::thread::Builder::new().name("sfz-decode".into()).spawn_scoped(scope, work).is_err() {
                break;
            }
        }
        work();
    });
    results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .into_iter()
        .map(|r| r.unwrap_or_else(|| Err(SfzError::Decode("not decoded".into()))))
        .collect()
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
        load: &(dyn Fn(&str) -> Result<Sample, SfzError> + Sync),
        keys: Option<&[u8]>,
    ) -> Result<Instrument, SfzError> {
        Instrument::build_switched(name, text, include, load, keys, None)
    }

    /// [`build`](Self::build) with `switch` as the active keyswitch (see [`parse_switched`]).
    pub fn build_switched(
        name: &str,
        text: &str,
        include: &mut dyn FnMut(&str) -> Option<String>,
        load: &(dyn Fn(&str) -> Result<Sample, SfzError> + Sync),
        keys: Option<&[u8]>,
        switch: Option<u8>,
    ) -> Result<Instrument, SfzError> {
        let parsed = parse_switched(text, include, switch);
        let mut warnings = parsed.warnings;
        let needed = keys.map(|ks| needed_regions(&parsed.regions, ks));
        let kept: Vec<Region> = parsed
            .regions
            .into_iter()
            .enumerate()
            .filter(|(i, _)| needed.as_ref().is_none_or(|n| n.get(*i).copied().unwrap_or(false)))
            .map(|(_, r)| r)
            .collect();
        // Each sample once, decoded on every core: a library's files are independent.
        let mut names: Vec<&str> = Vec::new();
        for r in &kept {
            if !names.contains(&r.sample.as_str()) {
                names.push(&r.sample);
            }
        }
        let decoded = decode_all(&names, load);
        let mut cache: HashMap<&str, Option<Arc<Sample>>> = HashMap::new();
        for (name, result) in names.iter().zip(decoded) {
            let sample = match result {
                Ok(s) => Some(Arc::new(s)),
                Err(e) => {
                    warnings.push(format!("{name}: {e}"));
                    None
                }
            };
            cache.insert(name, sample);
        }
        let (mut regions, mut releases) = (Vec::new(), Vec::new());
        for r in &kept {
            if let Some(Some(s)) = cache.get(r.sample.as_str()) {
                let list = if r.trigger == Trigger::Release { &mut releases } else { &mut regions };
                list.push((r.clone(), Arc::clone(s)));
            }
        }
        if regions.is_empty() && releases.is_empty() {
            return Err(SfzError::Empty(name.to_string()));
        }
        Ok(Instrument { name: name.to_string(), regions, releases, warnings })
    }

    pub fn region_count(&self) -> usize {
        self.regions.len() + self.releases.len()
    }

    /// The decoded samples it holds, each once, so `seen` (sample addresses already counted
    /// elsewhere) can sum several instruments without counting a shared one twice.
    pub fn memory(&self, seen: &mut std::collections::HashSet<usize>) -> usize {
        self.regions.iter().chain(&self.releases).filter(|(_, s)| seen.insert(Arc::as_ptr(s) as usize)).map(|(_, s)| s.bytes()).sum()
    }

    /// Whether a note-off starts anything.
    pub fn has_release(&self) -> bool {
        !self.releases.is_empty()
    }

    /// Whether the instrument has samples of its own for legato notes (`trigger=legato`).
    pub fn has_legato(&self) -> bool {
        self.regions.iter().any(|(r, _)| r.trigger == Trigger::Legato)
    }

    /// Regions that sound for `key` at `velocity` (0..=127). `seq` is the instrument's note counter
    /// (round robin) and `rand` a uniform 0..1 draw; `legato` says another note is held, which
    /// picks the `trigger=legato` regions over the `trigger=first` ones. A key outside every range
    /// borrows the nearest region and is transposed by the voice.
    pub fn select(&self, key: u8, velocity: u8, seq: u32, rand: f32, legato: bool) -> Vec<Hit<'_>> {
        let trigger = |r: &Region| match r.trigger {
            Trigger::Attack => true,
            Trigger::First => !legato,
            Trigger::Legato => legato,
            Trigger::Release => false,
        };
        pick(&self.regions, key, velocity, seq, rand, &trigger)
    }

    /// The release regions a note-off on `key` at `velocity` starts.
    pub fn select_release(&self, key: u8, velocity: u8, seq: u32, rand: f32) -> Vec<Hit<'_>> {
        pick(&self.releases, key, velocity, seq, rand, &|_| true)
    }

    /// The (lowest, highest) key any region covers.
    pub fn key_range(&self) -> (u8, u8) {
        let all = || self.regions.iter().chain(&self.releases);
        let lo = all().map(|(r, _)| r.lokey).min().unwrap_or(0);
        let hi = all().map(|(r, _)| r.hikey).max().unwrap_or(127);
        (lo, hi)
    }
}

/// The regions of `list` that sound for `key`: in range, velocity, random draw, round robin and
/// `trigger`, or the nearest ones when none covers the key.
fn pick<'a>(list: &'a [(Region, Arc<Sample>)], key: u8, velocity: u8, seq: u32, rand: f32, trigger: &dyn Fn(&Region) -> bool) -> Vec<Hit<'a>> {
    let by_vel = |r: &Region| velocity >= r.lovel && velocity <= r.hivel && rand >= r.lorand && (rand < r.hirand || r.hirand >= 1.0) && trigger(r);
    let round_robin = |r: &Region| r.seq_length <= 1 || seq % r.seq_length + 1 == r.seq_position;
    let hits = |range: &dyn Fn(&Region) -> bool| -> Vec<Hit<'a>> {
        list.iter().filter(|(r, _)| range(r) && by_vel(r) && round_robin(r)).map(|(r, s)| Hit { region: r, sample: s }).collect()
    };
    let exact = hits(&|r| key >= r.lokey && key <= r.hikey);
    if !exact.is_empty() {
        return exact;
    }
    let distance = |r: &Region| if key < r.lokey { r.lokey - key } else { key.saturating_sub(r.hikey) };
    let nearest = list.iter().filter(|(r, _)| by_vel(r)).map(|(r, _)| distance(r)).min();
    match nearest {
        Some(d) => hits(&|r| distance(r) == d),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32) -> Sample {
        Sample::from_f32(rate, &[0.5; 100], None)
    }

    fn inst(text: &str) -> Instrument {
        Instrument::build("t", text, &mut |_| None, &|_| Ok(tone(44_100)), None).unwrap()
    }

    #[test]
    fn selects_by_key_velocity_and_round_robin() {
        let i = inst(
            "<region> sample=a.wav lokey=40 hikey=50 hivel=60\n<region> sample=b.wav lokey=40 hikey=50 lovel=61\n<region> sample=c.wav key=60 seq_length=2 seq_position=1\n<region> sample=d.wav key=60 seq_length=2 seq_position=2",
        );
        assert_eq!(i.select(45, 30, 0, 0.5, false).first().map(|h| h.region.sample.as_str()), Some("a.wav"));
        assert_eq!(i.select(45, 100, 0, 0.5, false).first().map(|h| h.region.sample.as_str()), Some("b.wav"));
        assert_eq!(i.select(60, 100, 0, 0.5, false).first().map(|h| h.region.sample.as_str()), Some("c.wav"));
        assert_eq!(i.select(60, 100, 1, 0.5, false).first().map(|h| h.region.sample.as_str()), Some("d.wav"));
        assert_eq!(i.key_range(), (40, 60));
    }

    #[test]
    fn out_of_range_borrows_the_nearest_region() {
        let i = inst("<region> sample=a.wav lokey=40 hikey=50\n<region> sample=b.wav lokey=60 hikey=70");
        assert_eq!(i.select(30, 100, 0, 0.5, false).first().map(|h| h.region.sample.as_str()), Some("a.wav"));
        assert_eq!(i.select(120, 100, 0, 0.5, false).first().map(|h| h.region.sample.as_str()), Some("b.wav"));
    }

    #[test]
    fn missing_samples_warn_and_empty_fails() {
        let i = Instrument::build(
            "t",
            "<region> sample=a.wav key=1\n<region> sample=b.wav key=2",
            &mut |_| None,
            &|n| if n == "a.wav" { Ok(tone(8000)) } else { Err(SfzError::Io("gone".into())) },
            None,
        )
        .unwrap();
        assert_eq!(i.region_count(), 1);
        assert_eq!(i.warnings.len(), 1);
        assert!(Instrument::build("t", "<region> sample=b.wav key=2", &mut |_| None, &|_| Err(SfzError::Io("x".into())), None).is_err());
        assert!(Instrument::build("t", "", &mut |_| None, &|_| Ok(tone(8000)), None).is_err());
    }

    #[test]
    fn out_of_range_keys_load_the_nearest_region() {
        let loaded = std::sync::Mutex::new(Vec::new());
        let text = "<region> sample=a.wav lokey=40 hikey=50\n<region> sample=b.wav lokey=60 hikey=70";
        let load = |n: &str| {
            loaded.lock().unwrap().push(n.to_string());
            Ok(tone(8000))
        };
        let i = Instrument::build("t", text, &mut |_| None, &load, Some(&[30])).unwrap();
        assert_eq!(i.select(30, 100, 0, 0.5, false).len(), 1);
        assert_eq!(*loaded.lock().unwrap(), ["a.wav"]);
    }

    #[test]
    fn unwanted_regions_are_not_loaded() {
        let loaded = std::sync::Mutex::new(Vec::new());
        let i = Instrument::build(
            "t",
            "<region> sample=a.wav key=1\n<region> sample=b.wav key=2",
            &mut |_| None,
            &|n| {
                loaded.lock().unwrap().push(n.to_string());
                Ok(tone(8000))
            },
            Some(&[2]),
        )
        .unwrap();
        assert_eq!((i.region_count(), loaded.lock().unwrap().as_slice()), (1, &["b.wav".to_string()][..]));
    }

    #[test]
    fn many_samples_decode_in_parallel_in_region_order_each_once() {
        // Sixty regions over twenty files: every file decoded once, regions keep their order and
        // their own sample, one bad file only warns.
        let text: String = (0..60).map(|i| format!("<region> sample=s{}.wav key={}\n", i % 20, i)).collect();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let i = Instrument::build(
            "t",
            &text,
            &mut |_| None,
            &|n| {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let k: usize = n.trim_start_matches('s').trim_end_matches(".wav").parse().unwrap();
                if k == 7 { Err(SfzError::Io("gone".into())) } else { Ok(Sample::from_f32(8000, &[k as f32 / 100.0; 10], None)) }
            },
            None,
        )
        .unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 20);
        assert_eq!((i.region_count(), i.warnings.len()), (57, 1));
        for key in [0u8, 21, 59] {
            let hit = i.select(key, 100, 0, 0.5, false);
            assert!((hit[0].sample.get(0).0 - f32::from(key % 20) / 100.0).abs() < 1e-4, "key {key}");
        }
    }
}
