//! Guitar Pro 3, 4 and 5 (`.gp3`, `.gp4`, `.gp5`): the binary formats before Guitar Pro 6.
//!
//! Little-endian integers and length-prefixed Windows-1252 strings, read front to back: song
//! information, MIDI channels, master bars, tracks, then each track's measure for every master bar.
//! Written from public format notes and observation of files; nothing is copied from other
//! implementations. Anything the model cannot hold is dropped and counted in the warnings.

use crate::{GpError, Import, MAX_MEASURES, MAX_TRACKS};
use guitario_score::*;
use guitario_theory::{Duration, NoteValue, TICKS_PER_QUARTER, TimeSignature, Tuning};
use std::collections::BTreeMap;

/// The binary format generations. Order matters: later versions add fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Version {
    V3,
    V4,
    V5,
    /// 5.10 and later add RSE fields.
    V5_1,
}

/// The version banner every file starts with, e.g. `FICHIER GUITAR PRO v5.10`.
pub(crate) fn version_of(banner: &str) -> Option<Version> {
    let v = banner.strip_prefix("FICHIER GUITAR PRO v")?;
    let (major, minor) = v.split_once('.')?;
    let minor: u32 = minor.trim().get(..2).unwrap_or(minor.trim()).parse().ok()?;
    match major {
        "3" => Some(Version::V3),
        "4" => Some(Version::V4),
        "5" if minor == 0 => Some(Version::V5),
        "5" => Some(Version::V5_1),
        _ => None,
    }
}

/// True when `bytes` starts with a Guitar Pro 3-5 banner (or an older one this reader refuses
/// with a clear message).
pub(crate) fn looks_legacy(bytes: &[u8]) -> bool {
    bytes.get(1..19).is_some_and(|b| b.starts_with(b"FICHIER GUITAR") || b.starts_with(b"CLIPBOARD GUITAR"))
}

fn damaged(what: impl Into<String>) -> GpError {
    GpError::Damaged(what.into())
}

fn eof() -> GpError {
    damaged("the file ends early")
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], GpError> {
        let end = self.pos.checked_add(n).ok_or_else(eof)?;
        let out = self.data.get(self.pos..end).ok_or_else(eof)?;
        self.pos = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], GpError> {
        self.bytes(N)?.try_into().map_err(|_| eof())
    }

    fn skip(&mut self, n: usize) -> Result<(), GpError> {
        self.bytes(n).map(drop)
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn u8(&mut self) -> Result<u8, GpError> {
        Ok(u8::from_le_bytes(self.array()?))
    }

    fn i8(&mut self) -> Result<i8, GpError> {
        Ok(i8::from_le_bytes(self.array()?))
    }

    fn bool(&mut self) -> Result<bool, GpError> {
        Ok(self.u8()? != 0)
    }

    fn i16(&mut self) -> Result<i16, GpError> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    fn i32(&mut self) -> Result<i32, GpError> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    /// A non-negative count of at most `max`.
    fn count(&mut self, max: usize, what: &str) -> Result<usize, GpError> {
        let n = self.i32()?;
        usize::try_from(n).ok().filter(|n| *n <= max).ok_or_else(|| damaged(format!("{what} out of range ({n})")))
    }

    /// A length byte, then a field of exactly `size` bytes holding the text.
    fn fixed_string(&mut self, size: usize) -> Result<String, GpError> {
        let len = usize::from(self.u8()?);
        let raw = self.bytes(size)?;
        Ok(decode(raw.get(..len.min(size)).unwrap_or_default()))
    }

    /// An int giving the field size, then a length byte and the text.
    fn int_byte_string(&mut self) -> Result<String, GpError> {
        let size = self.count(self.remaining(), "string size")?;
        if size == 0 {
            return Ok(String::new());
        }
        let len = usize::from(self.u8()?);
        let raw = self.bytes(size - 1)?;
        Ok(decode(raw.get(..len.min(size - 1)).unwrap_or_default()))
    }

    /// An int length, then the text.
    fn int_string(&mut self) -> Result<String, GpError> {
        let n = self.count(self.remaining(), "string length")?;
        Ok(decode(self.bytes(n)?))
    }
}

/// Guitar Pro writes Windows-1252; files saved by other tools sometimes hold UTF-8.
fn decode(raw: &[u8]) -> String {
    let raw = raw.split(|b| *b == 0).next().unwrap_or_default();
    if let Ok(s) = std::str::from_utf8(raw) {
        return s.to_string();
    }
    raw.iter().map(|&b| cp1252(b)).collect()
}

fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™',
        'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9f => HIGH.get(usize::from(b - 0x80)).copied().unwrap_or('?'),
        _ => char::from(b),
    }
}

/// One of the 64 MIDI channels (4 ports of 16) a track plays through.
#[derive(Clone, Copy, Default)]
struct Channel {
    program: i32,
    volume: u8,
    balance: u8,
}

/// Effects stored on the beat that Guitario keeps on its notes.
#[derive(Default)]
struct BeatFx {
    vibrato: bool,
    wide_vibrato: bool,
    harmonic: Option<Harmonic>,
    tapping: bool,
}

