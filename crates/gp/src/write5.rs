//! Guitar Pro 5 (`.gp5`, version 5.10) export: the most widely read Guitar Pro format.
//!
//! The mirror of [`crate::legacy`]: every field the reader skips is written with the value Guitar
//! Pro writes when nothing is set. Anything the format cannot hold (more than seven strings or two
//! voices, double dots, fades out …) is dropped and counted in [`Export::warnings`].

use guitario_score::*;
use guitario_theory::{NoteValue, TICKS_PER_QUARTER};
use std::collections::BTreeMap;

/// The written file and what could not be represented in it.
#[derive(Debug)]
pub struct Export {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

const BANNER: &str = "FICHIER GUITAR PRO v5.10";
const MAX_STRINGS: usize = 7;
const DRUM_STRINGS: usize = 6;

/// Writes `score` as a Guitar Pro 5.10 file.
pub fn export_gp5(score: &Score) -> Export {
    let mut w = Writer { out: Vec::new(), dropped: BTreeMap::new() };
    w.song(score);
    for _ in score.tracks.iter().filter(|t| t.measures.iter().any(|m| m.volume.is_some() || m.pan.is_some())) {
        w.drop("volume and pan automation");
    }
    for _ in score.headers.iter().filter(|h| h.tempo.is_some() && h.tempo_linear) {
        w.drop("gradual tempo changes");
    }
    let warnings = w.dropped.iter().map(|(what, n)| format!("{what} cannot be written to Guitar Pro 5 and were dropped ({n})")).collect();
    Export { bytes: w.out, warnings }
}

struct Writer {
    out: Vec<u8>,
    dropped: BTreeMap<&'static str, usize>,
}

/// Windows-1252 bytes for `s`, at most `max` of them; characters it lacks become `?`.
fn encode(s: &str, max: usize) -> Vec<u8> {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™',
        'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    s.chars()
        .map(|c| match u32::from(c) {
            0..=0x7f | 0xa0..=0xff => u8::try_from(u32::from(c)).unwrap_or(b'?'),
            _ => HIGH.iter().position(|h| *h == c).and_then(|i| u8::try_from(0x80 + i).ok()).unwrap_or(b'?'),
        })
        .take(max)
        .collect()
}

impl Writer {
    fn drop(&mut self, what: &'static str) {
        *self.dropped.entry(what).or_default() += 1;
    }

    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }

    fn i8(&mut self, v: i8) {
        self.out.extend(v.to_le_bytes());
    }

