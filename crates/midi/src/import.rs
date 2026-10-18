//! Standard MIDI File import (format 0 and 1), as Guitar Pro's File › Import › MIDI does it: each
//! track and channel becomes a track, channel 10 a drum kit; note starts are quantized per quarter
//! note to the coarsest grid that holds them (so triplets are found), notes are fingered onto
//! strings and frets, and tempo, time and key signatures become bar properties.
//!
//! Written from the public Standard MIDI File 1.0 specification. Every count and length read from
//! the file is checked; anything the model cannot hold is dropped and reported in
//! [`Import::warnings`].

use crate::MidiError;
use guitario_score::{Beat, Dynamic, Measure, MeasureHeader, Note, NoteKind, Score, Track, Voice};
use guitario_theory::{Duration, NoteValue, TICKS_PER_QUARTER, TimeSignature, Tuning};
use std::collections::BTreeMap;

/// Largest file read.
pub const MAX_FILE: usize = 64 * 1024 * 1024;
/// Most events read from all tracks together.
const MAX_EVENTS: usize = 4_000_000;
/// Most track chunks read.
const MAX_CHUNKS: usize = 1024;
/// Most tracks the score gets.
const MAX_PARTS: usize = 64;
/// Most bars the score gets.
const MAX_MEASURES: usize = 20_000;
/// The finest straight value the quantizer writes: a 32nd note.
const FINEST: u32 = TICKS_PER_QUARTER / 8;
/// How far a start may sit from a grid point and still count as on it (a 128th note).
const SLACK: u32 = TICKS_PER_QUARTER / 32;
/// Highest fret the fingering uses.
const TOP_FRET: u8 = 24;

#[derive(Debug)]
pub struct Import {
    pub score: Score,
    pub warnings: Vec<String>,
}

/// True when `path` ends in `.mid` or `.midi` (any case).
pub fn is_midi_path(path: &str) -> bool {
    std::path::Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mid") || e.eq_ignore_ascii_case("midi"))
}

#[derive(Clone, Debug)]
enum Ev {
    On { ch: u8, key: u8, vel: u8 },
    Off { ch: u8, key: u8 },
    Program { ch: u8, program: u8 },
    Control { ch: u8, cc: u8, value: u8 },
    Bend,
    Tempo(u32),
    TimeSig(u8, u8),
    Key(i8),
    Name(String),
    Copyright(String),
}

struct Timed {
    tick: u64,
    chunk: usize,
    ev: Ev,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

fn damaged(what: &str) -> MidiError {
    MidiError::Damaged(what.to_string())
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8, MidiError> {
        let b = *self.bytes.get(self.at).ok_or_else(|| damaged("the file ends inside an event"))?;
        self.at += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], MidiError> {
        let end = self.at.checked_add(n).ok_or_else(|| damaged("a length overflows"))?;
        let out = self.bytes.get(self.at..end).ok_or_else(|| damaged("a chunk or event runs past the end of the file"))?;
        self.at = end;
        Ok(out)
    }

    fn u16(&mut self) -> Result<u16, MidiError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b.first().copied().unwrap_or(0), b.get(1).copied().unwrap_or(0)]))
    }

    fn u32(&mut self) -> Result<u32, MidiError> {
        let b = self.take(4)?;
        let mut a = [0u8; 4];
        a.copy_from_slice(b);
        Ok(u32::from_be_bytes(a))
    }

    /// A variable-length quantity: at most four bytes, as the specification allows.
    fn vlq(&mut self) -> Result<u32, MidiError> {
        let mut v = 0u32;
        for _ in 0..4 {
            let b = self.byte()?;
            v = (v << 7) | u32::from(b & 0x7f);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(damaged("a variable-length number is longer than four bytes"))
    }

    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }
}

fn text(bytes: &[u8]) -> String {
    let s: String = String::from_utf8_lossy(bytes).chars().filter(|c| !c.is_control()).collect();
    s.trim().chars().take(200).collect()
}

