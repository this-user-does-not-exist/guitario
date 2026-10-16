//! L2 Standard MIDI File (format 1) export, with repeats unrolled and ties merged.

use guitario_score::{Score, playback_order, timeline};
use guitario_theory::TICKS_PER_QUARTER;

#[derive(Debug, thiserror::Error)]
pub enum MidiError {
    #[error("score has more than 15 tracks; MIDI channels run out")]
    TooManyTracks,
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
    for mi in playback_order(score) {
        let Some(h) = score.headers.get(mi) else { continue };
        let us = 60_000_000 / u32::from(score.tempo_at(mi).max(1));
        let tempo = |us: u32| vec![0xff, 0x51, 0x03, (us >> 16) as u8, (us >> 8) as u8, us as u8];
        meta.push((clock, 0, tempo(us)));
        // A fermata slows the tempo for the beat it holds, then the bar's tempo comes back.
        for hold in score.holds(mi) {
            let slow = (f64::from(us) * hold.stretch).round().clamp(1.0, f64::from(0x00FF_FFFF)) as u32;
            meta.push((clock + u64::from(hold.at), 0, tempo(slow)));
            meta.push((clock + u64::from(hold.at) + u64::from(hold.ticks), 0, tempo(us)));
        }
        let den_pow = h.time_signature.denominator.max(1).ilog2() as u8;
        meta.push((clock, 0, vec![0xff, 0x58, 0x04, h.time_signature.numerator, den_pow, 24, 8]));
        clock += u64::from(h.capacity());
    }
    chunk(&mut out, b"MTrk", &write_events(meta));

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
        let silent = track.muted || (any_solo && !track.solo);
        if !silent {
            for n in notes.iter().filter(|n| n.track == ti) {
                let Some(p) = n.pitch.or_else(|| n.dead.then(|| track.tuning.pitch(usize::from(n.string), 0, 0)).flatten()) else {
                    continue;
                };
                let (vel, len) = if n.dead { (n.velocity / 2, 120) } else { (n.velocity, n.ticks) };
                ev.push((n.start_tick, 3, vec![0x90 | ch, p.0, vel.max(1)]));
                // Offs sort before ons at the same tick, so repeated notes retrigger cleanly.
                ev.push((n.start_tick + u64::from(len), 2, vec![0x80 | ch, p.0, 0]));
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
    fn vlq_encoding() {
        let mut v = Vec::new();
        vlq(&mut v, 0x80);
        assert_eq!(v, [0x81, 0x00]);
        let mut v = Vec::new();
        vlq(&mut v, 0);
        assert_eq!(v, [0]);
    }
}