    fn i16(&mut self, v: i16) {
        self.out.extend(v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.out.extend(v.to_le_bytes());
    }

    fn len(&mut self, n: usize) {
        self.i32(i32::try_from(n).unwrap_or(i32::MAX));
    }

    fn fill(&mut self, byte: u8, n: usize) {
        self.out.extend(std::iter::repeat_n(byte, n));
    }

    fn fixed_string(&mut self, s: &str, size: usize) {
        let b = encode(s, size.min(255));
        self.u8(u8::try_from(b.len()).unwrap_or(0));
        self.out.extend(&b);
        self.fill(0, size.saturating_sub(b.len()));
    }

    fn int_byte_string(&mut self, s: &str) {
        let b = encode(s, 254);
        self.len(b.len() + 1);
        self.u8(u8::try_from(b.len()).unwrap_or(0));
        self.out.extend(&b);
    }

    fn int_string(&mut self, s: &str) {
        let b = encode(s, 1 << 16);
        self.len(b.len());
        self.out.extend(&b);
    }
}

/// Each track's MIDI channel (0-based, port 1): drums on 10, the others in order around it.
fn channel_of(score: &Score) -> Vec<usize> {
    let mut free = (0..16).filter(|c| *c != 9);
    score.tracks.iter().map(|t| if t.is_drums() { 9 } else { free.next().unwrap_or(0) }).collect()
}

fn velocity_of(d: Dynamic) -> i8 {
    match d {
        Dynamic::Ppp => 1,
        Dynamic::Pp => 2,
        Dynamic::P => 3,
        Dynamic::Mp => 4,
        Dynamic::Mf => 5,
        Dynamic::F => 6,
        Dynamic::Ff => 7,
        Dynamic::Fff => 8,
    }
}

/// The tuplet Guitar Pro implies for `enters` notes: its "in the time of".
fn gp_times(enters: u8) -> Option<u8> {
    match enters {
        3 => Some(2),
        5..=7 => Some(4),
        9..=13 => Some(8),
        _ => None,
    }
}

/// The stroke speed (1 = a 128th … 6 = a quarter) closest to `ticks`.
fn stroke_speed(ticks: u32) -> i8 {
    (1..=6i8).min_by_key(|s| (TICKS_PER_QUARTER >> (6 - *s as u32)).abs_diff(ticks)).unwrap_or(4)
}

/// Eighth notes per beaming group, as Guitar Pro stores them (four groups).
/// Guitar Pro 5's "beam eighth notes by": four group sizes in eighths, repeated to fill the bar.
/// `groups` (in the time signature's note value) as those four, when they can say it exactly.
pub(crate) fn gp5_beams(ts: guitario_theory::TimeSignature, groups: &[u8]) -> Option<[u8; 4]> {
    let den = u32::from(ts.denominator.max(1));
    let eighths: Vec<u8> = groups
        .iter()
        .map(|g| {
            let e = u32::from(*g) * 8;
            (e % den == 0).then(|| u8::try_from(e / den).ok()).flatten().filter(|e| *e > 0)
        })
        .collect::<Option<_>>()?;
    let mut out = [0u8; 4];
    for (slot, e) in out.iter_mut().zip(&eighths) {
        *slot = *e;
    }
    // More than four groups survive only when the first four, repeated, give them back.
    (from_gp5_beams(ts, &out).as_deref() == Some(groups)).then_some(out)
}

/// The groups four Guitar Pro 5 sizes in eighths make in the time signature's note value, repeated
/// until they fill the bar; `None` when they cannot be counted in that value.
pub(crate) fn from_gp5_beams(ts: guitario_theory::TimeSignature, bytes: &[u8; 4]) -> Option<Vec<u8>> {
    let den = u32::from(ts.denominator.max(1));
    let units: Vec<u32> = bytes.iter().filter(|b| **b > 0).map(|b| (u32::from(*b) * den % 8 == 0).then_some(u32::from(*b) * den / 8)).collect::<Option<_>>()?;
    let total = u32::from(ts.numerator);
    if units.is_empty() || units.contains(&0) {
        return None;
    }
    let mut out = Vec::new();
    let mut sum = 0u32;
    for u in units.iter().cycle().take(MAX_BEAM_GROUPS) {
        if sum >= total {
            break;
        }
        let g = (*u).min(total - sum);
        out.push(u8::try_from(g).ok()?);
        sum += g;
    }
    Some(out)
}

/// The four sizes Guitar Pro 5 gets for a time signature's usual grouping (one beat each when that
/// cannot be said exactly), so a file written here reads back with the usual grouping.
pub(crate) fn default_gp5_beams(ts: guitario_theory::TimeSignature) -> [u8; 4] {
    gp5_beams(ts, &default_beaming(ts)).unwrap_or_else(|| {
        let per_beat = u8::try_from((8 / u32::from(ts.denominator.max(1))).max(1)).unwrap_or(1);
        let mut out = [0u8; 4];
        for g in out.iter_mut().take(usize::from(ts.numerator)) {
            *g = per_beat;
        }
        out
    })
}

impl Writer {
    fn song(&mut self, score: &Score) {
        self.fixed_string(BANNER, 30);
        let m = &score.meta;
        for s in [m.title.as_str(), &m.subtitle, &m.artist, &m.album, &m.words, &m.music, &m.copyright, &m.author, &m.instructions] {
            self.int_byte_string(s);
        }
        let notice: Vec<&str> = if m.notes.is_empty() { Vec::new() } else { m.notes.lines().collect() };
        self.len(notice.len());
        for l in notice {
            self.int_byte_string(l);
        }
        // Lyrics: the track, then five lines of (first measure, text), all 1-based. Guitar Pro 5
        // keeps one track's lyrics: the first track that has any.
        let mut with_lyrics = score.tracks.iter().enumerate().filter(|(_, t)| !t.lyrics.is_empty());
        let first = with_lyrics.next();
        if with_lyrics.next().is_some() {
            self.drop("lyrics on a second track");
        }
        self.i32(first.map_or(1, |(i, _)| i32::try_from(i + 1).unwrap_or(1)));
        for k in 0..MAX_LYRIC_LINES {
            let line = first.and_then(|(_, t)| t.lyrics.get(k));
            self.i32(line.map_or(1, |l| i32::try_from(l.measure + 1).unwrap_or(1)));
            self.int_string(line.map_or("", |l| l.text.as_str()));
        }
        // RSE master effect: volume, a reserved int, an 11-band equalizer.
        self.i32(100);
        self.i32(0);
        self.fill(0, 11);
        // Page setup: paper in millimetres, margins, score size, header/footer flags and templates.
        let (width, height, [left, right, top, bottom]) = crate::page::to_gp(&score.page);
        for v in [width, height, left, right, top, bottom, 100] {
            self.i32(v);
        }
        self.i16(0x01ff);
        for s in [
            "%TITLE%",
            "%SUBTITLE%",
            "%ARTIST%",
            "%ALBUM%",
            "Words by %WORDS%",
            "Music by %MUSIC%",
            "Words & Music by %WORDSMUSIC%",
            "Copyright %COPYRIGHT%",
            "",
            "Page %N%/%P%",
        ] {
            self.int_byte_string(s);
        }
        self.int_byte_string("");
        self.i32(i32::from(score.tempo));
        self.u8(0); // hide tempo
        self.i8(score.headers.first().map_or(0, |h| h.key));
        self.i32(0); // octave
        self.channels(score);
        self.directions(score);
        self.i32(0); // master reverb
        self.len(score.headers.len());
        self.len(score.tracks.len());
        self.headers(score);
        let channels = channel_of(score);
        for (i, t) in score.tracks.iter().enumerate() {
            self.track(i, t, channels.get(i).copied().unwrap_or(0));
        }
        self.u8(0);
        let last = (score.headers.len().saturating_sub(1), score.tracks.len().saturating_sub(1));
        for (mi, h) in score.headers.iter().enumerate() {
            for (ti, t) in score.tracks.iter().enumerate() {
                let measure = t.measures.get(mi);
                let tempo = if ti == 0 { h.tempo } else { None };
                self.measure(t, h, measure, tempo);
                if (mi, ti) != last {
                    self.u8(0); // line break
                }
            }
        }
    }