/// Reads one `MTrk` body into events at absolute ticks.
fn read_track(body: &[u8], chunk: usize, out: &mut Vec<Timed>) -> Result<(), MidiError> {
    let mut r = Reader { bytes: body, at: 0 };
    let mut tick = 0u64;
    let mut status = 0u8;
    while !r.done() {
        if out.len() >= MAX_EVENTS {
            return Err(MidiError::TooLarge);
        }
        tick = tick.saturating_add(u64::from(r.vlq()?));
        let mut first = r.byte()?;
        if first & 0x80 != 0 {
            if first < 0xf0 {
                status = first;
            }
            if first != 0xff && first != 0xf0 && first != 0xf7 {
                first = r.byte()?;
            }
        } else if status == 0 {
            return Err(damaged("running status without a status byte"));
        }
        let at = |ev| Timed { tick, chunk, ev };
        match if first & 0x80 != 0 && (first == 0xff || first == 0xf0 || first == 0xf7) { first } else { status } {
            0xff => {
                let kind = r.byte()?;
                let len = r.vlq()? as usize;
                let data = r.take(len)?;
                let get = |i: usize| data.get(i).copied().unwrap_or(0);
                match kind {
                    0x2f => break,
                    0x51 if len >= 3 => out.push(at(Ev::Tempo((u32::from(get(0)) << 16) | (u32::from(get(1)) << 8) | u32::from(get(2))))),
                    0x58 if len >= 2 => out.push(at(Ev::TimeSig(get(0), get(1)))),
                    0x59 if len >= 1 => out.push(at(Ev::Key(get(0) as i8))),
                    0x03 => out.push(at(Ev::Name(text(data)))),
                    0x02 => out.push(at(Ev::Copyright(text(data)))),
                    _ => {}
                }
            }
            0xf0 | 0xf7 => {
                let len = r.vlq()? as usize;
                r.take(len)?;
            }
            s => {
                let ch = s & 0x0f;
                let a = first & 0x7f;
                match s & 0xf0 {
                    0x80 => {
                        r.byte()?;
                        out.push(at(Ev::Off { ch, key: a }));
                    }
                    0x90 => {
                        let vel = r.byte()? & 0x7f;
                        out.push(at(if vel == 0 { Ev::Off { ch, key: a } } else { Ev::On { ch, key: a, vel } }));
                    }
                    0xa0 => {
                        r.byte()?;
                    }
                    0xb0 => {
                        let value = r.byte()? & 0x7f;
                        out.push(at(Ev::Control { ch, cc: a, value }));
                    }
                    0xc0 => out.push(at(Ev::Program { ch, program: a })),
                    0xd0 => {}
                    0xe0 => {
                        let hi = r.byte()?;
                        if a != 0 || hi != 0x40 {
                            out.push(at(Ev::Bend));
                        }
                    }
                    _ => return Err(damaged("an unknown status byte")),
                }
            }
        }
    }
    Ok(())
}

/// One sounding note in score ticks.
#[derive(Clone, Copy, Debug)]
struct Played {
    start: u64,
    end: u64,
    key: u8,
    vel: u8,
}

/// The notes of one track chunk on one channel.
struct Part {
    chunk: usize,
    ch: u8,
    notes: Vec<Played>,
    program: Option<u8>,
}

/// Reads a Standard MIDI File from memory into a score.
pub fn import(bytes: &[u8]) -> Result<Import, MidiError> {
    if bytes.len() > MAX_FILE {
        return Err(MidiError::TooLarge);
    }
    let mut r = Reader { bytes, at: 0 };
    if r.take(4).ok() != Some(b"MThd".as_slice()) {
        return Err(MidiError::NotMidi);
    }
    let head_len = r.u32()? as usize;
    let head = r.take(head_len)?;
    let mut h = Reader { bytes: head, at: 0 };
    let format = h.u16()?;
    let count = usize::from(h.u16()?);
    let division = h.u16()?;
    if format > 2 {
        return Err(MidiError::Unsupported(format!("Standard MIDI File format {format}")));
    }
    let mut warnings = Vec::new();
    if format == 2 {
        warnings.push("a format 2 file holds separate songs; they are read one after another as tracks of one".into());
    }
    // Ticks per quarter note. SMPTE time counts in seconds; read it at the default 120 bpm.
    let ppq: u64 = if division & 0x8000 != 0 {
        let fps = u64::from((-((division >> 8) as i8) as i16).unsigned_abs());
        let per_frame = u64::from(division & 0xff);
        warnings.push("the file counts time in SMPTE frames; it is read at 120 bpm".into());
        (fps * per_frame / 2).max(1)
    } else {
        u64::from(division)
    };
    if ppq == 0 {
        return Err(damaged("the header gives zero ticks per quarter note"));
    }
    let mut events = Vec::new();
    let mut chunks = 0usize;
    while !r.done() && chunks < count.min(MAX_CHUNKS) {
        let tag = r.take(4)?;
        let len = r.u32()? as usize;
        // A short last chunk is read as far as it goes.
        let body = r.take(len.min(r.bytes.len().saturating_sub(r.at)))?;
        if tag == b"MTrk" {
            read_track(body, chunks, &mut events)?;
            chunks += 1;
        }
    }
    if chunks == 0 {
        return Err(damaged("the file has no tracks"));
    }
    let scale = |t: u64| -> u64 { (u128::from(t) * u128::from(TICKS_PER_QUARTER) / u128::from(ppq)).min(u128::from(u64::MAX / 4)) as u64 };
    events.sort_by_key(|e| e.tick);
    build(events, scale, &mut warnings).map(|score| Import { score, warnings })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn import_file(path: &std::path::Path) -> Result<Import, MidiError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| MidiError::Io(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE as u64 + 1).read_to_end(&mut bytes).map_err(|e| MidiError::Io(e.to_string()))?;
    import(&bytes)
}

