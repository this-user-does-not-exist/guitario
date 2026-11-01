//! L2 Standard MIDI Files: export (format 1, repeats unrolled and ties merged) and import
//! ([`import`]).

mod import;

pub use import::{Import, MAX_FILE, finger, import, import_file, into_range, is_midi_path, range, tuning_for};

use guitario_score::{Score, ScoreError, playback_order, timeline};
use guitario_theory::TICKS_PER_QUARTER;

#[derive(Debug, thiserror::Error)]
pub enum MidiError {
    #[error("score has more than 15 tracks; MIDI channels run out")]
    TooManyTracks,
    #[error("not a MIDI file")]
    NotMidi,
    #[error("unsupported MIDI file: {0}")]
    Unsupported(String),
    #[error("damaged MIDI file: {0}")]
    Damaged(String),
    #[error("the file is too large")]
    TooLarge,
    #[error("could not read the file: {0}")]
    Io(String),
    #[error("imported score is invalid: {0}")]
    Invalid(#[from] ScoreError),
}

fn vlq(out: &mut Vec<u8>, mut v: u32) {
    let mut buf = [0u8; 5];
    let mut n = 0;
    loop {
        if let Some(b) = buf.get_mut(n) {
            *b = (v & 0x7f) as u8 | if n == 0 { 0 } else { 0x80 };
        }
        n += 1;
        v >>= 7;
        if v == 0 || n >= buf.len() {
            break;
        }
    }
    out.extend(buf.iter().take(n).rev());
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(tag);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
}

/// Events at absolute ticks, written as delta times. Sorted so offs precede ons at equal ticks.
fn write_events(mut events: Vec<(u64, u8, Vec<u8>)>) -> Vec<u8> {
    events.sort_by_key(|e| (e.0, e.1));
    let mut body = Vec::new();
    let mut last = 0u64;
    for (tick, _, data) in events {
        vlq(&mut body, (tick - last).min(u64::from(u32::MAX >> 4)) as u32);
        last = tick;
        body.extend(data);
    }
    vlq(&mut body, 0);
    body.extend([0xff, 0x2f, 0x00]);
    body
}

pub fn export(score: &Score) -> Result<Vec<u8>, MidiError> {
    if score.tracks.len() > 15 {
        return Err(MidiError::TooManyTracks);
    }
    let mut out = Vec::new();
    let mut head = Vec::new();
    head.extend(1u16.to_be_bytes());
    head.extend((score.tracks.len() as u16 + 1).to_be_bytes());
    head.extend((TICKS_PER_QUARTER as u16).to_be_bytes());
    chunk(&mut out, b"MThd", &head);

    // Tempo / time-signature track.
    let mut meta: Vec<(u64, u8, Vec<u8>)> = Vec::new();
    let mut clock = 0u64;
    let curves = score.tempo_curves();
    let mut key: Option<i8> = None;
    // The song's title names the tempo track, as sequencers do; an import reads it back.
    let title = score.meta.title.trim();
    if !title.is_empty() {
        let mut name = vec![0xff, 0x03];
        vlq(&mut name, title.len() as u32);
        name.extend(title.bytes());
        meta.push((0, 0, name));
    }
    for mi in playback_order(score) {
        let Some(h) = score.headers.get(mi) else { continue };
        let us = 60_000_000 / u32::from(score.tempo_at(mi).max(1));
        let tempo = |us: u32| vec![0xff, 0x51, 0x03, (us >> 16) as u8, (us >> 8) as u8, us as u8];
        meta.push((clock, 0, tempo(us)));
        // A tempo automation gliding through the bar: a step per quarter note, at its middle tempo.
        if let Some(&(b0, b1)) = curves.get(mi)
            && (b1 - b0).abs() > 1e-9
        {
            let cap = h.capacity().max(1);
            let mut at = 0u32;
            while at < cap {
                let mid = (f64::from(at) + f64::from(TICKS_PER_QUARTER.min(cap - at)) / 2.0) / f64::from(cap);
                let bpm = (b0 + (b1 - b0) * mid).max(1.0);
                let step = (60_000_000.0 / bpm).round().clamp(1.0, f64::from(0x00FF_FFFF)) as u32;
                meta.push((clock + u64::from(at), 0, tempo(step)));
                at = at.saturating_add(TICKS_PER_QUARTER);
            }
        }
        // A fermata slows the tempo for the beat it holds, then the bar's tempo comes back.
        for hold in score.holds(mi) {
            let slow = (f64::from(us) * hold.stretch).round().clamp(1.0, f64::from(0x00FF_FFFF)) as u32;
            meta.push((clock + u64::from(hold.at), 0, tempo(slow)));
            meta.push((clock + u64::from(hold.at) + u64::from(hold.ticks), 0, tempo(us)));
        }
        // The key signature where it changes (and at the start), as sharps or flats, major.
        if key != Some(h.key) {
            meta.push((clock, 0, vec![0xff, 0x59, 0x02, h.key.clamp(-7, 7) as u8, 0]));
            key = Some(h.key);
        }
        let den_pow = h.time_signature.denominator.max(1).ilog2() as u8;
        meta.push((clock, 0, vec![0xff, 0x58, 0x04, h.time_signature.numerator, den_pow, 24, 8]));
        clock += u64::from(h.capacity());
    }
    chunk(&mut out, b"MTrk", &write_events(meta));
    let song_end = clock;

    let notes = timeline(score);
    let any_solo = score.tracks.iter().any(|t| t.solo);
    for (ti, track) in score.tracks.iter().enumerate() {
        // General MIDI: percussion lives on channel 10 (index 9); pitched tracks skip it.
        let ch = if track.is_drums() {
            9
        } else {
            let pitched = score.tracks.iter().take(ti).filter(|t| !t.is_drums()).count() as u8;
            if pitched >= 9 { pitched + 1 } else { pitched }
        };
        let mut ev: Vec<(u64, u8, Vec<u8>)> = Vec::new();
        let mut name = vec![0xff, 0x03];
        vlq(&mut name, track.name.len() as u32);
        name.extend(track.name.bytes());
        ev.push((0, 0, name));
        ev.push((0, 1, vec![0xc0 | ch, track.program & 0x7f]));
        ev.push((0, 1, vec![0xb0 | ch, 7, track.volume & 0x7f]));
        ev.push((0, 1, vec![0xb0 | ch, 10, (i16::from(track.pan.clamp(-63, 63)) + 64) as u8]));
        // Volume and pan automation: a controller change where each bar starts, and one per
        // quarter note while a value glides.
        let (vol, pan) = score.mix_curves(ti);
        let mut clock = 0u64;
        for mi in playback_order(score) {
            let cap = score.headers.get(mi).map_or(0, |h| h.capacity());
            for (cc, curve, to_midi) in
                [(7u8, &vol, (|v: f64| v.round().clamp(0.0, 127.0) as u8) as fn(f64) -> u8), (10, &pan, |v: f64| (v.round().clamp(-63.0, 63.0) + 64.0) as u8)]
            {
                let Some(&Some((a, b))) = curve.get(mi) else { continue };
                let step = if (b - a).abs() > 1e-9 { TICKS_PER_QUARTER } else { cap.max(1) };
                let mut at = 0u32;
                while at < cap.max(1) {
                    let v = a + (b - a) * f64::from(at) / f64::from(cap.max(1));
                    ev.push((clock + u64::from(at), 1, vec![0xb0 | ch, cc, to_midi(v)]));
                    at = at.saturating_add(step);
                }
            }
            clock += u64::from(cap);
        }
        let silent = track.muted || (any_solo && !track.solo);
        if !silent {
            for n in notes.iter().filter(|n| n.track == ti) {
                let Some(p) = n.pitch.or_else(|| n.dead.then(|| track.tuning.pitch(usize::from(n.string), 0, 0)).flatten()) else {
                    continue;
                };
                let (vel, mut len) = if n.dead { (n.velocity / 2, 120) } else { (n.velocity, n.ticks) };
                // A staccato note sounds for half its length.
                if n.effects.staccato && !n.dead {
                    len = (len / 2).max(1);
                }
                // A harmonic sounds its partial; a semi-harmonic keeps the plain note under it.
                // A drum kit plays General MIDI: Guitar Pro's articulations sound as the drum they vary.
                let key = if track.is_drums() { guitario_theory::drums::plays(p.0) } else { n.harmonic.map_or(p.0, |h| h.key()) };
                let mut keys = vec![(key, vel)];
                if let Some(h) = n.harmonic.filter(|h| h.fundamental >= 0.5) {
                    keys.push((p.0, (f32::from(vel) * h.fundamental) as u8));
                }
                // Let ring holds the note until its string is played again (or past the end).
                let off = if n.effects.let_ring && !n.dead {
                    notes
                        .iter()
                        .filter(|m| m.track == ti && m.string == n.string && m.start_tick > n.start_tick)
                        .map(|m| m.start_tick)
                        .min()
                        .unwrap_or_else(|| song_end.max(n.start_tick + u64::from(len)) + u64::from(4 * TICKS_PER_QUARTER))
                } else {
                    n.start_tick + u64::from(len)
                };
                for (key, vel) in keys {
                    ev.push((n.start_tick, 3, vec![0x90 | ch, key, vel.max(1)]));
                    // Offs sort before ons at the same tick, so repeated notes retrigger cleanly.
                    ev.push((off, 2, vec![0x80 | ch, key, 0]));
                }
            }
        }
        chunk(&mut out, b"MTrk", &write_events(ev));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::{Beat, Note};
    use guitario_theory::Duration;

    fn one_note() -> Score {
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(0, 0)], ..Beat::rest(Duration::WHOLE) }];
        s
    }

    #[test]
    fn header_and_track_count() {
        let b = export(&one_note()).unwrap();
        assert_eq!(&b[0..4], b"MThd");
        assert_eq!(u16::from_be_bytes([b[10], b[11]]), 2);
        assert_eq!(u16::from_be_bytes([b[12], b[13]]), 20_160);
        assert_eq!(b.windows(4).filter(|w| *w == b"MTrk").count(), 2);
    }

    #[test]
    fn contains_note_on_for_open_e() {
        let b = export(&one_note()).unwrap();
        assert!(b.windows(3).any(|w| w == [0x90, 64, 80]));
        assert!(b.windows(3).any(|w| w == [0x80, 64, 0]));
    }

    #[test]
    fn muted_track_has_no_notes() {
        let mut s = one_note();
        s.tracks[0].muted = true;
        let b = export(&s).unwrap();
        assert!(!b.windows(2).any(|w| w == [0x90, 64]));
    }

    #[test]
    fn drums_go_to_channel_ten() {
        let mut s = one_note();
        s.tracks.push(guitario_score::Track::drums("Drums", &s.headers));
        s.tracks[1].measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(7, 36)], ..Beat::rest(Duration::WHOLE) }];
        let b = export(&s).unwrap();
        // Note on, channel 10 (0x99), kick 36.
        assert!(b.windows(2).any(|w| w == [0x99, 36]));
        // The guitar stays on channel 1.
        assert!(b.windows(2).any(|w| w == [0x90, 64]));
        // A choked crash and a rim shot are written as the crash and the snare.
        s.tracks[1].measures[0].voices[0].beats[0].notes = vec![Note::new(0, 97), Note::new(4, 91)];
        let b = export(&s).unwrap();
        assert!(b.windows(2).any(|w| w == [0x99, 49]) && b.windows(2).any(|w| w == [0x99, 38]));
        assert!(!b.windows(2).any(|w| w == [0x99, 97] || w == [0x99, 91]));
    }

    #[test]
    fn a_fermata_slows_the_tempo_for_its_beat_and_restores_it() {
        let mut s = one_note();
        let tempo_events = |b: &[u8]| b.windows(3).filter(|w| *w == [0xff, 0x51, 0x03]).count();
        let plain = export(&s).unwrap();
        s.headers[0].fermatas = vec![guitario_score::Fermata { tick: 0, kind: guitario_score::FermataKind::Medium, length: 0 }];
        let held = export(&s).unwrap();
        assert_eq!(tempo_events(&held), tempo_events(&plain) + 2);
        // 120 bpm is 500 000 us per quarter; held twice as long it is 1 000 000 (0x0F4240).
        assert!(held.windows(6).any(|w| w == [0xff, 0x51, 0x03, 0x0f, 0x42, 0x40]));
    }

    #[test]
    fn a_tempo_ramp_is_written_as_a_step_per_quarter() {
        let mut s = one_note();
        let tempo_events = |b: &[u8]| b.windows(3).filter(|w| *w == [0xff, 0x51, 0x03]).count();
        let h = s.headers[0].clone();
        s.headers.push(guitario_score::MeasureHeader { tempo: Some(180), tempo_linear: true, ..h.clone() });
        s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
        s.tempo = 60;
        let plain = {
            let mut j = s.clone();
            j.headers[1].tempo_linear = false;
            export(&j).unwrap()
        };
        let ramp = export(&s).unwrap();
        // Bar 1 glides from 60 to 180: four steps on top of the bar's own tempo event.
        assert_eq!(tempo_events(&ramp), tempo_events(&plain) + 4);
        // Its first quarter is at 75 bpm (the middle of 60..90): 800 000 us (0x0C3500).
        assert!(ramp.windows(6).any(|w| w == [0xff, 0x51, 0x03, 0x0c, 0x35, 0x00]));
    }

    #[test]
    fn volume_and_pan_automation_become_controller_changes() {
        let mut s = one_note();
        let h = s.headers[0].clone();
        s.headers.push(h.clone());
        s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
        s.tracks[0].measures[0].volume = Some(20);
        s.tracks[0].measures[1].volume = Some(100);
        s.tracks[0].measures[1].volume_linear = true;
        s.tracks[0].measures[1].pan = Some(-63);
        let b = export(&s).unwrap();
        let cc = |n: u8, v: u8| b.windows(3).any(|w| w == [0xb0, n, v]);
        // Bar 1 glides 20 -> 100 a quarter at a time (20, 40, 60, 80), then holds 100; pan hard left.
        assert!(cc(7, 20) && cc(7, 40) && cc(7, 60) && cc(7, 80) && cc(7, 100), "volume steps");
        assert!(cc(10, 1), "pan left");
    }

    /// Absolute ticks of each event matching `bytes` in the first note track.
    fn at(b: &[u8], bytes: [u8; 3]) -> Vec<u64> {
        let tracks: Vec<usize> = b.windows(4).enumerate().filter(|(_, w)| *w == b"MTrk").map(|(i, _)| i).collect();
        let start = tracks[1] + 8;
        let mut i = start;
        let (mut tick, mut out) = (0u64, Vec::new());
        while i < b.len() {
            let mut d = 0u64;
            loop {
                let x = b[i];
                i += 1;
                d = (d << 7) | u64::from(x & 0x7f);
                if x & 0x80 == 0 {
                    break;
                }
            }
            tick += d;
            let len = if b[i] == 0xff {
                3 + usize::from(b[i + 2])
            } else if b[i] & 0xf0 == 0xc0 {
                2
            } else {
                3
            };
            if b[i..i + 3] == bytes {
                out.push(tick);
            }
            if b[i] == 0xff && b[i + 1] == 0x2f {
                break;
            }
            i += len;
        }
        out
    }

    #[test]
    fn harmonics_sound_their_partial_and_staccato_half_the_note() {
        use guitario_score::Harmonic;
        let q = u64::from(TICKS_PER_QUARTER);
        let note = |string: u8, fret: u8, f: &dyn Fn(&mut Note)| {
            let mut n = Note::new(string, fret);
            f(&mut n);
            Beat { rest: false, notes: vec![n], ..Beat::rest(Duration::QUARTER) }
        };
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![
            note(0, 12, &|n| n.effects.harmonic = Some(Harmonic::Natural)),
            note(1, 3, &|n| n.effects.harmonic = Some(Harmonic::Semi)),
            note(2, 0, &|n| n.effects.staccato = true),
            note(3, 0, &|_| {}),
        ];
        let b = export(&s).unwrap();
        let on = |key: u8| b.windows(2).any(|w| w == [0x90, key]);
        assert!(on(76) && !on(64), "the open E's octave, not the fretted note");
        assert!(on(86) && on(62), "a semi-harmonic sounds two octaves up and the note under it");
        assert_eq!(at(&b, [0x80, 55, 0]), vec![2 * q + q / 2], "the staccato G stops halfway through its beat");
        assert_eq!(at(&b, [0x80, 50, 0]), vec![4 * q]);
    }

    #[test]
    fn a_let_ring_note_is_held_until_its_string_is_played_again() {
        let q = u64::from(TICKS_PER_QUARTER);
        let note = |string: u8, fret: u8, ring: bool| {
            let mut n = Note::new(string, fret);
            n.effects.let_ring = ring;
            Beat { rest: false, notes: vec![n], ..Beat::rest(Duration::QUARTER) }
        };
        let mut s = Score::new();
        // Low E rings through the A string's note until the low E string plays G.
        s.tracks[0].measures[0].voices[0].beats = vec![note(5, 0, true), note(4, 2, false), Beat::rest(Duration::QUARTER), note(5, 3, true)];
        let b = export(&s).unwrap();
        assert_eq!(at(&b, [0x80, 40, 0]), vec![3 * q], "the open E ends when its string plays again");
        assert_eq!(at(&b, [0x80, 47, 0]), vec![2 * q], "the plain B ends with its beat");
        // The last let-ring note rings a bar past the end of the song.
        assert_eq!(at(&b, [0x80, 43, 0]), vec![8 * q]);
    }

    #[test]
    fn vlq_encoding() {
        let mut v = Vec::new();
        vlq(&mut v, 0x80);
        assert_eq!(v, [0x81, 0x00]);
        let mut v = Vec::new();
        vlq(&mut v, 0);
        assert_eq!(v, [0]);
    }
}