    fn channels(&mut self, score: &Score) {
        let mut table = [(25i32, 13u8, 8u8); 64];
        if let Some(c) = table.get_mut(9) {
            *c = (0, 13, 8);
        }
        let channels = channel_of(score);
        for (t, c) in score.tracks.iter().zip(&channels).rev() {
            if let Some(slot) = table.get_mut(*c) {
                // Volume and balance are stored in steps of 8 (0..=16).
                let balance = u8::try_from(((i16::from(t.pan) + 64 + 4) / 8).clamp(0, 16)).unwrap_or(8);
                *slot = (if t.is_drums() { 0 } else { i32::from(t.program) }, (t.volume.saturating_add(4) / 8).min(16), balance);
            }
        }
        for (program, volume, balance) in table {
            self.i32(program);
            self.u8(volume);
            self.u8(balance);
            self.fill(0, 4 + 2); // chorus, reverb, phaser, tremolo; blank
        }
    }

    /// Each direction once, at the first bar that has it (measures counted from 1, -1 for none).
    fn directions(&mut self, score: &Score) {
        use DirectionTarget as T;
        use JumpEnd as E;
        let first_target = |t: T| score.headers.iter().position(|h| h.targets.contains(&t));
        let first_jump = |j: Jump| score.headers.iter().position(|h| h.jump == Some(j));
        let slots = [T::Coda, T::DoubleCoda, T::Segno, T::SegnoSegno, T::Fine].map(first_target).into_iter().chain(
            [
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
            ]
            .map(first_jump),
        );
        let mut written = 0;
        for slot in slots {
            written += usize::from(slot.is_some());
            self.i16(slot.and_then(|m| i16::try_from(m + 1).ok()).unwrap_or(-1));
        }
        let total = score.headers.iter().map(|h| h.targets.len() + usize::from(h.jump.is_some())).sum::<usize>();
        for _ in written..total {
            self.drop("repeated direction marks");
        }
    }