#[cfg(target_arch = "wasm32")]
pub fn import_file(_path: &std::path::Path) -> Result<Import, MidiError> {
    Err(MidiError::Io("files on disk are not available in the browser build; use import".into()))
}

fn build(events: Vec<Timed>, scale: impl Fn(u64) -> u64, warnings: &mut Vec<String>) -> Result<Score, MidiError> {
    let mut parts: Vec<Part> = Vec::new();
    let mut names: BTreeMap<usize, String> = BTreeMap::new();
    let mut copyright = String::new();
    let mut tempos: Vec<(u64, u32)> = Vec::new();
    let mut signatures: Vec<(u64, u8, u8)> = Vec::new();
    let mut keys: Vec<(u64, i8)> = Vec::new();
    // Sounding notes by (chunk, channel, key): start and velocity, oldest first.
    let mut open: BTreeMap<(usize, u8, u8), Vec<(u64, u8)>> = BTreeMap::new();
    let mut programs: BTreeMap<(usize, u8), u8> = BTreeMap::new();
    let mut volumes: BTreeMap<(usize, u8), u8> = BTreeMap::new();
    let mut pans: BTreeMap<(usize, u8), u8> = BTreeMap::new();
    let mut bends = 0usize;
    let mut song_end = 0u64;
    let part_of = |parts: &mut Vec<Part>, chunk: usize, ch: u8| -> Option<usize> {
        if let Some(i) = parts.iter().position(|p| p.chunk == chunk && p.ch == ch) {
            return Some(i);
        }
        if parts.len() >= MAX_PARTS {
            return None;
        }
        parts.push(Part { chunk, ch, notes: Vec::new(), program: None });
        Some(parts.len() - 1)
    };
    let mut dropped_parts = false;
    for e in events {
        let tick = scale(e.tick);
        match e.ev {
            Ev::On { ch, key, vel } => {
                let program = programs.get(&(e.chunk, ch)).copied();
                match part_of(&mut parts, e.chunk, ch) {
                    Some(i) => {
                        if let Some(p) = parts.get_mut(i).filter(|p| p.program.is_none()) {
                            p.program = program;
                        }
                        open.entry((e.chunk, ch, key)).or_default().push((tick, vel));
                    }
                    None => dropped_parts = true,
                }
            }
            Ev::Off { ch, key } => {
                let Some(stack) = open.get_mut(&(e.chunk, ch, key)) else { continue };
                if stack.is_empty() {
                    continue;
                }
                let (start, vel) = stack.remove(0);
                if let Some(p) = parts.iter_mut().find(|p| p.chunk == e.chunk && p.ch == ch) {
                    p.notes.push(Played { start, end: tick.max(start), key, vel });
                    song_end = song_end.max(tick);
                }
            }
            Ev::Program { ch, program } => {
                programs.insert((e.chunk, ch), program);
            }
            // The first volume and pan a channel sets are the track's mixer values.
            Ev::Control { ch, cc: 7, value } => {
                volumes.entry((e.chunk, ch)).or_insert(value);
            }
            Ev::Control { ch, cc: 10, value } => {
                pans.entry((e.chunk, ch)).or_insert(value);
            }
            Ev::Control { .. } => {}
            Ev::Bend => bends += 1,
            Ev::Tempo(us) => tempos.push((tick, us)),
            Ev::TimeSig(n, d) => signatures.push((tick, n, d)),
            Ev::Key(sf) => keys.push((tick, sf)),
            Ev::Name(n) => {
                names.entry(e.chunk).or_insert(n);
            }
            Ev::Copyright(c) => {
                if copyright.is_empty() {
                    copyright = c;
                }
            }
        }
    }
    // Notes never released end where the song does.
    for ((chunk, ch, key), stack) in open {
        for (start, vel) in stack {
            if let Some(p) = parts.iter_mut().find(|p| p.chunk == chunk && p.ch == ch) {
                p.notes.push(Played { start, end: song_end.max(start + u64::from(TICKS_PER_QUARTER)), key, vel });
            }
        }
    }
    if dropped_parts {
        warnings.push(format!("only the first {MAX_PARTS} tracks were read"));
    }
    if bends > 0 {
        warnings.push(format!("{bends} pitch-bend events were not imported"));
    }
    parts.retain(|p| !p.notes.is_empty());
    parts.sort_by_key(|p| (p.chunk, p.ch));
    for p in &mut parts {
        p.notes.sort_by_key(|n| (n.start, std::cmp::Reverse(n.key)));
        song_end = song_end.max(p.notes.iter().map(|n| n.end).max().unwrap_or(0));
    }

    let headers = bars(&signatures, &keys, song_end, warnings);
    let starts: Vec<u64> = {
        let mut t = 0u64;
        let mut v: Vec<u64> = headers
            .iter()
            .map(|h| {
                let s = t;
                t += u64::from(h.capacity());
                s
            })
            .collect();
        v.push(t);
        v
    };
    let mut score = Score::new();
    score.headers = headers;
    tempo_marks(&mut score, &starts, &tempos, warnings);
    score.meta.copyright = copyright;
    // A first track without notes is the conductor track; its name is the song's title.
    let first_has_notes = parts.iter().any(|p| p.chunk == 0);
    if !first_has_notes && let Some(title) = names.get(&0) {
        score.meta.title = title.clone();
    }
    if parts.is_empty() {
        warnings.push("the file has no notes".into());
        score.tracks = vec![Track::new("Guitar", Tuning::standard(), &score.headers)];
        return Ok(score);
    }
    let mut tracks = Vec::new();
    let per_chunk = |chunk: usize| parts.iter().filter(|p| p.chunk == chunk).count();
    for (i, p) in parts.iter().enumerate() {
        let drums = p.ch == 9;
        let base = names.get(&p.chunk).filter(|n| !n.is_empty() && (first_has_notes || p.chunk != 0)).cloned();
        let name = match base {
            Some(n) if per_chunk(p.chunk) > 1 => format!("{n} ({})", u16::from(p.ch) + 1),
            Some(n) => n,
            None if drums => "Drums".into(),
            None => format!("Track {}", i + 1),
        };
        let program = p.program.unwrap_or(0);
        let mut track = if drums {
            Track::drums(name, &score.headers)
        } else {
            let (tuning, moved) = tuning_for(program, &p.notes);
            if moved > 0 {
                warnings.push(format!("track {}: {moved} notes out of the instrument's range were moved by octaves", i + 1));
            }
            let mut t = Track::new(name, tuning, &score.headers);
            t.program = program;
            t
        };
        if let Some(&v) = volumes.get(&(p.chunk, p.ch)) {
            track.volume = v;
        }
        if let Some(&pan) = pans.get(&(p.chunk, p.ch)) {
            track.pan = (i16::from(pan) - 64).clamp(-63, 63) as i8;
        }
        let lost = fill_track(&mut track, &p.notes, &starts, drums);
        if lost > 0 {
            warnings.push(format!("track {}: {lost} notes found no free string and were left out", i + 1));
        }
        tracks.push(track);
    }
    score.tracks = tracks;
    score.validate()?;
    Ok(score)
}