/// A mix-table change: what Guitario uses of it.
struct Mix {
    instrument: i8,
    tempo: i32,
}

/// Per-track state while reading measures.
struct TrackState {
    strings: usize,
    drums: bool,
    /// The last fret on each string of each voice, for tied notes.
    last_fret: [[Option<u8>; 7]; 2],
    /// An instrument set by a mix table on the track's first beat.
    program: Option<u8>,
}

/// The import velocity Guitar Pro 3-5 stores per note (1 = ppp … 8 = fff); forte when absent.
fn dynamic_of(v: i8) -> Dynamic {
    match v {
        i8::MIN..=1 => Dynamic::Ppp,
        2 => Dynamic::Pp,
        3 => Dynamic::P,
        4 => Dynamic::Mp,
        5 => Dynamic::Mf,
        6 => Dynamic::F,
        7 => Dynamic::Ff,
        _ => Dynamic::Fff,
    }
}

/// `n` in the time of `m`: the tuplets Guitar Pro offers.
fn tuplet_of(enters: i32) -> (u8, u8) {
    match enters {
        3 => (3, 2),
        5..=7 => (enters as u8, 4),
        9..=13 => (enters as u8, 8),
        _ => (1, 1),
    }
}

/// A stroke speed 1..=6 (a 128th up to a quarter note) as ticks from first string to last.
fn stroke_ticks(speed: i8) -> u32 {
    let speed = u32::try_from(speed.clamp(1, 6)).unwrap_or(1);
    TICKS_PER_QUARTER >> (6 - speed)
}

struct Parser<'a> {
    r: Reader<'a>,
    v: Version,
    warnings: Vec<String>,
    /// Features the model cannot hold yet, counted for one warning each.
    dropped: BTreeMap<&'static str, usize>,
}

impl Parser<'_> {
    fn drop(&mut self, what: &'static str) {
        *self.dropped.entry(what).or_default() += 1;
    }
}

/// Reads a Guitar Pro 3, 4 or 5 file from memory.
pub fn parse(bytes: &[u8]) -> Result<Import, GpError> {
    let mut r = Reader { data: bytes, pos: 0 };
    let banner = r.fixed_string(30)?;
    let Some(v) = version_of(&banner) else {
        return Err(GpError::Unsupported(if banner.is_empty() { "not a Guitar Pro file".into() } else { format!("{banner} is not supported") }));
    };
    let mut p = Parser { r, v, warnings: Vec::new(), dropped: BTreeMap::new() };
    let score = p.song()?;
    // Some writers pad the end with zeros; anything else left over means part of the file was not understood.
    let rest = p.r.data.get(p.r.pos..).unwrap_or_default();
    if rest.iter().any(|b| *b != 0) {
        p.warnings.push(format!("{} bytes after the last measure were not read", rest.len()));
    }
    let mut warnings = std::mem::take(&mut p.warnings);
    warnings.extend(p.dropped.iter().map(|(what, n)| format!("{what} are not supported yet and were dropped ({n})")));
    score.validate()?;
    Ok(Import { score, warnings })
}