    fn headers(&mut self, score: &Score) {
        let mut prev: Option<&MeasureHeader> = None;
        for (i, h) in score.headers.iter().enumerate() {
            if i > 0 {
                self.u8(0);
            }
            let ts = h.time_signature;
            let mut flags = 0u8;
            if prev.is_none_or(|p| p.time_signature.numerator != ts.numerator) {
                flags |= 0x01;
            }
            if prev.is_none_or(|p| p.time_signature.denominator != ts.denominator) {
                flags |= 0x02;
            }
            // The beaming is written with the time signature, so a change of beaming writes both.
            if prev.is_some_and(|p| p.beaming() != h.beaming()) {
                flags |= 0x03;
            }
            if h.repeat_open {
                flags |= 0x04;
            }
            if h.repeat_close > 0 {
                flags |= 0x08;
            }
            if h.endings != 0 {
                flags |= 0x10;
            }
            if h.marker.is_some() {
                flags |= 0x20;
            }
            if prev.map_or(h.key != 0, |p| p.key != h.key) {
                flags |= 0x40;
            }
            if h.double_bar {
                flags |= 0x80;
            }
            self.u8(flags);
            if flags & 0x01 != 0 {
                self.u8(ts.numerator);
            }
            if flags & 0x02 != 0 {
                self.u8(ts.denominator);
            }
            if flags & 0x08 != 0 {
                // Guitar Pro 5 counts every play.
                self.i8(i8::try_from(u16::from(h.repeat_close) + 1).unwrap_or(i8::MAX));
            }
            if let Some(marker) = &h.marker {
                self.int_byte_string(marker);
                self.out.extend([0xff, 0, 0, 0]); // red
            }
            if flags & 0x40 != 0 {
                self.i8(h.key);
                self.u8(0); // major
            }
            if flags & 0x03 != 0 {
                let b = if h.beams.is_empty() {
                    default_gp5_beams(ts)
                } else {
                    gp5_beams(ts, &h.beams).unwrap_or_else(|| {
                        self.drop("beam groupings");
                        default_gp5_beams(ts)
                    })
                };
                self.out.extend(b);
            }
            self.u8(h.endings);
            let feel = match h.triplet_feel {
                TripletFeel::None => 0,
                TripletFeel::Triplet8th => 1,
                TripletFeel::Triplet16th => 2,
                _ => {
                    self.drop("dotted and Scottish triplet feels");
                    0
                }
            };
            self.u8(feel);
            if h.free_time {
                self.drop("free-time bars");
            }
            if !h.fermatas.is_empty() {
                self.drop("fermatas");
            }
            prev = Some(h);
        }
    }

    fn track(&mut self, index: usize, t: &Track, channel: usize) {
        if index == 0 {
            self.u8(0);
        }
        let mut flags = 0x08; // visible
        if t.is_drums() {
            flags |= 0x01;
        }
        if t.solo {
            flags |= 0x10;
        }
        if t.muted {
            flags |= 0x20;
        }
        self.u8(flags);
        self.fixed_string(&t.name, 40);
        let strings = if t.is_drums() { DRUM_STRINGS } else { t.string_count().min(MAX_STRINGS) };
        self.len(strings);
        for i in 0..MAX_STRINGS {
            let pitch = if t.is_drums() || i >= strings { 0 } else { t.tuning.strings.get(i).copied().unwrap_or(0) };
            self.i32(i32::from(pitch));
        }
        let channel = i32::try_from(channel + 1).unwrap_or(1);
        self.i32(1); // port
        self.i32(channel);
        self.i32(channel); // effect channel
        self.i32(24); // frets
        self.i32(if t.is_drums() { 0 } else { i32::from(t.capo) });
        let [r, g, b] = t.color_at(index);
        self.out.extend([r, g, b, 0]);
        // Display flags, auto-accentuation, MIDI bank, RSE humanize and reserved fields, the RSE
        // instrument (none) and its equalizer: the values Guitar Pro writes for a new track.
        self.i16(0x0143);
        self.fill(0, 3);
        self.i32(0);
        self.i32(0);
        self.i32(100);
        self.out.extend([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0xff, 3]);
        self.fill(0xff, 16);
        self.fill(0, 4);
        self.int_byte_string("");
        self.int_byte_string("");
        if t.string_count() > MAX_STRINGS && !t.is_drums() {
            self.drop("strings beyond the seventh");
        }
    }