/// Bar headers from the time and key signature events, enough to hold `end`.
fn bars(signatures: &[(u64, u8, u8)], keys: &[(u64, i8)], end: u64, warnings: &mut Vec<String>) -> Vec<MeasureHeader> {
    let mut out = Vec::new();
    let mut ts = TimeSignature::COMMON;
    let mut key = 0i8;
    let mut start = 0u64;
    let mut moved = 0usize;
    let mut refused = 0usize;
    let (mut si, mut ki) = (0usize, 0usize);
    loop {
        // Signatures up to the bar's start take effect here; one inside the bar before moved here.
        while let Some(&(t, n, d)) = signatures.get(si).filter(|s| s.0 <= start) {
            if t != start {
                moved += 1;
            }
            match TimeSignature::new(n, 1u8.checked_shl(u32::from(d)).unwrap_or(0)) {
                Some(next) => ts = next,
                None => refused += 1,
            }
            si += 1;
        }
        while let Some(&(_, sf)) = keys.get(ki).filter(|k| k.0 <= start) {
            key = if (-7..=7).contains(&sf) { sf } else { key };
            ki += 1;
        }
        let mut h = MeasureHeader::new(ts);
        h.key = key;
        let cap = u64::from(h.capacity());
        out.push(h);
        start += cap;
        if start >= end || out.len() >= MAX_MEASURES {
            break;
        }
    }
    if out.len() >= MAX_MEASURES && start < end {
        warnings.push(format!("the song was cut at {MAX_MEASURES} bars"));
    }
    if moved > 0 {
        warnings.push(format!("{moved} time signatures inside a bar were moved to the next bar"));
    }
    if refused > 0 {
        warnings.push(format!("{refused} time signatures the score cannot hold were ignored"));
    }
    out
}