impl Parser<'_> {
    fn song(&mut self) -> Result<Score, GpError> {
        let v = self.v;
        let title = self.r.int_byte_string()?;
        let _subtitle = self.r.int_byte_string()?;
        let artist = self.r.int_byte_string()?;
        let album = self.r.int_byte_string()?;
        let words = self.r.int_byte_string()?;
        let music = if v >= Version::V5 { self.r.int_byte_string()? } else { String::new() };
        let _copyright = self.r.int_byte_string()?;
        let tab = self.r.int_byte_string()?;
        let instructions = self.r.int_byte_string()?;
        let lines = self.r.count(self.r.remaining(), "notice lines")?;
        let mut notes = Vec::new();
        for _ in 0..lines {
            notes.push(self.r.int_byte_string()?);
        }
        if !instructions.is_empty() {
            notes.insert(0, instructions);
        }
        let author = [tab, music, words].into_iter().find(|s| !s.trim().is_empty()).unwrap_or_default();
        let meta = Meta { title, artist, album, author, notes: notes.join("\n").trim().to_string() };

        let feel = if v < Version::V5 && self.r.bool()? { TripletFeel::Triplet8th } else { TripletFeel::None };
        if v >= Version::V4 {
            self.lyrics()?;
        }
        if v >= Version::V5_1 {
            self.r.skip(4 + 4 + 11)?; // master volume, a reserved int, 11-band equalizer
        }
        if v >= Version::V5 {
            self.r.skip(7 * 4 + 2)?; // page size, margins, score proportion, header/footer flags
            for _ in 0..10 {
                self.r.int_byte_string()?; // page header and footer templates
            }
        }
        let tempo = if v >= Version::V5 {
            self.r.int_byte_string()?; // tempo name
            let t = self.r.i32()?;
            if v >= Version::V5_1 {
                self.r.skip(1)?; // hide tempo
            }
            t
        } else {
            self.r.i32()?
        };
        let key = match v {
            Version::V3 => self.r.i32()?,
            Version::V4 => {
                let k = self.r.i32()?;
                self.r.skip(1)?; // octave
                k
            }
            _ => {
                let k = self.r.i8()?;
                self.r.skip(4)?; // octave
                i32::from(k)
            }
        };
        let channels = self.channels()?;
        let directions = if v >= Version::V5 {
            let mut d = [0i16; 19];
            for slot in &mut d {
                *slot = self.r.i16()?;
            }
            self.r.skip(4)?; // master reverb
            Some(d)
        } else {
            None
        };
        let measure_count = self.r.count(self.r.remaining(), "measure count")?;
        let track_count = self.r.count(self.r.remaining() / 64, "track count")?;
        if measure_count == 0 || track_count == 0 {
            return Err(damaged("the file has no measures or no tracks"));
        }

        let key = i8::try_from(key.clamp(-7, 7)).unwrap_or(0);
        let mut headers = self.headers(measure_count, key, feel)?;
        if let Some(d) = directions {
            apply_directions(&mut headers, &d);
        }
        let mut tracks = Vec::new();
        let mut states = Vec::new();
        for i in 0..track_count {
            let (track, state) = self.track(i, &channels, &headers)?;
            tracks.push(track);
            states.push(state);
        }
        self.r.skip(match v {
            Version::V5 => 2,
            Version::V5_1 => 1,
            _ => 0,
        })?;

        let mut tempo_changes = Vec::new();
        let keep = measure_count.min(MAX_MEASURES);
        for (mi, header) in headers.iter().enumerate().take(keep) {
            for (ti, (track, state)) in tracks.iter_mut().zip(&mut states).enumerate() {
                let measure = self.measure(state, header, mi, ti, &mut tempo_changes)?;
                if let Some(m) = track.measures.get_mut(mi) {
                    *m = measure;
                }
            }
        }
        if measure_count > keep {
            self.warnings.push(format!("only the first {MAX_MEASURES} measures were imported"));
            headers.truncate(keep);
            for t in &mut tracks {
                t.measures.truncate(keep);
            }
        }

        for (track, state) in tracks.iter_mut().zip(&states) {
            if let Some(p) = state.program.filter(|_| !state.drums) {
                track.program = p;
            }
        }
        let mut score = Score { meta, tempo: clamp_tempo(tempo).unwrap_or(DEFAULT_TEMPO), headers, tracks };
        self.apply_tempo_changes(&mut score, &tempo_changes);
        if score.tracks.len() > MAX_TRACKS {
            let extra = score.tracks.len() - MAX_TRACKS;
            self.warnings.push(format!("{extra} tracks skipped: at most {MAX_TRACKS} tracks are supported"));
            score.tracks.truncate(MAX_TRACKS);
        }
        Ok(score)
    }

    fn lyrics(&mut self) -> Result<(), GpError> {
        self.r.i32()?; // the track the lyrics belong to
        for _ in 0..5 {
            self.r.i32()?; // first measure
            if !self.r.int_string()?.trim().is_empty() {
                self.drop("lyrics lines");
            }
        }
        Ok(())
    }

    fn channels(&mut self) -> Result<[Channel; 64], GpError> {
        let mut out = [Channel::default(); 64];
        for c in &mut out {
            c.program = self.r.i32()?;
            c.volume = self.r.u8()?;
            c.balance = self.r.u8()?;
            self.r.skip(4 + 2)?; // chorus, reverb, phaser, tremolo; two blank bytes
        }
        Ok(out)
    }

    fn headers(&mut self, count: usize, mut key: i8, feel: TripletFeel) -> Result<Vec<MeasureHeader>, GpError> {
        let v = self.v;
        let mut out: Vec<MeasureHeader> = Vec::new();
        let mut ts = TimeSignature::COMMON;
        for i in 0..count {
            if v >= Version::V5 && i > 0 {
                self.r.skip(1)?;
            }
            let flags = self.r.u8()?;
            let mut num = ts.numerator;
            let mut den = ts.denominator;
            if flags & 0x01 != 0 {
                num = self.r.u8()?;
            }
            if flags & 0x02 != 0 {
                den = self.r.u8()?;
            }
            match TimeSignature::new(num, den) {
                Some(t) => ts = t,
                None => self.warnings.push(format!("measure {}: time signature {num}/{den} is not supported; kept {}/{}", i + 1, ts.numerator, ts.denominator)),
            }
            let mut h = MeasureHeader::new(ts);
            h.repeat_open = flags & 0x04 != 0;
            if flags & 0x08 != 0 {
                let n = self.r.i8()?;
                // Guitar Pro 5 counts every play; 3 and 4 count the repeats.
                let extra = if v >= Version::V5 { n.saturating_sub(1) } else { n };
                h.repeat_close = u8::try_from(extra).unwrap_or(1).max(1);
            }
            let mut ending = 0;
            if v < Version::V5 && flags & 0x10 != 0 {
                ending = self.r.u8()?;
            }
            if flags & 0x20 != 0 {
                let name = self.r.int_byte_string()?;
                self.r.skip(4)?; // colour
                h.marker = Some(name.trim().to_string()).filter(|n| !n.is_empty());
            }
            if flags & 0x40 != 0 {
                key = self.r.i8()?.clamp(-7, 7);
                self.r.skip(1)?; // major or minor
            }
            h.key = key;
            h.double_bar = flags & 0x80 != 0;
            if v >= Version::V5 {
                if flags & 0x03 != 0 {
                    self.r.skip(4)?; // beaming groups
                }
                let endings = self.r.u8()?;
                if flags & 0x10 != 0 {
                    h.endings = endings;
                }
                h.triplet_feel = match self.r.u8()? {
                    1 => TripletFeel::Triplet8th,
                    2 => TripletFeel::Triplet16th,
                    _ => TripletFeel::None,
                };
            } else {
                h.triplet_feel = feel;
                if flags & 0x10 != 0 {
                    h.endings = legacy_endings(&out, ending);
                }
            }
            out.push(h);
        }
        Ok(out)
    }

    fn track(&mut self, index: usize, channels: &[Channel; 64], headers: &[MeasureHeader]) -> Result<(Track, TrackState), GpError> {
        let v = self.v;
        if v == Version::V5 || (v == Version::V5_1 && index == 0) {
            self.r.skip(1)?;
        }
        let flags = self.r.u8()?;
        let name = self.r.fixed_string(40)?;
        let strings = self.r.count(7, "string count")?;
        let mut pitches = Vec::new();
        for i in 0..7 {
            let p = self.r.i32()?;
            if i < strings {
                pitches.push(u8::try_from(p.clamp(0, 127)).unwrap_or(0));
            }
        }
        let port = self.r.i32()?;
        let channel = self.r.i32()?;
        self.r.skip(4 + 4)?; // effect channel, fret count
        let capo = self.r.i32()?;
        self.r.skip(4)?; // colour
        if v >= Version::V5 {
            // Display flags, auto-accentuation, MIDI bank and the RSE instrument and equalizer.
            self.r.skip(if v == Version::V5 { 44 } else { 49 })?;
            if v >= Version::V5_1 {
                self.r.int_byte_string()?; // RSE effect
                self.r.int_byte_string()?; // RSE effect category
            }
        }
        let name = if name.trim().is_empty() { format!("Track {}", index + 1) } else { name.trim().to_string() };
        let drums = flags & 0x01 != 0 || channel == 10;
        let ch = port
            .checked_sub(1)
            .and_then(|p| p.checked_mul(16))
            .and_then(|p| p.checked_add(channel.checked_sub(1)?))
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| channels.get(i))
            .copied()
            .unwrap_or_default();
        let mut track = if drums {
            Track::drums(&name, headers)
        } else {
            let tuning = if pitches.is_empty() {
                self.warnings.push(format!("track `{name}` has no strings; standard tuning used"));
                Tuning::standard()
            } else {
                let preset = Tuning::presets().into_iter().find(|t| t.strings == pitches).map_or_else(|| "Custom".to_string(), |t| t.name);
                Tuning::new(preset, pitches).unwrap_or_else(|_| Tuning::standard())
            };
            let mut t = Track::new(&name, tuning, headers);
            t.capo = u8::try_from(capo.clamp(0, 24)).unwrap_or(0);
            t.program = u8::try_from(ch.program.clamp(0, 127)).unwrap_or(t.program);
            t
        };
        // Volume and balance are stored in steps of 8 (0..=16).
        track.volume = u8::try_from((u16::from(ch.volume) * 8).min(127)).unwrap_or(127);
        track.pan = i8::try_from((i16::from(ch.balance) * 8 - 64).clamp(-63, 63)).unwrap_or(0);
        if v >= Version::V5 {
            track.solo = flags & 0x10 != 0;
            track.muted = flags & 0x20 != 0;
        }
        let state = TrackState { strings: if drums { 7 } else { track.string_count() }, drums, last_fret: [[None; 7]; 2], program: None };
        Ok((track, state))
    }

    fn measure(&mut self, t: &mut TrackState, header: &MeasureHeader, mi: usize, ti: usize, tempos: &mut Vec<(usize, u32, i32)>) -> Result<Measure, GpError> {
        let voice_count = if self.v >= Version::V5 { 2 } else { 1 };
        let mut voices = Vec::new();
        for vi in 0..voice_count {
            let voice = self.voice(t, header, vi, mi, ti, tempos)?;
            if vi == 0 || voice.beats.iter().any(|b| !b.rest) {
                voices.push(voice);
            }
        }
        if self.v >= Version::V5 && self.r.remaining() > 0 {
            self.r.skip(1)?; // line break; absent after the last measure
        }
        if voices.first().is_none_or(|v| v.beats.is_empty()) {
            let empty = Measure::empty_for(header);
            match voices.first_mut() {
                Some(v) => *v = empty.voices.into_iter().next().unwrap_or_default(),
                None => voices = empty.voices,
            }
        }
        Ok(Measure { voices })
    }

    fn voice(
        &mut self,
        t: &mut TrackState,
        header: &MeasureHeader,
        vi: usize,
        mi: usize,
        ti: usize,
        tempos: &mut Vec<(usize, u32, i32)>,
    ) -> Result<Voice, GpError> {
        let count = self.r.count(self.r.remaining() / 3, "beat count")?;
        let cap = header.capacity();
        let mut out = Voice::default();
        let mut used = 0u32;
        let mut overflow = false;
        for _ in 0..count {
            let (beat, mix) = self.beat(t, vi)?;
            if let Some(mix) = mix {
                if mix.tempo > 0 {
                    tempos.push((mi, used, mix.tempo));
                }
                if let Ok(program) = u8::try_from(mix.instrument) {
                    if (mi, used, vi) == (0, 0, 0) {
                        t.program = Some(program.min(127));
                    } else {
                        self.drop("instrument changes inside a track");
                    }
                }
            }
            let ticks = beat.duration.ticks();
            if overflow || used + ticks > cap {
                overflow = true;
                continue;
            }
            used += ticks;
            out.beats.push(beat);
        }
        if overflow {
            self.warnings.push(format!("track {} measure {}: beats overflow the bar and were trimmed", ti + 1, mi + 1));
        }
        Ok(out)
    }

    fn beat(&mut self, t: &mut TrackState, vi: usize) -> Result<(Beat, Option<Mix>), GpError> {
        let v = self.v;
        let flags = self.r.u8()?;
        // 0 is an empty beat, 2 a rest.
        let status = if flags & 0x40 != 0 { self.r.u8()? } else { 1 };
        let value = match self.r.i8()? {
            -2 => NoteValue::Whole,
            -1 => NoteValue::Half,
            0 => NoteValue::Quarter,
            1 => NoteValue::Eighth,
            2 => NoteValue::Sixteenth,
            3 => NoteValue::ThirtySecond,
            4 => NoteValue::SixtyFourth,
            other => return Err(damaged(format!("unknown note value {other}"))),
        };
        let dots = u8::from(flags & 0x01 != 0);
        let tuplet = if flags & 0x20 != 0 { tuplet_of(self.r.i32()?) } else { (1, 1) };
        let mut beat = Beat::rest(Duration { value, dots, tuplet });
        if flags & 0x02 != 0 {
            beat.chord = self.chord()?;
            if let Some(c) = &mut beat.chord {
                // Diagrams always list seven strings; a drum chord is only a name.
                c.frets.truncate(if t.drums { 0 } else { t.strings });
                if c.frets.iter().all(Option::is_none) {
                    c.frets.clear();
                }
            }
            beat.chord = beat.chord.take().filter(|c| !c.name.is_empty() || !c.frets.is_empty());
        }
        if flags & 0x04 != 0 {
            beat.text = Some(self.r.int_byte_string()?.trim().to_string()).filter(|s| !s.is_empty());
        }
        let fx = if flags & 0x08 != 0 { self.beat_effects(&mut beat)? } else { BeatFx::default() };
        let mix = if flags & 0x10 != 0 { Some(self.mix_table()?) } else { None };
        let strings = self.r.u8()?;
        let mut dynamic = None;
        for bit in (0..7u8).rev() {
            if strings & (1 << bit) == 0 {
                continue;
            }
            let string = usize::from(6 - bit);
            let (note, velocity) = self.note(t, vi, string, &fx)?;
            dynamic = dynamic.or(velocity);
            let Some(note) = note else { continue };
            if beat.notes.iter().any(|n| n.string == note.string) {
                self.drop("drum hits sharing a lane");
                continue;
            }
            beat.notes.push(note);
        }
        if v >= Version::V5 {
            let flags2 = self.r.i16()?;
            if flags2 & 0x0800 != 0 {
                self.r.skip(1)?; // secondary beam break
            }
        }
        beat.notes.sort_by_key(|n| n.string);
        beat.rest = status != 1 || beat.notes.is_empty();
        if beat.rest {
            beat.notes.clear();
        }
        // Notes without a velocity play forte in Guitar Pro 3-5; a rest has no dynamic of its own.
        beat.dynamic = if beat.rest { Dynamic::default() } else { dynamic.map_or(Dynamic::F, dynamic_of) };
        Ok((beat, mix))
    }

    /// A chord diagram: kept as its name and frets.
    fn chord(&mut self) -> Result<Option<Chord>, GpError> {
        let v = self.v;
        let new_format = v >= Version::V5 || self.r.u8()? & 0x01 != 0;
        if v >= Version::V5 {
            self.r.skip(1)?; // format byte
        }
        if !new_format {
            let name = self.r.int_byte_string()?;
            let first_fret = self.r.i32()?;
            let mut frets = Vec::new();
            if first_fret != 0 {
                for _ in 0..6 {
                    frets.push(self.r.i32()?);
                }
            }
            return Ok(chord_of(name, first_fret, &frets));
        }
        let (name, first_fret, frets) = if v == Version::V3 {
            self.r.skip(25)?; // sharp, root, type, extension, bass, tonality, added note
            let name = self.r.fixed_string(22)?;
            self.r.skip(12)?; // fifth, ninth and eleventh alterations
            let first = self.r.i32()?;
            let mut frets = Vec::new();
            for _ in 0..6 {
                frets.push(self.r.i32()?);
            }
            self.r.skip(36)?; // barres, omissions
            (name, first, frets)
        } else {
            self.r.skip(16)?; // sharp, root, type, extension, bass, tonality, added note
            let name = self.r.fixed_string(21)?;
            self.r.skip(4)?; // fifth, ninth and eleventh alterations
            let first = self.r.i32()?;
            let mut frets = Vec::new();
            for _ in 0..7 {
                frets.push(self.r.i32()?);
            }
            self.r.skip(32)?; // barres, omissions, fingering
            (name, first, frets)
        };
        Ok(chord_of(name, first_fret, &frets))
    }

    fn beat_effects(&mut self, beat: &mut Beat) -> Result<BeatFx, GpError> {
        let v = self.v;
        let mut fx = BeatFx::default();
        let f1 = self.r.u8()?;
        let f2 = if v >= Version::V4 { self.r.u8()? } else { 0 };
        if v == Version::V3 {
            fx.vibrato = f1 & 0x01 != 0;
            if f1 & 0x04 != 0 {
                fx.harmonic = Some(Harmonic::Natural);
            }
            if f1 & 0x08 != 0 {
                fx.harmonic = Some(Harmonic::Artificial);
            }
        }
        fx.wide_vibrato = f1 & 0x02 != 0;
        if f1 & 0x10 != 0 {
            beat.fade = Some(Fade::In);
        }
        if f1 & 0x20 != 0 {
            let kind = self.r.u8()?;
            match kind {
                1 => fx.tapping = true,
                2 => beat.technique = Some(BeatTechnique::Slap),
                3 => beat.technique = Some(BeatTechnique::Pop),
                _ => {}
            }
            if v == Version::V3 {
                // The tremolo bar's depth when `kind` is 0; unused otherwise.
                if self.r.i32()? != 0 && kind == 0 {
                    self.drop("tremolo bar dives");
                }
            }
        }
        if v >= Version::V4 && f2 & 0x04 != 0 {
            self.bend()?;
            self.drop("tremolo bar dives");
        }
        if f1 & 0x40 != 0 {
            let (a, b) = (self.r.i8()?, self.r.i8()?);
            // Guitar Pro 3 and 4 write the down-stroke speed first; 5 writes the up-stroke first.
            let (down, up) = if v >= Version::V5 { (b, a) } else { (a, b) };
            if up > 0 {
                beat.brush = Some(Brush { kind: BrushKind::Brush, down: false, ticks: stroke_ticks(up) });
            } else if down > 0 {
                beat.brush = Some(Brush { kind: BrushKind::Brush, down: true, ticks: stroke_ticks(down) });
            }
        }
        if v >= Version::V4 {
            if f2 & 0x01 != 0 && beat.brush.is_none() {
                beat.brush = Some(Brush { kind: BrushKind::Rasgueado, down: true, ticks: TICKS_PER_QUARTER / 8 });
            }
            if f2 & 0x02 != 0 {
                beat.stroke = match self.r.i8()? {
                    1 => Some(StrokeDirection::Up),
                    2 => Some(StrokeDirection::Down),
                    _ => None,
                };
            }
        }
        Ok(fx)
    }

    fn mix_table(&mut self) -> Result<Mix, GpError> {
        let v = self.v;
        let instrument = self.r.i8()?;
        if v >= Version::V5 {
            self.r.skip(16)?; // RSE instrument
        }
        let mut changes = 0;
        for _ in 0..6 {
            // volume, balance, chorus, reverb, phaser, tremolo: -1 is no change
            if self.r.i8()? >= 0 {
                changes += 1;
            }
        }
        if v >= Version::V5 {
            self.r.int_byte_string()?; // tempo name
        }
        let tempo = self.r.i32()?;
        self.r.skip(changes)?; // transition lengths
        if tempo >= 0 {
            self.r.skip(if v >= Version::V5_1 { 2 } else { 1 })?; // transition, hide tempo
        }
        if v >= Version::V4 {
            self.r.skip(1)?; // apply-to-all-tracks flags
        }
        if v >= Version::V5 {
            self.r.skip(1)?; // wah
            if v >= Version::V5_1 {
                self.r.int_byte_string()?; // RSE effect
                self.r.int_byte_string()?; // RSE effect category
            }
        }
        Ok(Mix { instrument, tempo })
    }

    fn note(&mut self, t: &mut TrackState, vi: usize, string: usize, fx: &BeatFx) -> Result<(Option<Note>, Option<i8>), GpError> {
        let v = self.v;
        let flags = self.r.u8()?;
        let kind = if flags & 0x20 != 0 { self.r.u8()? } else { 1 };
        if v < Version::V5 && flags & 0x01 != 0 {
            self.r.skip(2)?; // the note's own duration and tuplet
        }
        let velocity = if flags & 0x10 != 0 { Some(self.r.i8()?) } else { None };
        let fret = if flags & 0x20 != 0 { self.r.i8()? } else { 0 };
        if flags & 0x80 != 0 {
            self.r.skip(2)?;
            self.drop("fingerings");
        }
        if v >= Version::V5 {
            if flags & 0x01 != 0 {
                self.r.skip(8)?; // duration percent
            }
            self.r.skip(1)?; // accidental spelling
        }
        let mut note = Note::new(0, 0);
        let e = &mut note.effects;
        e.ghost = flags & 0x04 != 0;
        e.accent = v >= Version::V4 && flags & 0x40 != 0;
        e.heavy_accent = v >= Version::V5 && flags & 0x02 != 0;
        e.vibrato = fx.vibrato;
        e.wide_vibrato = fx.wide_vibrato;
        e.harmonic = fx.harmonic;
        e.tapped = fx.tapping;
        if flags & 0x08 != 0 {
            self.note_effects(&mut note)?;
        }
        // A wide vibrato replaces the plain one: the two flags are never both on in the model.
        if note.effects.wide_vibrato {
            note.effects.vibrato = false;
        }
        let voice = t.last_fret.get_mut(vi.min(1)).and_then(|v| v.get_mut(string));
        let fret = u8::try_from(fret).unwrap_or(0);
        let fret = match kind {
            2 => {
                note.kind = NoteKind::Tie;
                voice.as_ref().and_then(|f| **f).unwrap_or(fret)
            }
            3 => {
                note.kind = NoteKind::Dead;
                fret
            }
            _ => fret,
        };
        if let Some(slot) = voice {
            *slot = Some(fret);
        }
        if t.drums {
            let midi = fret.min(guitario_theory::drums::MAX_DRUM);
            note.string = guitario_theory::drums::lane_for(midi);
            note.fret = midi;
            return Ok((Some(note), velocity));
        }
        if string >= t.strings {
            return Ok((None, velocity));
        }
        if fret > 36 {
            self.drop("frets above 36");
            return Ok((None, velocity));
        }
        note.string = u8::try_from(string).unwrap_or(0);
        note.fret = fret;
        Ok((Some(note), velocity))
    }

    fn note_effects(&mut self, note: &mut Note) -> Result<(), GpError> {
        let v = self.v;
        let f1 = self.r.u8()?;
        let f2 = if v >= Version::V4 { self.r.u8()? } else { 0 };
        note.effects.hammer = f1 & 0x02 != 0;
        note.effects.let_ring = f1 & 0x08 != 0;
        if f1 & 0x01 != 0 {
            note.effects.bend = self.bend()?;
        }
        if f1 & 0x10 != 0 {
            self.r.skip(if v >= Version::V5 { 5 } else { 4 })?; // fret, dynamic, transition, duration, flags
            self.drop("grace notes");
        }
        if v == Version::V3 {
            if f1 & 0x04 != 0 {
                note.effects.slide = Some(Slide::Shift);
            }
            return Ok(());
        }
        let e = &mut note.effects;
        e.staccato = f2 & 0x01 != 0;
        e.palm_mute = f2 & 0x02 != 0;
        e.vibrato |= f2 & 0x40 != 0;
        if f2 & 0x04 != 0 {
            self.r.skip(1)?;
            self.drop("tremolo picking marks");
        }
        if f2 & 0x08 != 0 {
            let s = self.r.i8()?;
            note.effects.slide = if v >= Version::V5 {
                match s as u8 {
                    f if f & 0x01 != 0 => Some(Slide::Shift),
                    f if f & 0x02 != 0 => Some(Slide::Legato),
                    f if f & 0x04 != 0 => Some(Slide::OutDown),
                    f if f & 0x08 != 0 => Some(Slide::OutUp),
                    f if f & 0x10 != 0 => Some(Slide::IntoFromBelow),
                    f if f & 0x20 != 0 => Some(Slide::IntoFromAbove),
                    _ => None,
                }
            } else {
                match s {
                    -2 => Some(Slide::IntoFromAbove),
                    -1 => Some(Slide::IntoFromBelow),
                    1 => Some(Slide::Shift),
                    2 => Some(Slide::Legato),
                    3 => Some(Slide::OutDown),
                    4 => Some(Slide::OutUp),
                    _ => None,
                }
            };
        }
        if f2 & 0x10 != 0 {
            let kind = self.r.i8()?;
            note.effects.harmonic = if v >= Version::V5 {
                match kind {
                    1 => Some(Harmonic::Natural),
                    2 => {
                        self.r.skip(3)?; // the harmonic's pitch: semitone, accidental, octave
                        Some(Harmonic::Artificial)
                    }
                    3 => {
                        self.r.skip(1)?; // the tapped fret
                        Some(Harmonic::Tapped)
                    }
                    4 => Some(Harmonic::Pinch),
                    5 => Some(Harmonic::Semi),
                    _ => None,
                }
            } else {
                match kind {
                    1 => Some(Harmonic::Natural),
                    3 => Some(Harmonic::Tapped),
                    4 => Some(Harmonic::Pinch),
                    5 => Some(Harmonic::Semi),
                    15 | 17 | 22 => Some(Harmonic::Artificial),
                    _ => None,
                }
            };
        }
        if f2 & 0x20 != 0 {
            self.r.skip(2)?; // trill fret and speed
            self.drop("trills");
        }
        Ok(())
    }

    /// A bend or tremolo-bar curve; returns its peak in quarter tones (Guitar Pro counts 25 per
    /// quarter tone).
    fn bend(&mut self) -> Result<Option<u8>, GpError> {
        self.r.skip(1 + 4)?; // kind, overall height
        let points = self.r.count(self.r.remaining() / 9, "bend points")?;
        let mut peak = 0i32;
        for _ in 0..points {
            self.r.skip(4)?; // position, 0..=60 across the note
            peak = peak.max(self.r.i32()?);
            self.r.skip(1)?; // vibrato
        }
        let quarters = u8::try_from(((peak.clamp(0, 300) + 12) / 25).min(12)).unwrap_or(0);
        Ok((quarters > 0).then_some(quarters))
    }

    /// Tempo changes from mix tables move to the nearest bar line: the model holds one tempo per bar.
    fn apply_tempo_changes(&mut self, score: &mut Score, changes: &[(usize, u32, i32)]) {
        let mut moved = 0;
        for &(mi, offset, bpm) in changes {
            let Some(bpm) = clamp_tempo(bpm) else { continue };
            let cap = score.headers.get(mi).map_or(0, MeasureHeader::capacity);
            let at = if offset == 0 {
                mi
            } else {
                moved += 1;
                if offset * 2 <= cap { mi } else { mi + 1 }
            };
            if at == 0 {
                score.tempo = bpm;
            } else if let Some(h) = score.headers.get_mut(at) {
                h.tempo = Some(bpm);
            }
        }
        if moved > 0 {
            self.warnings.push(format!("{moved} tempo changes inside a bar were moved to the nearest bar line"));
        }
        // A change that only restates the tempo in force is no change.
        let mut current = score.tempo;
        for h in &mut score.headers {
            match h.tempo {
                Some(t) if t == current => h.tempo = None,
                Some(t) => current = t,
                None => {}
            }
        }
    }
}