    fn measure(&mut self, t: &Track, h: &MeasureHeader, measure: Option<&Measure>, tempo: Option<u16>) {
        let voices = measure.map(|m| m.voices.as_slice()).unwrap_or_default();
        for vi in 0..2 {
            match voices.get(vi).filter(|v| !v.beats.is_empty()) {
                Some(v) => {
                    self.len(v.beats.len());
                    for (bi, beat) in v.beats.iter().enumerate() {
                        let mix = if vi == 0 && bi == 0 { tempo } else { None };
                        self.beat(t, beat, mix);
                    }
                }
                None if vi == 0 => {
                    // A bar-long rest.
                    let empty = Measure::empty_for(h);
                    let rests = empty.voices.first().map(|v| v.beats.as_slice()).unwrap_or_default();
                    self.len(rests.len());
                    for (bi, beat) in rests.iter().enumerate() {
                        self.beat(t, beat, if bi == 0 { tempo } else { None });
                    }
                }
                None => {
                    // An empty second voice: one empty beat.
                    self.i32(1);
                    self.out.extend([0x40, 0, 0, 0]);
                    self.i16(0);
                }
            }
        }
        if voices.len() > 2 {
            self.drop("third and fourth voices");
        }
    }

    fn beat(&mut self, t: &Track, b: &Beat, tempo: Option<u16>) {
        let d = b.duration;
        let notes: Vec<(usize, &Note)> = if t.is_drums() {
            // Hits go on the drum track's strings in lane order; the fret is the drum number.
            let mut hits: Vec<&Note> = b.notes.iter().collect();
            hits.sort_by_key(|n| n.string);
            if hits.len() > DRUM_STRINGS {
                self.drop("drum hits beyond six at once");
            }
            hits.into_iter().take(DRUM_STRINGS).enumerate().collect()
        } else {
            let mut keep = Vec::new();
            for n in &b.notes {
                if usize::from(n.string) < MAX_STRINGS {
                    keep.push((usize::from(n.string), n));
                } else {
                    self.drop("notes on strings beyond the seventh");
                }
            }
            keep
        };
        let rest = b.rest || notes.is_empty();
        let mut flags = 0u8;
        if d.dots > 0 {
            flags |= 0x01;
        }
        if d.dots > 1 {
            self.drop("double dots");
        }
        if b.chord.is_some() {
            flags |= 0x02;
        }
        if b.text.is_some() {
            flags |= 0x04;
        }
        let fx = beat_fx(b, &notes);
        if fx.any() {
            flags |= 0x08;
        }
        if tempo.is_some() {
            flags |= 0x10;
        }
        let tuplet = d.tuplet.0 > 1 && d.tuplet != (1, 1);
        if tuplet {
            flags |= 0x20;
            if gp_times(d.tuplet.0) != Some(d.tuplet.1) {
                self.drop("tuplets Guitar Pro 5 cannot express");
            }
        }
        if rest {
            flags |= 0x40;
        }
        self.u8(flags);
        if rest {
            self.u8(2);
        }
        self.i8(match d.value {
            NoteValue::Whole => -2,
            NoteValue::Half => -1,
            NoteValue::Quarter => 0,
            NoteValue::Eighth => 1,
            NoteValue::Sixteenth => 2,
            NoteValue::ThirtySecond => 3,
            NoteValue::SixtyFourth => 4,
        });
        if tuplet {
            self.i32(i32::from(d.tuplet.0));
        }
        if let Some(c) = &b.chord {
            self.chord(c);
        }
        if let Some(text) = &b.text {
            self.int_byte_string(text);
        }
        if fx.any() {
            self.beat_effects(&fx);
        }
        if matches!(b.fade, Some(Fade::Out | Fade::Swell)) {
            self.drop("fades out and volume swells");
        }
        if let Some(bpm) = tempo {
            self.mix_table(bpm);
        }
        let mut mask = 0u8;
        if !rest {
            for (s, _) in &notes {
                mask |= 0x40 >> s;
            }
        }
        self.u8(mask);
        if !rest {
            let mut sorted = notes;
            sorted.sort_by_key(|(s, _)| *s);
            for (_, n) in sorted {
                self.note(n, b.dynamic, b.tremolo);
            }
        }
        self.i16(0);
    }