fn bpm(us: u32) -> u16 {
    (60_000_000.0 / f64::from(us.max(1))).round().clamp(20.0, 400.0) as u16
}

/// The song tempo, then a tempo mark on each bar where the tempo changes.
fn tempo_marks(score: &mut Score, starts: &[u64], tempos: &[(u64, u32)], warnings: &mut Vec<String>) {
    let mut current = tempos.iter().take_while(|t| t.0 == 0).last().map_or(120, |t| bpm(t.1));
    score.tempo = current;
    let mut inside = 0usize;
    for (b, h) in score.headers.iter_mut().enumerate().skip(1) {
        let (Some(&s), Some(&e)) = (starts.get(b), starts.get(b + 1)) else { continue };
        // A change just before a bar line counts for the bar it leads into.
        let mut here = tempos.iter().filter(|t| t.0 + u64::from(SLACK) >= s && t.0 + u64::from(SLACK) < e);
        if let Some(&(_, us)) = here.next() {
            let t = bpm(us);
            if t != current {
                h.tempo = Some(t);
                current = t;
            }
        }
        inside += here.count();
    }
    if inside > 0 {
        warnings.push(format!("{inside} tempo changes inside bars were left out; each bar keeps the tempo it starts with"));
    }
}

/// A tuning that fits the part's range, and how many notes still fall outside it.
fn tuning_for(program: u8, notes: &[Played]) -> (Tuning, usize) {
    let low = notes.iter().map(|n| n.key).min().unwrap_or(40);
    let tuning = if (32..=39).contains(&program) {
        if low >= 28 { Tuning::bass() } else { Tuning::bass_five() }
    } else if low >= 40 {
        Tuning::standard()
    } else if low >= 38 {
        Tuning::drop_d()
    } else if low >= 35 {
        Tuning::seven_string()
    } else if low >= 33 {
        Tuning::seven_string_drop_a()
    } else {
        Tuning::eight_string()
    };
    let (lo, hi) = range(&tuning);
    let moved = notes.iter().filter(|n| n.key < lo || n.key > hi).count();
    (tuning, moved)
}

/// The lowest open string and the highest fretted note of a tuning.
fn range(t: &Tuning) -> (u8, u8) {
    let lo = t.strings.iter().copied().min().unwrap_or(40);
    let hi = t.strings.iter().copied().max().unwrap_or(64).saturating_add(TOP_FRET).min(127);
    (lo, hi)
}

/// Moves a key by octaves into `lo..=hi`.
fn into_range(key: u8, lo: u8, hi: u8) -> u8 {
    let mut k = key;
    while k < lo && k <= 115 {
        k += 12;
    }
    while k > hi && k >= 12 {
        k -= 12;
    }
    k
}