/// A chord from its name and Guitar Pro's diagram: absolute frets, highest string first, -1 not played.
fn chord_of(name: String, first_fret: i32, frets: &[i32]) -> Option<Chord> {
    let name = name.trim().to_string();
    let frets = frets.iter().map(|f| u8::try_from(*f).ok().filter(|f| *f <= 36)).collect();
    let chord = Chord { name, frets, first_fret: u8::try_from(first_fret.clamp(1, 36)).unwrap_or(1) };
    (!chord.name.is_empty() || chord.frets.iter().any(Option::is_some)).then_some(chord)
}

fn clamp_tempo(bpm: i32) -> Option<u16> {
    (bpm > 0).then(|| u16::try_from(bpm.clamp(20, 400)).unwrap_or(DEFAULT_TEMPO))
}

/// Guitar Pro 3 and 4 store an alternate ending as a number: the bar takes every ending up to it
/// that earlier bars since the last repeat start have not taken.
fn legacy_endings(before: &[MeasureHeader], number: u8) -> u8 {
    let mut taken = 0u8;
    for h in before.iter().rev() {
        taken |= h.endings;
        if h.repeat_open {
            break;
        }
    }
    let upto = if number >= 8 { u8::MAX } else { (1u8 << number) - 1 };
    upto & !taken
}