    fn chord(&mut self, c: &Chord) {
        self.u8(1); // new format
        self.fill(0, 16); // sharp, root, type, extension, bass, tonality, added note
        self.fixed_string(&c.name, 21);
        self.fill(0, 4); // fifth, ninth and eleventh alterations
        self.i32(if c.frets.is_empty() { 0 } else { i32::from(c.first_fret) });
        for i in 0..MAX_STRINGS {
            self.i32(c.frets.get(i).copied().flatten().map_or(-1, i32::from));
        }
        self.fill(0, 32); // barres, omissions, fingering
    }

    fn beat_effects(&mut self, fx: &Fx) {
        let mut f1 = 0u8;
        let mut f2 = 0u8;
        if fx.wide_vibrato {
            f1 |= 0x02;
        }
        if fx.fade_in {
            f1 |= 0x10;
        }
        if fx.slap.is_some() {
            f1 |= 0x20;
        }
        if fx.stroke.is_some() {
            f1 |= 0x40;
        }
        if fx.rasgueado {
            f2 |= 0x01;
        }
        if fx.pick.is_some() {
            f2 |= 0x02;
        }
        self.u8(f1);
        self.u8(f2);
        if let Some(kind) = fx.slap {
            self.u8(kind);
        }
        if let Some((down, speed)) = fx.stroke {
            // Guitar Pro 5 writes the up-stroke speed first.
            let (up, dn) = if down { (0, speed) } else { (speed, 0) };
            self.i8(up);
            self.i8(dn);
        }
        if let Some(s) = fx.pick {
            self.i8(match s {
                StrokeDirection::Up => 1,
                StrokeDirection::Down => 2,
            });
        }
    }

    fn mix_table(&mut self, bpm: u16) {
        self.i8(-1); // instrument
        self.fill(0xff, 16); // RSE instrument
        self.fill(0xff, 6); // volume, balance, chorus, reverb, phaser, tremolo: unchanged
        self.int_byte_string(""); // tempo name
        self.i32(i32::from(bpm));
        self.u8(0); // transition
        self.u8(0); // hide tempo
        self.u8(0); // apply-to-all-tracks flags
        self.i8(-1); // wah
        self.int_byte_string("");
        self.int_byte_string("");
    }

    fn note(&mut self, n: &Note, dynamic: Dynamic, tremolo: Option<TremoloSpeed>) {
        let e = &n.effects;
        let has_fx = e.bend.is_some()
            || e.hammer
            || e.let_ring
            || e.staccato
            || e.palm_mute
            || e.slide.is_some()
            || e.harmonic.is_some()
            || e.vibrato
            || e.trill.is_some()
            || e.grace.is_some()
            || tremolo.is_some();
        let mut flags = 0x20u8;
        if e.heavy_accent {
            flags |= 0x02;
        }
        if e.ghost {
            flags |= 0x04;
        }
        if has_fx {
            flags |= 0x08;
        }
        if dynamic != Dynamic::F {
            flags |= 0x10;
        }
        if e.accent {
            flags |= 0x40;
        }
        if e.left_finger.is_some() || e.right_finger.is_some() {
            flags |= 0x80;
        }
        if e.left_hand_tapped {
            self.drop("left-hand tapping marks");
        }
        self.u8(flags);
        self.u8(match n.kind {
            NoteKind::Normal => 1,
            NoteKind::Tie => 2,
            NoteKind::Dead => 3,
        });
        if flags & 0x10 != 0 {
            self.i8(velocity_of(dynamic));
        }
        self.i8(i8::try_from(n.fret).unwrap_or(i8::MAX));
        if flags & 0x80 != 0 {
            let finger = |f: Option<Finger>| f.map_or(-1, |f| i8::try_from(f.index()).unwrap_or(-1));
            self.i8(finger(e.left_finger));
            self.i8(finger(e.right_finger));
        }
        self.u8(0); // accidental spelling
        if has_fx {
            self.note_effects(n, dynamic, tremolo);
        }
    }