/// The grid of one quarter-note window: how many slots it splits into.
fn grid(len: u32, offsets: &[u32]) -> u32 {
    let ok = |k: u32| len.is_multiple_of(k) && ((len / k).is_multiple_of(FINEST) || (len == TICKS_PER_QUARTER && (k == 3 || k == 6)));
    let errors = |k: u32| offsets.iter().map(move |&o| (o % (len / k)).min(len / k - o % (len / k)));
    for k in [1u32, 2, 3, 4, 6, 8] {
        if ok(k) && errors(k).max().unwrap_or(0) < (len / k / 4).clamp(1, SLACK) {
            return k;
        }
    }
    // Played rather than written (or a grace note off the grid): sixteenths, or eighth-note
    // triplets when they are nearer in all.
    let straight = sixteenths(len);
    if ok(3) && errors(3).sum::<u32>() < errors(straight).sum::<u32>() { 3 } else { straight }
}

/// The slots of a window without starts: sixteenths, or as fine as its length allows.
fn sixteenths(len: u32) -> u32 {
    [4u32, 2, 1].into_iter().find(|&k| len.is_multiple_of(k) && (len / k).is_multiple_of(FINEST)).unwrap_or(1)
}

/// Bar starts and quarter windows: snaps a score tick to the grid of the window it falls in.
struct Grid<'a> {
    starts: &'a [u64],
    /// Slots per window, by (bar, window).
    slots: BTreeMap<(usize, u32), u32>,
}

impl Grid<'_> {
    fn bar_of(&self, t: u64) -> usize {
        self.starts.partition_point(|&s| s <= t).saturating_sub(1).min(self.starts.len().saturating_sub(2))
    }

    /// The window holding `t`: (bar, index, its start, its length).
    fn window(&self, t: u64) -> (usize, u32, u64, u32) {
        let b = self.bar_of(t);
        let s = self.starts.get(b).copied().unwrap_or(0);
        let e = self.starts.get(b + 1).copied().unwrap_or(s);
        let q = u64::from(TICKS_PER_QUARTER);
        let w = (t.saturating_sub(s) / q).min(e.saturating_sub(s).saturating_sub(1) / q);
        let ws = s + w * q;
        (b, w as u32, ws, (e.saturating_sub(ws)).min(q).max(1) as u32)
    }

    fn snap(&self, t: u64) -> u64 {
        let (b, w, ws, len) = self.window(t);
        let k = self.slots.get(&(b, w)).copied().unwrap_or_else(|| sixteenths(len));
        let slot = u64::from(len / k.max(1)).max(1);
        let off = t.saturating_sub(ws);
        let n = (off + slot / 2) / slot;
        ws + (n * slot).min(u64::from(len))
    }

    /// True when the window holding `t` is split in triplets.
    fn triplet(&self, t: u64) -> bool {
        let (b, w, _, _) = self.window(t);
        matches!(self.slots.get(&(b, w)), Some(3 | 6))
    }
}

/// Durations filling `len` ticks, longest first: straight values with one dot, or in a triplet
/// window, triplet quarters, eighths and sixteenths.
fn durations(len: u64, triplet: bool) -> Vec<Duration> {
    let mut out = Vec::new();
    let mut left = len;
    let choices: Vec<Duration> = if triplet {
        [NoteValue::Quarter, NoteValue::Eighth, NoteValue::Sixteenth].into_iter().map(|v| Duration { tuplet: (3, 2), ..Duration::new(v) }).collect()
    } else {
        NoteValue::ALL.into_iter().flat_map(|v| [Duration::new(v).dotted(1), Duration::new(v)]).collect()
    };
    while left > 0 && out.len() < 64 {
        let Some(d) = choices.iter().copied().find(|d| u64::from(d.ticks()) <= left) else { break };
        out.push(d);
        left -= u64::from(d.ticks());
    }
    out
}

/// One stretch of a voice: notes (strings and frets) or a rest, from `start` to `end`.
struct Span {
    start: u64,
    end: u64,
    notes: Vec<Note>,
    vel: u8,
}

fn dynamic(vel: u8) -> Dynamic {
    [Dynamic::Ppp, Dynamic::Pp, Dynamic::P, Dynamic::Mp, Dynamic::Mf, Dynamic::F, Dynamic::Ff, Dynamic::Fff]
        .into_iter()
        .min_by_key(|d| (i16::from(d.velocity()) - i16::from(vel)).abs())
        .unwrap_or_default()
}