/// Guitar Pro 5 lists each direction once, with the measure (from 1) it sits on, or -1.
fn apply_directions(headers: &mut [MeasureHeader], d: &[i16; 19]) {
    use DirectionTarget as T;
    use JumpEnd as E;
    let targets = [T::Coda, T::DoubleCoda, T::Segno, T::SegnoSegno, T::Fine];
    let jumps = [
        Jump::DaCapo(E::End),
        Jump::DaCapo(E::Coda),
        Jump::DaCapo(E::DoubleCoda),
        Jump::DaCapo(E::Fine),
        Jump::DalSegno(E::End),
        Jump::DalSegno(E::Coda),
        Jump::DalSegno(E::DoubleCoda),
        Jump::DalSegno(E::Fine),
        Jump::DalSegnoSegno(E::End),
        Jump::DalSegnoSegno(E::Coda),
        Jump::DalSegnoSegno(E::DoubleCoda),
        Jump::DalSegnoSegno(E::Fine),
        Jump::ToCoda,
        Jump::ToDoubleCoda,
    ];
    let at = |m: i16| usize::try_from(m).ok().and_then(|m| m.checked_sub(1));
    for (target, m) in targets.iter().zip(d) {
        if let Some(h) = at(*m).and_then(|i| headers.get_mut(i))
            && !h.targets.contains(target)
        {
            h.targets.push(*target);
        }
    }
    for (jump, m) in jumps.iter().zip(d.iter().skip(targets.len())) {
        if let Some(h) = at(*m).and_then(|i| headers.get_mut(i)) {
            h.jump = Some(*jump);
        }
    }
}