    fn note_effects(&mut self, n: &Note, dynamic: Dynamic, tremolo: Option<TremoloSpeed>) {
        let e = &n.effects;
        let mut f1 = 0u8;
        let mut f2 = 0u8;
        if tremolo.is_some() {
            f2 |= 0x04;
        }
        if e.trill.is_some() {
            f2 |= 0x20;
        }
        if e.bend.is_some() {
            f1 |= 0x01;
        }
        if e.hammer {
            f1 |= 0x02;
        }
        if e.let_ring {
            f1 |= 0x08;
        }
        if e.grace.is_some() {
            f1 |= 0x10;
        }
        if e.staccato {
            f2 |= 0x01;
        }
        if e.palm_mute {
            f2 |= 0x02;
        }
        if e.slide.is_some() {
            f2 |= 0x08;
        }
        if e.harmonic.is_some() {
            f2 |= 0x10;
        }
        if e.vibrato {
            f2 |= 0x40;
        }
        self.u8(f1);
        self.u8(f2);
        if let Some(q) = e.bend {
            // A plain bend: up to the height by the middle of the note, held to its end.
            let height = i32::from(q) * 25;
            self.i8(1);
            self.i32(height);
            self.i32(3);
            for (pos, value) in [(0, 0), (30, height), (60, height)] {
                self.i32(pos);
                self.i32(value);
                self.u8(0);
            }
        }
        if let Some(g) = e.grace {
            self.u8(g.fret);
            self.i8(velocity_of(dynamic));
            self.u8(match g.transition {
                GraceTransition::None => 0,
                GraceTransition::Slide => 1,
                GraceTransition::Bend => 2,
                GraceTransition::Hammer => 3,
            });
            self.u8(match g.duration {
                GraceDuration::SixtyFourth => 1,
                GraceDuration::ThirtySecond => 2,
                GraceDuration::Sixteenth => 3,
            });
            self.u8(u8::from(g.dead) | u8::from(g.on_beat) << 1);
        }
        if let Some(speed) = tremolo {
            self.u8(speed.slashes());
        }
        if let Some(s) = e.slide {
            self.u8(match s {
                Slide::Shift => 0x01,
                Slide::Legato => 0x02,
                Slide::OutDown => 0x04,
                Slide::OutUp => 0x08,
                Slide::IntoFromBelow => 0x10,
                Slide::IntoFromAbove => 0x20,
            });
        }
        if let Some(h) = e.harmonic {
            match h {
                Harmonic::Natural => self.i8(1),
                Harmonic::Artificial => {
                    self.i8(2);
                    self.out.extend([0, 0, 1]); // semitone, accidental, octave
                }
                Harmonic::Tapped => {
                    self.i8(3);
                    self.u8(n.fret.saturating_add(12));
                }
                Harmonic::Pinch => self.i8(4),
                Harmonic::Semi => self.i8(5),
            }
        }
        if let Some(trill) = e.trill {
            self.u8(trill.fret);
            self.u8(match trill.speed {
                TrillSpeed::Sixteenth => 1,
                TrillSpeed::ThirtySecond => 2,
                TrillSpeed::SixtyFourth => 3,
            });
        }
    }
}

/// Beat effects as Guitar Pro 5 groups them.
struct Fx {
    wide_vibrato: bool,
    fade_in: bool,
    /// 1 tapping, 2 slap, 3 pop.
    slap: Option<u8>,
    /// A brush stroke: (down, speed 1..=6).
    stroke: Option<(bool, i8)>,
    rasgueado: bool,
    pick: Option<StrokeDirection>,
}

impl Fx {
    fn any(&self) -> bool {
        self.wide_vibrato || self.fade_in || self.slap.is_some() || self.stroke.is_some() || self.rasgueado || self.pick.is_some()
    }
}

fn beat_fx(b: &Beat, notes: &[(usize, &Note)]) -> Fx {
    let slap = match b.technique {
        Some(BeatTechnique::Slap) => Some(2),
        Some(BeatTechnique::Pop) => Some(3),
        None if notes.iter().any(|(_, n)| n.effects.tapped) => Some(1),
        None => None,
    };
    let (stroke, rasgueado) = match b.brush {
        Some(Brush { kind: BrushKind::Rasgueado, .. }) => (None, true),
        Some(br) => (Some((br.down, stroke_speed(br.ticks))), false),
        None => (None, false),
    };
    Fx { wide_vibrato: notes.iter().any(|(_, n)| n.effects.wide_vibrato), fade_in: b.fade == Some(Fade::In), slap, stroke, rasgueado, pick: b.stroke }
}