/// Fills a track's bars from its notes; returns how many notes found no string.
fn fill_track(track: &mut Track, notes: &[Played], starts: &[u64], drums: bool) -> usize {
    let song_end = starts.last().copied().unwrap_or(0);
    // Each quarter window's grid from the starts that fall in it.
    let mut grid_of = Grid { starts, slots: BTreeMap::new() };
    let mut offsets: BTreeMap<(usize, u32), (u32, Vec<u32>)> = BTreeMap::new();
    for n in notes {
        let (b, w, ws, len) = grid_of.window(n.start);
        offsets.entry((b, w)).or_insert((len, Vec::new())).1.push(n.start.saturating_sub(ws) as u32);
    }
    grid_of.slots = offsets.into_iter().map(|(at, (len, offs))| (at, grid(len, &offs))).collect();

    // Chords by quantized start.
    let mut chords: BTreeMap<u64, Vec<Played>> = BTreeMap::new();
    for n in notes {
        let start = grid_of.snap(n.start).min(song_end);
        if start >= song_end {
            continue;
        }
        let mut end = grid_of.snap(n.end).min(song_end);
        if end <= start {
            // Shorter than the grid: one slot long.
            let (b, w, _, len) = grid_of.window(start);
            let k = grid_of.slots.get(&(b, w)).copied().unwrap_or(1).max(1);
            end = (start + u64::from(len / k)).min(song_end);
        }
        chords.entry(start).or_default().push(Played { start, end, ..*n });
    }

    let (lo, hi) = range(&track.tuning);
    let mut lost = 0usize;
    let mut hand = 0.0f32;
    let mut spans: Vec<Span> = Vec::new();
    let onsets: Vec<u64> = chords.keys().copied().collect();
    let mut cursor = 0u64;
    for (i, (&start, group)) in chords.iter().enumerate() {
        let next = onsets.get(i + 1).copied().unwrap_or(song_end);
        let longest = group.iter().map(|n| n.end).max().unwrap_or(next);
        let end =
            if drums { if i + 1 < onsets.len() { next } else { longest.max(start + u64::from(TICKS_PER_QUARTER)).min(song_end) } } else { longest.min(next) };
        if start > cursor {
            spans.push(Span { start: cursor, end: start, notes: Vec::new(), vel: 0 });
        }
        let vel = group.iter().map(|n| n.vel).max().unwrap_or(80);
        let placed = if drums {
            let mut out: Vec<Note> = Vec::new();
            for n in group {
                let lane = guitario_theory::drums::lane_for(n.key);
                let free = (0..8u8).filter(|l| !out.iter().any(|o| o.string == *l)).min_by_key(|l| (i16::from(*l) - i16::from(lane)).abs());
                match free {
                    Some(l) if !out.iter().any(|o| o.fret == n.key) => out.push(Note::new(l, n.key)),
                    Some(_) => {}
                    None => lost += 1,
                }
            }
            out
        } else {
            let mut keys: Vec<u8> = group.iter().map(|n| into_range(n.key, lo, hi)).collect();
            keys.sort_unstable_by(|a, b| b.cmp(a));
            keys.dedup();
            let (placed, missing) = finger(&track.tuning, &keys, &mut hand);
            lost += missing;
            placed
        };
        if placed.is_empty() {
            spans.push(Span { start, end, notes: Vec::new(), vel: 0 });
        } else {
            spans.push(Span { start, end, notes: placed, vel });
        }
        cursor = end;
    }
    write_spans(track, &spans, starts, &grid_of);
    lost
}

/// Strings and frets for the keys of one chord (highest first), near the hand's position.
fn finger(tuning: &Tuning, keys: &[u8], hand: &mut f32) -> (Vec<Note>, usize) {
    let strings = &tuning.strings;
    let n = keys.len().min(strings.len());
    let mut best: Option<(f32, Vec<(u8, u8)>)> = None;
    let mut pick: Vec<(u8, u8)> = Vec::new();
    // Highest key on the lowest-numbered string, never crossing: a depth-first search.
    fn search(strings: &[u8], keys: &[u8], from: usize, hand: f32, pick: &mut Vec<(u8, u8)>, best: &mut Option<(f32, Vec<(u8, u8)>)>) {
        let Some(&key) = keys.get(pick.len()) else {
            let fretted: Vec<f32> = pick.iter().filter(|p| p.1 > 0).map(|p| f32::from(p.1)).collect();
            let span = fretted.iter().copied().fold(None, |a: Option<(f32, f32)>, f| Some(a.map_or((f, f), |(lo, hi)| (lo.min(f), hi.max(f)))));
            let stretch = span.map_or(0.0, |(lo, hi)| (hi - lo - 4.0).max(0.0) * 3.0);
            let travel: f32 = fretted.iter().map(|f| (f - hand).abs()).sum::<f32>() / fretted.len().max(1) as f32;
            let high: f32 = fretted.iter().map(|f| (f - 12.0).max(0.0) * 0.1).sum();
            let cost = travel + stretch + high;
            if best.as_ref().is_none_or(|b| cost < b.0) {
                *best = Some((cost, pick.clone()));
            }
            return;
        };
        for (s, &open) in strings.iter().enumerate().skip(from) {
            if key < open || key - open > TOP_FRET {
                continue;
            }
            pick.push((s as u8, key - open));
            search(strings, keys, s + 1, hand, pick, best);
            pick.pop();
        }
    }
    // Drop the middle notes of a chord too big for the strings until it fits.
    let mut kept: Vec<u8> = keys.to_vec();
    while kept.len() > n {
        kept.remove(kept.len() / 2);
    }
    loop {
        search(strings, &kept, 0, *hand, &mut pick, &mut best);
        if best.is_some() || kept.len() <= 1 {
            break;
        }
        let mid = kept.len() / 2;
        kept.remove(mid);
    }
    let Some((_, chosen)) = best else { return (Vec::new(), keys.len()) };
    let fretted: Vec<f32> = chosen.iter().filter(|p| p.1 > 0).map(|p| f32::from(p.1)).collect();
    if !fretted.is_empty() {
        *hand = fretted.iter().sum::<f32>() / fretted.len() as f32;
    }
    let missing = keys.len() - chosen.len();
    (chosen.into_iter().map(|(s, f)| Note::new(s, f)).collect(), missing)
}

/// Writes spans into the track's bars: split at bar lines and grid windows, the later pieces of a
/// note tied to the first.
fn write_spans(track: &mut Track, spans: &[Span], starts: &[u64], grid: &Grid) {
    let mut bars: Vec<Vec<Beat>> = vec![Vec::new(); starts.len().saturating_sub(1)];
    for span in spans {
        let mut at = span.start;
        let mut first = true;
        while at < span.end {
            let b = grid.bar_of(at);
            let bar_end = starts.get(b + 1).copied().unwrap_or(span.end).min(span.end);
            // Triplet windows are written on their own; straight time runs on to the bar line or
            // the next triplet window.
            let piece_end = if grid.triplet(at) {
                let (_, _, ws, len) = grid.window(at);
                (ws + u64::from(len)).min(bar_end)
            } else {
                let mut t = at;
                loop {
                    let (_, _, ws, len) = grid.window(t);
                    let next = ws + u64::from(len);
                    if next >= bar_end || grid.triplet(next) {
                        break next.min(bar_end);
                    }
                    t = next;
                }
            };
            if piece_end <= at {
                break;
            }
            let whole_window = {
                let (_, _, ws, len) = grid.window(at);
                at == ws && piece_end == ws + u64::from(len)
            };
            for d in durations(piece_end - at, grid.triplet(at) && !whole_window) {
                let Some(bar) = bars.get_mut(b) else { break };
                let mut beat = Beat::rest(d);
                if !span.notes.is_empty() {
                    beat.rest = false;
                    beat.dynamic = dynamic(span.vel);
                    beat.notes = span.notes.iter().map(|n| if first { *n } else { Note { kind: NoteKind::Tie, ..*n } }).collect();
                    first = false;
                }
                bar.push(beat);
            }
            at = piece_end;
        }
    }
    for ((measure, beats), header_cap) in track.measures.iter_mut().zip(bars).zip(starts.windows(2).map(|w| match w {
        [a, b] => b - a,
        _ => 0,
    })) {
        if beats.iter().all(|b| b.rest) {
            continue;
        }
        let used: u64 = beats.iter().map(|b| u64::from(b.duration.ticks())).sum();
        let mut beats = beats;
        if used < header_cap {
            beats.extend(durations(header_cap - used, false).into_iter().map(Beat::rest));
        }
        *measure = Measure { voices: vec![Voice { beats }], ..Measure::default() };
    }
}
