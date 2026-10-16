//! Generates the demo suite in `demos/`: `cargo run -p guitario-engine --example demos`.
//!
//! Scores are written in a small tab mini-language and entered through the engine's own commands,
//! so every demo also exercises the editing path. Every piece is an original composition, and the
//! suite is built around metal, with drum parts on every band piece.
//!
//! A bar is a comma-separated list of beats. A beat is a duration (`8`, `4.`, `2..`, `8t` for a
//! triplet), then notes as `string:fret` (string 1 is the high E, `x` is a dead note) or `r` for a
//! rest. Modifiers follow the note they belong to: `+pm` palm mute, `+lr` let ring, `+h`
//! hammer/pull, `+v` vibrato, `+g` ghost, `+a` accent, `+st` staccato, `+b`/`+b2`/`+b6`/`+b8` bend
//! (quarter tones), `+sl` shift slide, `+sll` legato slide, `+sli`/`+sla` slide in from
//! below/above, `+slo`/`+slu` slide out, `+nh`/`+ah`/`+ph` harmonics, `~` tie. `*pm` applies a
//! modifier to every note in the beat so far; `@mf` sets the beat's dynamic. On a drum track a beat
//! lists kit pieces by their short names: `bd` kick, `sd` snare, `hh`/`oh`/`ph` hi-hat closed, open
//! and pedal, `cr` crash, `ch` china, `sp` splash, `rd`/`rb` ride and bell, `t1`-`t4` and `ft` toms.

// The generator indexes fixed literal lists, so out-of-range is a compile-time-visible mistake.
#![allow(clippy::indexing_slicing)]

use guitario_engine::Session;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

type R<T = ()> = Result<T, String>;

struct Bar {
    beats: String,
    /// A second voice entered over the first, in the same notation.
    voice2: Option<String>,
    ts: Option<(u8, u8)>,
    key: Option<i8>,
    tempo: Option<u16>,
    marker: Option<String>,
    open: bool,
    close: u8,
}

fn bar(beats: impl AsRef<str>) -> Bar {
    Bar { beats: beats.as_ref().to_string(), voice2: None, ts: None, key: None, tempo: None, marker: None, open: false, close: 0 }
}

impl Bar {
    fn with_voice2(mut self, beats: impl AsRef<str>) -> Self {
        self.voice2 = Some(beats.as_ref().to_string());
        self
    }
    fn ts(mut self, n: u8, d: u8) -> Self {
        self.ts = Some((n, d));
        self
    }
    fn key(mut self, k: i8) -> Self {
        self.key = Some(k);
        self
    }
    fn tempo(mut self, t: u16) -> Self {
        self.tempo = Some(t);
        self
    }
    fn mark(mut self, m: &str) -> Self {
        self.marker = Some(m.to_string());
        self
    }
    fn open(mut self) -> Self {
        self.open = true;
        self
    }
    fn close(mut self, extra: u8) -> Self {
        self.close = extra;
        self
    }
}

fn run(s: &mut Session, id: &str, p: Value) -> R {
    s.run(id, &p).map(|_| ()).map_err(|e| format!("{id} {p}: {e}"))
}

fn effect(s: &mut Session, fx: &str) -> R {
    let toggle = |s: &mut Session, name: &str| run(s, "note.toggleEffect", json!({"effect": name}));
    match fx {
        "pm" => toggle(s, "palmMute"),
        "lr" => toggle(s, "letRing"),
        "h" => toggle(s, "hammer"),
        "v" => toggle(s, "vibrato"),
        "g" => toggle(s, "ghost"),
        "a" => toggle(s, "accent"),
        "st" => toggle(s, "staccato"),
        "b" => run(s, "note.setBend", json!({"quarterTones": 4})),
        "sl" => run(s, "note.setSlide", json!({"slide": "shift"})),
        "sll" => run(s, "note.setSlide", json!({"slide": "legato"})),
        "sli" => run(s, "note.setSlide", json!({"slide": "intoFromBelow"})),
        "sla" => run(s, "note.setSlide", json!({"slide": "intoFromAbove"})),
        "slo" => run(s, "note.setSlide", json!({"slide": "outDown"})),
        "slu" => run(s, "note.setSlide", json!({"slide": "outUp"})),
        "nh" => run(s, "note.setHarmonic", json!({"harmonic": "natural"})),
        "ah" => run(s, "note.setHarmonic", json!({"harmonic": "artificial"})),
        "ph" => run(s, "note.setHarmonic", json!({"harmonic": "pinch"})),
        b if b.starts_with('b') => {
            let q: u8 = b[1..].parse().map_err(|_| format!("bad bend `{b}`"))?;
            run(s, "note.setBend", json!({"quarterTones": q}))
        }
        other => Err(format!("unknown modifier `{other}`")),
    }
}

fn enter_beat(s: &mut Session, spec: &str, advance: bool) -> R {
    let mut toks = spec.split_whitespace();
    let first = toks.next().ok_or("empty beat")?;
    let triplet = first.ends_with('t');
    let body = first.trim_end_matches('t');
    let dots = body.matches('.').count() as u8;
    let div: u32 = body.trim_end_matches('.').parse().map_err(|_| format!("bad duration `{first}`"))?;
    // Set a triplet in one step: a plain value first would overflow a nearly full bar.
    let tuplet = if triplet { json!([3, 2]) } else { json!([1, 1]) };
    run(s, "beat.setDuration", json!({"value": div, "dots": dots, "tuplet": tuplet}))?;
    let mut placed: Vec<usize> = Vec::new();
    for tok in toks {
        if tok == "r" {
            run(s, "beat.toggleRest", Value::Null)?;
        } else if tok == "~" {
            run(s, "note.toggleTie", Value::Null)?;
        } else if let Some(name) = tok.strip_prefix('%') {
            run(s, "beat.setChord", json!({"name": name}))?;
        } else if let Some(d) = tok.strip_prefix('@') {
            run(s, "beat.setDynamic", json!({"dynamic": d}))?;
        } else if let Some(fx) = tok.strip_prefix('+') {
            effect(s, fx)?;
        } else if let Some(piece) = guitario_theory::drums::by_name(tok) {
            run(s, "note.setDrum", json!({"drum": piece.midi}))?;
        } else if let Some(fx) = tok.strip_prefix('*') {
            let last = placed.last().copied();
            for st in placed.clone() {
                run(s, "cursor.set", json!({"string": st}))?;
                effect(s, fx)?;
            }
            if let Some(l) = last {
                run(s, "cursor.set", json!({"string": l}))?;
            }
        } else {
            let (st, fret) = tok.split_once(':').ok_or_else(|| format!("bad note `{tok}`"))?;
            let string: usize = st.parse::<usize>().map_err(|_| format!("bad string in `{tok}`"))?.checked_sub(1).ok_or("strings start at 1")?;
            run(s, "cursor.set", json!({"string": string}))?;
            if fret == "x" {
                run(s, "note.setFret", json!({"fret": 0}))?;
                run(s, "note.toggleDead", Value::Null)?;
            } else {
                run(s, "note.setFret", json!({"fret": fret.parse::<u32>().map_err(|_| format!("bad fret in `{tok}`"))?}))?;
            }
            placed.push(string);
        }
    }
    if advance {
        run(s, "cursor.right", Value::Null)?;
    }
    Ok(())
}

fn play_track(s: &mut Session, track: usize, bars: &[Bar]) -> R {
    run(s, "track.select", json!({"index": track}))?;
    for (i, b) in bars.iter().enumerate() {
        if track == 0 && s.score().headers.len() <= i {
            run(s, "measure.append", Value::Null)?;
        }
        run(s, "cursor.set", json!({"measure": i, "beat": 0}))?;
        if track == 0 {
            if let Some((n, d)) = b.ts {
                run(s, "measure.setTimeSignature", json!({"numerator": n, "denominator": d}))?;
            }
            if let Some(k) = b.key {
                run(s, "measure.setKey", json!({"key": k}))?;
            }
            if let Some(t) = b.tempo {
                run(s, "measure.setTempo", json!({"bpm": t}))?;
            }
            if let Some(m) = &b.marker {
                run(s, "measure.setMarker", json!({"text": m}))?;
            }
            if b.open || b.close > 0 {
                run(s, "measure.setRepeat", json!({"open": b.open, "close": b.close}))?;
            }
        }
        let beats: Vec<&str> = b.beats.split(',').map(str::trim).collect();
        for (j, beat) in beats.iter().enumerate() {
            enter_beat(s, beat, j + 1 < beats.len()).map_err(|e| format!("track {track} bar {}: {e}", i + 1))?;
        }
        if let Some(v2) = &b.voice2 {
            run(s, "edit.setVoice", json!({"voice": 2}))?;
            run(s, "cursor.set", json!({"measure": i, "beat": 0}))?;
            let beats: Vec<&str> = v2.split(',').map(str::trim).collect();
            for (j, beat) in beats.iter().enumerate() {
                enter_beat(s, beat, j + 1 < beats.len()).map_err(|e| format!("track {track} bar {} voice 2: {e}", i + 1))?;
            }
            run(s, "edit.setVoice", json!({"voice": 1}))?;
        }
        let score = s.score();
        let measure = score.tracks.get(track).and_then(|t| t.measures.get(i));
        let cap = score.headers.get(i).map_or(0, |h| h.capacity());
        for (vi, v) in measure.map(|m| m.voices.as_slice()).unwrap_or_default().iter().enumerate() {
            if v.ticks() != cap {
                return Err(format!("track {track} bar {} voice {}: {} of {cap} ticks filled", i + 1, vi + 1, v.ticks()));
            }
        }
    }
    Ok(())
}

fn new_demo(title: &str, artist: &str, notes: &str, tempo: u16, name: &str, program: u8, tuning: &str) -> R<Session> {
    let mut s = Session::new();
    run(&mut s, "score.setMeta", json!({"title": title, "artist": artist, "author": "Guitario demo", "notes": notes}))?;
    run(&mut s, "score.setTempo", json!({"bpm": tempo}))?;
    run(&mut s, "track.rename", json!({"name": name}))?;
    run(&mut s, "track.setProgram", json!({"program": program}))?;
    run(&mut s, "track.setTuning", json!({"preset": tuning}))?;
    Ok(s)
}

fn add_track(s: &mut Session, name: &str, tuning: &str, program: u8) -> R {
    run(s, "track.add", json!({"name": name, "tuning": tuning}))?;
    run(s, "track.setProgram", json!({"program": program}))
}

fn write(s: &mut Session, dir: &Path, name: &str) -> R {
    let path = |sub: &str, ext: &str| dir.join(sub).join(format!("{name}.{ext}")).display().to_string();
    run(s, "file.save", json!({"path": path("", "gt")}))?;
    run(s, "file.exportMidi", json!({"path": path("midi", "mid")}))?;
    run(s, "file.exportAscii", json!({"path": path("tab", "txt")}))?;
    run(s, "file.exportGp5", json!({"path": path("gp5", "gp5")}))?;
    run(s, "score.validate", Value::Null)
}

// ---- helpers for writing riffs -----------------------------------------------------------

fn rep(n: usize, beat: &str) -> String {
    vec![beat; n].join(", ")
}

fn j<S: AsRef<str>>(parts: &[S]) -> String {
    parts.iter().map(AsRef::as_ref).collect::<Vec<_>>().join(", ")
}

/// A palm-muted gallop (eighth, sixteenth, sixteenth) on one note: one quarter note long.
fn gallop(string: u8, fret: u8) -> String {
    format!("8 {string}:{fret} +pm, 16 {string}:{fret} +pm, 16 {string}:{fret} +pm")
}

/// The same figure without palm muting, for bass.
fn gallop_open(string: u8, fret: u8) -> String {
    format!("8 {string}:{fret}, 16 {string}:{fret}, 16 {string}:{fret}")
}

/// A power chord in standard-style tuning: root on `string`, fifth and octave above it.
fn p5(string: u8, fret: u8) -> String {
    format!("{string}:{fret} {}:{} {}:{}", string - 1, fret + 2, string - 2, fret + 2)
}

/// A power chord in a drop tuning, where one finger barres the three lowest strings.
fn pd(fret: u8) -> String {
    format!("6:{fret} 5:{fret} 4:{fret}")
}

/// Adds a drum kit and enters its part, bar for bar alongside the existing tracks.
fn drums(s: &mut Session, bars: &[Bar]) -> R {
    run(s, "track.addDrums", json!({}))?;
    let t = s.score().tracks.len().saturating_sub(1);
    play_track(s, t, bars)
}

/// `n` beats of `div` playing `hits`.
fn strokes(n: usize, div: u32, pieces: &str) -> String {
    rep(n, &format!("{div} {pieces}"))
}

/// A 16th-note blast beat: kick and snare alternate under a ride (or crash on the downbeat).
fn blast(cymbal: &str, crash_first: bool) -> String {
    let mut beats = Vec::new();
    for i in 0..16 {
        let piece = if i % 2 == 0 { "bd" } else { "sd" };
        let cym = if i == 0 && crash_first { "cr" } else { cymbal };
        beats.push(format!("16 {piece} {cym}"));
    }
    j(&beats)
}

/// The thrash "skank" beat: kick and snare alternate on eighths with the hi-hat.
fn skank(crash: bool) -> String {
    let first = if crash { "8 bd cr" } else { "8 bd hh" };
    format!("{first}, 8 sd hh, {}", rep(3, "8 bd hh, 8 sd hh"))
}

/// Constant sixteenth double kick with the snare on 2 and 4.
fn double_kick(cymbal: &str, crash: bool) -> String {
    let q = |snare: bool, first: &str| format!("16 bd{} {first}, 16 bd, 16 bd {cymbal}, 16 bd", if snare { " sd" } else { "" });
    j(&[q(false, if crash { "cr" } else { cymbal }), q(true, cymbal), q(false, cymbal), q(true, cymbal)])
}

/// A gallop on the kick (eighth, sixteenth, sixteenth) with the snare on 2 and 4.
fn kick_gallop(crash: bool) -> String {
    let q = |top: &str| format!("8 {top}, 16 bd, 16 bd");
    j(&[q(if crash { "bd cr" } else { "bd hh" }), q("sd hh"), q("bd hh"), q("sd hh")])
}

/// A straight rock/metal eighth-note beat on the hi-hat or ride.
fn eighths(cymbal: &str, crash: bool) -> String {
    let first = if crash { "8 bd cr".to_string() } else { format!("8 bd {cymbal}") };
    format!("{first}, 8 {cymbal}, 8 sd {cymbal}, 8 {cymbal}, 8 bd {cymbal}, 8 bd {cymbal}, 8 sd {cymbal}, 8 {cymbal}")
}

// ---- the demos --------------------------------------------------------------------------

fn blank() -> R<Session> {
    let mut s = new_demo("Blank score", "", "Eight empty bars in 4/4. Click a beat and type a fret number to start.", 120, "Guitar", 30, "standard")?;
    for _ in 0..7 {
        run(&mut s, "measure.append", Value::Null)?;
    }
    Ok(s)
}

fn thrash() -> R<Session> {
    let mut s = new_demo(
        "Thrash Attack",
        "Guitario",
        "Downpicked sixteenth chugs in E phrygian. Palm mutes on every chug; try 50% speed to build up.",
        190,
        "Rhythm Guitar",
        30,
        "standard",
    )?;
    let a1 = format!("{}, 16 6:3 +pm, 16 6:3 +pm, 16 6:1 +pm, 16 6:1 +pm, 4 {} *a", rep(8, "16 6:0 +pm"), p5(6, 3));
    let a2 = format!(
        "{}, 16 6:1 +pm, 16 6:1 +pm, 16 6:0 +pm, 16 6:0 +pm, 16 6:3 +pm, 16 6:3 +pm, 16 6:0 +pm, 16 6:0 +pm, 8 6:6 5:8 4:8 *a, 8 6:5 5:7 4:7 *a",
        rep(4, "16 6:0 +pm")
    );
    let hits = format!("8 {} *a, 8 r, 8 {} *a, 8 r, 8 {} *a, 8 r, 8 {} *a, 8 r", p5(6, 0), p5(6, 0), p5(6, 3), p5(6, 5));
    // A descending then ascending run in the E minor pentatonic, with hammer-ons.
    let run16 = "16 1:15 +h, 16 1:12, 16 2:15 +h, 16 2:12, 16 3:14 +h, 16 3:12, 16 4:14 +h, 16 4:12, 16 4:14, 16 3:12, 16 3:14, 16 2:12, 16 2:15, 16 1:12, 16 1:15, 16 1:12";
    play_track(
        &mut s,
        0,
        &[
            bar(&a1).mark("Riff").open(),
            bar(&a2),
            bar(&a1),
            bar(&a2).close(1),
            bar(&hits).mark("Hits"),
            bar(run16).mark("Run"),
            bar(format!("2 {} *lr *v, 2 r", p5(6, 0))),
        ],
    )?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let eights = rep(8, "8 4:0");
    let bhits = "8 4:0, 8 r, 8 4:0, 8 r, 8 4:3, 8 r, 8 4:5, 8 r";
    play_track(&mut s, 1, &[bar(&eights), bar(&eights), bar(&eights), bar(&eights), bar(bhits), bar("1 4:0"), bar("2 4:0, 2 r")])?;
    let fill = format!("{}, {}, {}, {}", strokes(4, 16, "sd"), strokes(4, 16, "t1"), strokes(4, 16, "t2"), strokes(4, 16, "ft"));
    drums(
        &mut s,
        &[bar(skank(true)), bar(skank(false)), bar(skank(false)), bar(skank(false)), bar(strokes(4, 8, "bd cr, 8 r")), bar(fill), bar("2 bd cr, 2 r")],
    )?;
    Ok(s)
}

fn doom() -> R<Session> {
    let mut s = new_demo(
        "Funeral Procession",
        "Guitario",
        "Doom in Drop C at 56 bpm. Let every chord ring and lean on the vibrato.",
        56,
        "Rhythm Guitar",
        30,
        "drop c",
    )?;
    let d1 = format!("1 {} *lr *v @ff", pd(0));
    let d2 = format!("2. {} *lr, 4 {} *lr", pd(3), pd(2));
    let d3 = format!("4. {} *lr, 8 {}, 2 {} *lr", pd(5), pd(3), pd(2));
    let d4 = format!("1 {} *lr", pd(0));
    play_track(&mut s, 0, &[bar(&d1).mark("Procession"), bar(&d2), bar(&d3), bar(&d4), bar(&d1).mark("Lament"), bar(&d2), bar(&d3), bar(&d4)])?;
    add_track(&mut s, "Lead Guitar", "drop c", 29)?;
    play_track(
        &mut s,
        1,
        &[
            bar("1 r"),
            bar("1 r"),
            bar("1 r"),
            bar("1 r"),
            bar("2 1:3 +v, 4 1:1, 4 2:3"),
            bar("2. 1:5 +b +v, 4 1:3"),
            bar("4 1:6, 4 1:5, 2 1:3 +v"),
            bar("1 1:0 +v"),
        ],
    )?;
    add_track(&mut s, "Bass", "bass", 33)?;
    play_track(
        &mut s,
        2,
        &[
            bar("1 4:8 +lr"),
            bar("2. 4:11, 4 4:10"),
            bar("4. 4:13, 8 4:11, 2 4:10"),
            bar("1 4:8 +lr"),
            bar("1 4:8 +lr"),
            bar("2. 4:11, 4 4:10"),
            bar("4. 4:13, 8 4:11, 2 4:10"),
            bar("1 4:8 +lr"),
        ],
    )?;
    let open = "4 bd cr @ff, 4 rd, 4 sd rd, 4 rd";
    let groove = "4 bd rd, 4 rd, 4 sd rd, 4 bd rd";
    let fill = "4 bd rd, 4 rd, 8 t1, 8 t2, 8 ft, 8 ft";
    drums(&mut s, &[bar(open), bar(groove), bar(groove), bar(fill), bar(open), bar(groove), bar(groove), bar("1 bd cr")])?;
    Ok(s)
}

fn death() -> R<Session> {
    let mut s = new_demo(
        "Burial Ground",
        "Guitario",
        "Death metal in Drop B: thirty-second tremolo chugs and a slow, crushing breakdown.",
        200,
        "Rhythm Guitar",
        30,
        "drop b",
    )?;
    let trem = rep(8, "32 6:0 +pm");
    let x1 = format!("{trem}, 16 6:0 +pm, 16 6:1 +pm, 16 6:0 +pm, 16 6:1 +pm, {trem}, 8 {} *a, 8 {} *a", pd(3), pd(2));
    let x2 = format!("16 6:5 +pm, 16 6:4 +pm, 16 6:3 +pm, 16 6:2 +pm, 16 6:1 +pm, 16 6:0 +pm, 16 6:0 +pm, 16 6:0 +pm, {trem}, 4 {} *a", pd(0));
    let x3 = format!("4 {} *lr @ff, 4 r, 4 {} *lr, 8 {} *a, 8 {} *a", pd(0), pd(0), pd(3), pd(2));
    let x4 = format!("{}, 2 {} *lr *v", rep(8, "16 6:0 +pm"), pd(0));
    play_track(
        &mut s,
        0,
        &[bar(&x1).mark("Tremolo riff").open(), bar(&x2), bar(&x1), bar(&x2).close(1), bar(&x3).mark("Breakdown").tempo(100), bar(&x4).tempo(200)],
    )?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let pedal = rep(8, "8 4:7");
    play_track(&mut s, 1, &[bar(&pedal), bar(&pedal), bar(&pedal), bar(&pedal), bar("4 4:7, 4 r, 4 4:7, 4 4:10"), bar(format!("{}, 2 4:7", rep(4, "8 4:7")))])?;
    drums(
        &mut s,
        &[
            bar(blast("rd", true)),
            bar(blast("rd", false)),
            bar(blast("rd", true)),
            bar(blast("rd", false)),
            bar("4 bd ch, 4 r, 4 bd ch, 8 sd, 8 sd"),
            bar(format!("{}, 2 bd cr", strokes(8, 16, "bd"))),
        ],
    )?;
    Ok(s)
}

fn power_metal() -> R<Session> {
    let mut s = new_demo(
        "Steel Horizon",
        "Guitario",
        "Power metal in E minor: twin harmony leads, galloping rhythm and sweeping arpeggios.",
        168,
        "Lead Guitar",
        29,
        "standard",
    )?;
    let lead1 = [
        "8 1:7, 8 1:10, 4 1:12, 8 1:10, 8 1:7, 4 1:5",
        "4 1:8, 8 1:7, 8 1:5, 4 1:3, 4 1:5",
        "8 1:10, 8 1:12, 8 1:14, 8 1:12, 2 1:10 +v",
        "2 1:7 +v, 4 1:5, 4 1:3",
    ];
    let lead2 = [
        "8 2:8, 8 2:12, 4 2:13, 8 2:12, 8 2:8, 4 1:2",
        "4 2:10, 8 2:8, 8 2:6, 4 2:5, 4 2:6",
        "8 2:12, 8 2:13, 8 2:15, 8 2:13, 2 2:12 +v",
        "2 2:8 +v, 4 2:7, 4 2:5",
    ];
    let arp = |notes: [&str; 8]| {
        let group: Vec<String> = notes.iter().map(|n| format!("16 {n}")).collect();
        let g = j(&group);
        format!("{g}, {g}")
    };
    let arps = [
        arp(["4:2", "3:0", "2:0", "1:0", "2:0", "3:0", "4:2", "3:0"]),
        arp(["5:3", "4:2", "3:0", "2:1", "1:0", "2:1", "3:0", "4:2"]),
        arp(["4:0", "3:2", "2:3", "1:2", "2:3", "3:2", "4:0", "3:2"]),
        arp(["5:2", "4:4", "3:4", "2:4", "3:4", "4:4", "5:2", "4:4"]),
    ];
    let mut l1: Vec<Bar> = lead1.iter().map(bar).collect();
    l1.extend(arps.iter().map(bar));
    l1[0] = bar(lead1[0]).mark("Verse");
    l1[4] = bar(&arps[0]).mark("Chorus");
    play_track(&mut s, 0, &l1)?;
    add_track(&mut s, "Harmony Guitar", "standard", 29)?;
    let mut l2: Vec<Bar> = lead2.iter().map(bar).collect();
    l2.extend(["1 1:7 +v", "1 1:8 +v", "1 1:10 +v", "1 1:7 +v"].map(bar));
    play_track(&mut s, 1, &l2)?;
    add_track(&mut s, "Rhythm Guitar", "standard", 30)?;
    let roots = [(6u8, 0u8), (5, 3), (5, 5), (5, 2)];
    let chords = [p5(6, 0), p5(5, 3), p5(5, 5), p5(5, 2)];
    let mut rh: Vec<Bar> = roots.iter().map(|(st, f)| bar(rep(4, &gallop(*st, *f)))).collect();
    rh.extend(chords.iter().map(|c| bar(rep(4, &format!("4 {c} *a")))));
    play_track(&mut s, 2, &rh)?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let broots = [(4u8, 0u8), (3, 3), (3, 5), (3, 2)];
    let mut bb: Vec<Bar> = broots.iter().map(|(st, f)| bar(rep(4, &gallop_open(*st, *f)))).collect();
    bb.extend(broots.iter().map(|(st, f)| bar(rep(4, &format!("4 {st}:{f}")))));
    play_track(&mut s, 3, &bb)?;
    let dk = |crash| bar(double_kick("hh", crash));
    let dr = |crash| bar(double_kick("rd", crash));
    drums(&mut s, &[dk(true), dk(false), dk(false), dk(false), dr(true), dr(false), dr(false), dr(false)])?;
    Ok(s)
}

fn djent() -> R<Session> {
    let mut s = new_demo(
        "Seven Strings Deep",
        "Guitario",
        "Djent on a 7-string in drop A: a 3+3+3+3+4 polyrhythm over four beats, then a half-time breakdown.",
        128,
        "7-String Guitar",
        30,
        "7 string drop a",
    )?;
    // Sixteen sixteenths split into accented groups.
    let grp = |groups: &[(usize, u8)]| {
        let mut beats = Vec::new();
        for (len, fret) in groups {
            beats.push(format!("16 7:{fret} +pm +a"));
            beats.extend((1..*len).map(|_| format!("16 7:{fret} +pm")));
        }
        j(&beats)
    };
    let p1 = grp(&[(3, 0), (3, 0), (3, 0), (3, 0), (4, 0)]);
    let p1b = grp(&[(3, 0), (3, 0), (3, 3), (3, 0), (4, 5)]);
    let p2 = grp(&[(3, 0), (3, 3), (3, 0), (3, 5), (4, 6)]);
    let p3 = "4 7:0 6:0 5:2 *a *lr, 4 r, 8 7:3 +pm, 8 7:3 +pm, 4 7:0 6:0 5:2 *a";
    let p4 = "8 7:0 +pm, 8 7:0 +pm, 8 7:5 +pm, 8 7:0 +pm, 4. 3:7 +ph +v, 8 r";
    play_track(
        &mut s,
        0,
        &[bar(&p1).mark("3-3-3-3-4").open(), bar(&p1b), bar(&p2), bar(p3).mark("Breakdown").tempo(110), bar(&p1).tempo(128), bar(p4).close(1)],
    )?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let b8 = rep(8, "8 4:5");
    play_track(&mut s, 1, &[bar(&b8), bar(&b8), bar(&b8), bar("4 4:5, 4 r, 8 4:8, 8 4:8, 4 4:5"), bar(&b8), bar("8 4:5, 8 4:5, 8 4:10, 8 4:5, 4. 4:5, 8 r")])?;
    // The kick locks to the guitar's accented groups; snare on beat 3, hi-hat on eighths.
    let locked = |groups: &[usize]| {
        let mut starts = Vec::new();
        let mut at = 0;
        for g in groups {
            starts.push(at);
            at += g;
        }
        let beats: Vec<String> = (0..16)
            .map(|i| {
                let mut h = Vec::new();
                if starts.contains(&i) {
                    h.push("bd");
                }
                if i == 8 {
                    h.push("sd");
                }
                if i % 2 == 0 {
                    h.push(if i == 0 { "cr" } else { "hh" });
                }
                if h.is_empty() { "16 r".to_string() } else { format!("16 {}", h.join(" ")) }
            })
            .collect();
        j(&beats)
    };
    let p1 = locked(&[3, 3, 3, 3, 4]);
    drums(
        &mut s,
        &[bar(&p1), bar(&p1), bar(&p1), bar("4 bd ch, 4 r, 8 bd, 8 bd, 4 bd sd ch"), bar(&p1), bar("8 bd hh, 8 bd hh, 8 bd hh, 8 bd hh, 4. bd cr, 8 r")],
    )?;
    Ok(s)
}

fn odd_meters() -> R<Session> {
    let mut s = new_demo(
        "Odd Meter Assault",
        "Guitario",
        "Progressive metal in Drop D: 7/8, 5/4, 13/16, 6/8 and 12/8 with tempo and key changes.",
        150,
        "Rhythm Guitar",
        30,
        "drop d",
    )?;
    let seven = format!("8 6:0 +pm, 8 6:0 +pm, 8 {} *a, 8 6:0 +pm, 8 {} *a, 8 6:0 +pm, 8 {} *a", pd(3), pd(5), pd(6));
    let seven_b = format!("8 6:0 +pm, 8 6:0 +pm, 8 {} *a, 8 6:3 +pm, 8 {} *a, 8 6:5 +pm, 8 {} *a", pd(3), pd(5), pd(7));
    let five = format!("4 6:0 +pm, 8 6:0 +pm, 8 6:0 +pm, 4 {} *a, 4 {} *a, 4 {} *lr", pd(3), pd(5), pd(6));
    let thirteen = format!("{}, 16 {} *a, {}, 16 {} *a, {}, 16 {} *a", rep(3, "16 6:0 +pm"), pd(3), rep(3, "16 6:0 +pm"), pd(5), rep(4, "16 6:0 +pm"), pd(6));
    let six = format!("4. {} *lr, 8 {}, 8 {}, 8 {}", pd(0), pd(3), pd(5), pd(3));
    let twelve = format!("4. {} *lr, 4. {}, 4. {}, 4. {} *a", pd(0), pd(0), pd(3), pd(5));
    play_track(
        &mut s,
        0,
        &[
            bar(&seven).ts(7, 8).mark("7/8 (3+2+2)").open(),
            bar(&seven_b).close(1),
            bar(&five).ts(5, 4).mark("5/4").tempo(132),
            bar(&thirteen).ts(13, 16).mark("13/16 (4+4+5)").tempo(150),
            bar(&six).ts(6, 8).mark("6/8").tempo(100).key(-3),
            bar(&twelve).ts(12, 8).mark("12/8").tempo(70),
            bar(format!("1 {} *lr *v", pd(0))).ts(4, 4).mark("Outro").tempo(70).key(0),
        ],
    )?;
    let thirteen_d = format!("16 bd cr, {}, 16 sd hh, 16 bd hh, {}, 16 sd hh, 16 bd hh, {}, 16 sd, 16 sd", rep(2, "16 hh"), rep(2, "16 hh"), rep(2, "16 hh"));
    drums(
        &mut s,
        &[
            bar("8 bd cr, 8 hh, 8 sd hh, 8 bd hh, 8 sd hh, 8 bd hh, 8 sd hh"),
            bar("8 bd hh, 8 hh, 8 sd hh, 8 bd hh, 8 sd hh, 8 bd hh, 8 sd ch"),
            bar("4 bd cr, 8 bd, 8 bd, 4 sd, 4 bd, 4 sd ch"),
            bar(&thirteen_d),
            bar("8 bd rd, 8 rd, 8 rd, 8 sd rd, 8 rd, 8 rd"),
            bar("8 bd cr, 8 rd, 8 rd, 8 sd rd, 8 rd, 8 rd, 8 bd rd, 8 rd, 8 rd, 8 sd rd, 8 t1, 8 ft"),
            bar("1 bd cr"),
        ],
    )?;
    Ok(s)
}

fn nwobhm() -> R<Session> {
    let mut s = new_demo(
        "Iron Gallop",
        "Guitario",
        "Twin-guitar harmony in A minor over a galloping bass, in the spirit of classic British heavy metal.",
        168,
        "Lead Guitar",
        29,
        "standard",
    )?;
    let l1 = [
        "4 1:5, 8 1:8, 8 1:10, 4. 1:12, 8 1:10",
        "4 1:8, 8 1:10, 8 1:8, 4 1:5, 4 1:8",
        "8 1:7, 8 1:8, 8 1:10, 8 1:12, 2 1:15 +b +v",
        "2 1:12 +v, 4 1:10, 4 1:8",
    ];
    let l2 = [
        "4 2:6, 8 2:10, 8 2:12, 4. 2:13, 8 2:12",
        "4 2:10, 8 2:12, 8 2:10, 4 2:6, 4 2:10",
        "8 2:8, 8 2:10, 8 2:12, 8 2:13, 2 2:17 +v",
        "2 2:13 +v, 4 2:12, 4 2:10",
    ];
    let mut a: Vec<Bar> = l1.iter().map(bar).collect();
    a[0] = bar(l1[0]).mark("Theme").open();
    a[3] = bar(l1[3]).close(1);
    a.push(bar("1 1:5 +v"));
    play_track(&mut s, 0, &a)?;
    add_track(&mut s, "Harmony Guitar", "standard", 29)?;
    let mut b: Vec<Bar> = l2.iter().map(bar).collect();
    b.push(bar("1 2:6 +v"));
    play_track(&mut s, 1, &b)?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let g = |st: u8, f: u8| bar(rep(4, &gallop_open(st, f)));
    play_track(&mut s, 2, &[g(3, 0), g(4, 1), g(4, 3), g(4, 0), bar("1 3:0")])?;
    drums(&mut s, &[bar(kick_gallop(true)), bar(kick_gallop(false)), bar(kick_gallop(false)), bar(kick_gallop(false)), bar("1 bd cr")])?;
    Ok(s)
}

fn black_metal() -> R<Session> {
    let mut s = new_demo(
        "Frostbitten Dawn",
        "Guitario",
        "Tremolo-picked melody in F# minor over open fifths. Each tremolo is eight thirty-second notes.",
        190,
        "Tremolo Guitar",
        30,
        "standard",
    )?;
    let t = |fret: u8| rep(8, &format!("32 2:{fret}"));
    let c = |chord: &str| rep(8, &format!("32 {chord}"));
    let t1 = j(&[t(7), t(9), t(10), t(9)]);
    let t2 = j(&[t(12), t(10), t(9), t(7)]);
    let t3 = j(&[c("6:2 5:4 4:4"), c("6:0 5:2 4:2"), c("6:4 5:6 4:6"), c("6:2 5:4 4:4")]);
    play_track(&mut s, 0, &[bar(&t1).mark("Tremolo melody").key(3).open(), bar(&t2).close(1), bar(&t3).mark("Open fifths"), bar("1 6:2 5:4 4:4 *lr *v")])?;
    add_track(&mut s, "Bass", "bass", 34)?;
    play_track(&mut s, 1, &[bar(rep(8, "8 4:2")), bar(rep(8, "8 4:2")), bar("4 4:2, 4 4:0, 4 4:4, 4 4:2"), bar("1 4:2")])?;
    drums(&mut s, &[bar(blast("rd", true)), bar(blast("rd", false)), bar(blast("cr", true)), bar("1 bd cr")])?;
    Ok(s)
}

fn breakdown() -> R<Session> {
    let mut s = new_demo(
        "Breakdown Protocol",
        "Guitario",
        "Metalcore half-time breakdown in Drop B: syncopated palm-muted chugs, dead notes and a pinch harmonic.",
        140,
        "Rhythm Guitar",
        30,
        "drop b",
    )?;
    let m1 = format!("8 {} *a, 8 6:0 +pm, 8 r, 8 6:0 +pm, 8 r, 8 6:3 +pm, 4 {} *a", pd(0), pd(0));
    let m2 = format!("4 {} *a *lr, 4 r, 8 6:0 +pm, 8 6:0 +pm, 4 6:5 +slo", pd(0));
    let m3 = "8 6:0 +pm, 8 6:x, 8 6:0 +pm, 8 6:x, 4 3:7 +ph +v, 4 r";
    play_track(
        &mut s,
        0,
        &[bar(&m1).mark("Breakdown").open(), bar(&m2), bar(&m1), bar(&m2).close(1), bar(m3).mark("Pinch"), bar(format!("1 {} *lr *a", pd(0)))],
    )?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let b1 = "8 4:7, 8 4:7, 8 r, 8 4:7, 8 r, 8 4:10, 4 4:7";
    let b2 = "4 4:7, 4 r, 8 4:7, 8 4:7, 4 4:12";
    play_track(&mut s, 1, &[bar(b1), bar(b2), bar(b1), bar(b2), bar("8 4:7, 8 r, 8 4:7, 8 r, 4 4:7, 4 r"), bar("1 4:7")])?;
    let d1 = "8 bd ch, 8 bd, 8 r, 8 bd, 8 sd, 8 bd, 4 bd ch";
    let d2 = "4 bd cr, 4 sd, 8 bd, 8 bd, 4 sd ch";
    drums(&mut s, &[bar(d1), bar(d2), bar(d1), bar(d2), bar("8 bd hh, 8 sd, 8 bd hh, 8 sd, 4 bd cr, 4 r"), bar("1 bd cr")])?;
    Ok(s)
}

fn ballad() -> R<Session> {
    let mut s = new_demo(
        "Ashes to Distortion",
        "Guitario",
        "A fingerpicked nylon intro in Am - F - C - G that the band crashes into at bar 5.",
        84,
        "Nylon Guitar",
        24,
        "standard",
    )?;
    let arp = |bass: &str, mid: &str, top: &str| format!("8 {bass} +lr @mp, 8 {mid}, 8 {top}, 8 1:0, 8 {top}, 8 {mid}, 8 {top}, 8 1:0");
    let _ = &arp;
    let am = "8 5:0 +lr @mp %Am, 8 3:2, 8 2:1, 8 1:0, 8 2:1, 8 3:2, 8 2:1, 8 1:0";
    let f = "8 4:3 +lr %F, 8 3:2, 8 2:1, 8 1:0, 8 2:1, 8 3:2, 8 2:1, 8 1:0";
    let c = "8 5:3 +lr %C, 8 3:0, 8 2:1, 8 1:0, 8 2:1, 8 3:0, 8 2:1, 8 1:0";
    let g = "8 6:3 +lr %G, 8 4:0, 8 3:0, 8 2:0, 8 3:0, 8 4:0, 8 3:0, 8 2:0";
    play_track(&mut s, 0, &[bar(am).mark("Acoustic intro"), bar(f), bar(c), bar(g), bar(am).mark("Heavy").tempo(112), bar(f), bar(c), bar(g)])?;
    add_track(&mut s, "Distortion Guitar", "standard", 30)?;
    let chug = |chord: String| format!("8 {chord} *pm @f, 8 {chord} *pm, 8 {chord} *pm, 8 r, 8 {chord} *pm, 8 {chord} *pm, 4 {chord} *a");
    let mut gbars: Vec<Bar> = (0..4).map(|_| bar("1 r")).collect();
    gbars.extend([chug(p5(5, 0)), chug(p5(6, 1)), chug(p5(5, 3)), chug(p5(6, 3))].map(bar));
    play_track(&mut s, 1, &gbars)?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let mut bbars: Vec<Bar> = (0..4).map(|_| bar("1 r")).collect();
    bbars.extend(["3:0", "4:1", "3:3", "4:3"].map(|r| bar(rep(8, &format!("8 {r}")))));
    play_track(&mut s, 2, &bbars)?;
    let mut d: Vec<Bar> = (0..4).map(|_| bar("1 r")).collect();
    d.extend([bar(eighths("hh", true)), bar(eighths("hh", false)), bar(eighths("hh", false)), bar(eighths("hh", false))]);
    drums(&mut s, &d)?;
    Ok(s)
}

fn solo() -> R<Session> {
    let mut s = new_demo(
        "Fretboard Fury",
        "Guitario",
        "A lead-guitar solo in E minor with bends, legato runs, harmonics and triplet pentatonics.",
        140,
        "Lead Guitar",
        29,
        "standard",
    )?;
    let triplets = ["1:15", "1:12", "2:15", "2:12", "3:14", "3:12", "3:14", "2:12", "2:15", "1:12", "1:15", "1:17"].map(|n| format!("8t {n}"));
    let solo_bars = [
        "4 1:12 +b +v, 8 1:15, 8 1:12, 4 2:15 +b, 4 2:12".to_string(),
        "16 1:15 +h, 16 1:12, 16 2:15 +h, 16 2:12, 16 3:14 +h, 16 3:12, 16 4:14 +h, 16 4:12, 8 3:14 +sl, 8 3:17, 4 3:14 +v".to_string(),
        "2 1:12 +nh, 4 1:15 +b +v, 4 1:12".to_string(),
        j(&triplets),
        "4. 1:17 +b +v, 8 1:15, 4 1:12, 4 2:15".to_string(),
        "8 2:15 +h, 8 2:12, 8 3:14 +h, 8 3:12, 4 4:14, 4 4:12".to_string(),
        "2 1:15 +b +v, 2 1:12 +v".to_string(),
        "1 1:12 +v".to_string(),
    ];
    let mut b: Vec<Bar> = solo_bars.iter().map(bar).collect();
    b[0] = bar(&solo_bars[0]).mark("Solo");
    play_track(&mut s, 0, &b)?;
    add_track(&mut s, "Rhythm Guitar", "standard", 30)?;
    let em = rep(4, &gallop(6, 0));
    let cc = rep(4, &gallop(5, 3));
    let dd = rep(4, &gallop(5, 5));
    play_track(&mut s, 1, &[bar(&em), bar(&em), bar(&cc), bar(&dd), bar(&em), bar(&em), bar(&cc), bar(format!("1 {} *lr", p5(6, 0)))])?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let eight = rep(8, "8 4:0");
    let c8 = rep(8, "8 3:3");
    let d8 = rep(8, "8 3:5");
    play_track(&mut s, 2, &[bar(&eight), bar(&eight), bar(&c8), bar(&d8), bar(&eight), bar(&eight), bar(&c8), bar("1 4:0")])?;
    let d = |crash| bar(eighths("rd", crash));
    drums(&mut s, &[d(true), d(false), d(false), d(false), d(true), d(false), d(false), bar("1 bd cr")])?;
    Ok(s)
}

fn techniques() -> R<Session> {
    let mut s = new_demo(
        "Metal Technique Showcase",
        "Guitario",
        "One bar per technique: use it to check how every effect is drawn and played back.",
        120,
        "Lead Guitar",
        30,
        "standard",
    )?;
    play_track(
        &mut s,
        0,
        &[
            bar("8 1:5 +h, 8 1:7, 8 1:7 +h, 8 1:5, 8 2:5 +h, 8 2:7, 8 2:7 +h, 8 2:5").mark("Hammer-ons and pull-offs"),
            bar("4 1:5 +sl, 4 1:9, 4 1:9 +sl, 4 1:5").mark("Shift slides"),
            bar("4 2:7 +sli, 4 2:7 +slo, 4 2:9 +sla, 4 2:9 +slu").mark("Slides in and out"),
            bar("4 2:8 +b, 4 2:8 +b +v, 4 2:8 +b2, 4 2:10 +v").mark("Bends and vibrato"),
            bar("4 6:12 +nh, 4 5:12 +nh, 4 4:12 +nh, 4 3:12 +nh").mark("Natural harmonics"),
            bar("4 3:7 +ph +v, 4 2:8 +ah, 4 1:7 +ph, 4 r").mark("Pinch and artificial harmonics"),
            bar(format!("{}, 2 5:0 4:2 3:2 2:1 1:0 *lr", rep(4, "8 6:0 +pm"))).mark("Palm mute, let ring"),
            bar(rep(4, &gallop(6, 0))).mark("Gallop"),
            bar(format!("{}, {}, {}, {}", rep(8, "32 6:0 +pm"), rep(8, "32 6:1 +pm"), rep(8, "32 6:3 +pm"), rep(8, "32 6:1 +pm"))).mark("Tremolo picking"),
            bar("8 6:0 +pm, 8 6:x, 8 5:2 +g, 8 6:x, 8 6:0, 8 6:x, 8 5:2 +a, 8 6:x").mark("Dead, ghost, accent"),
            bar("4 1:5 @p, 4 1:5 ~, 2 1:7 @ff").mark("Ties and dynamics"),
            bar("8t 1:0, 8t 1:3, 8t 1:5, 8t 1:0, 8t 1:3, 8t 1:5, 4 1:7, 4 r").mark("Triplets"),
            bar("4 6:0 5:2 4:2 3:1 2:0 1:0 *lr @mf, 4 r, 2 r"),
        ],
    )?;
    Ok(s)
}

fn bass_rumble() -> R<Session> {
    let mut s = new_demo(
        "Rumble",
        "Guitario",
        "A metal bass study in E: pick gallops, a climbing line, and a fill. Practise slowly with the metronome on.",
        180,
        "Picked Bass",
        34,
        "bass",
    )?;
    let gal = |f: u8| gallop_open(4, f);
    let r1 = rep(4, &gal(0));
    let r2 = j(&[gal(0), gal(0), gal(3), gal(5)]);
    let r3 = "8 4:0, 8 4:0, 8 4:0, 8 4:3, 8 4:0, 8 4:0, 8 4:5, 8 4:6";
    let r4 = "16 3:7, 16 3:5, 16 3:3, 16 3:0, 16 4:7, 16 4:5, 16 4:3, 16 4:0, 2 4:0 +v";
    play_track(&mut s, 0, &[bar(&r1).mark("Gallop").open(), bar(&r2), bar(r3), bar(r4).close(1), bar("1 4:0 +v")])?;
    let beat = format!("{}, {}", "8 bd cr, 8 hh, 8 sd hh, 8 hh", "8 bd hh, 8 hh, 8 sd hh, 8 hh");
    let beat2 = format!("{}, {}", "8 bd hh, 8 hh, 8 sd hh, 8 hh", "8 bd hh, 8 bd hh, 8 sd hh, 8 hh");
    drums(&mut s, &[bar(&beat), bar(&beat2), bar(&beat2), bar(&beat2), bar("1 bd cr")])?;
    Ok(s)
}

fn drum_grooves() -> R<Session> {
    let mut s = new_demo(
        "Drum Grooves",
        "Guitario",
        "A metal drummer's reference: one groove per bar, from the skank beat to blast beats and tom fills.",
        160,
        "Bass",
        34,
        "bass",
    )?;
    // A drum-only score: add the kit, then drop the default track.
    run(&mut s, "track.addDrums", json!({"name": "Drums"}))?;
    run(&mut s, "track.select", json!({"index": 0}))?;
    run(&mut s, "track.remove", Value::Null)?;
    let half_time = "4 bd cr, 4 hh, 4 sd hh, 4 hh";
    let d_beat = "8 bd hh, 8 bd hh, 8 sd hh, 8 bd hh, 8 bd hh, 8 bd hh, 8 sd hh, 8 hh";
    let triplet_fill = format!("{}, {}, {}, {}", rep(3, "8t sd"), rep(3, "8t t1"), rep(3, "8t t2"), rep(3, "8t ft"));
    let china_hits = "8 bd ch, 8 r, 8 bd ch, 8 r, 8 bd ch, 8 bd ch, 4 bd sd ch";
    play_track(
        &mut s,
        0,
        &[
            bar(skank(true)).mark("Skank beat"),
            bar(skank(false)),
            bar(d_beat).mark("D-beat"),
            bar(kick_gallop(true)).mark("Kick gallop"),
            bar(double_kick("hh", true)).mark("Double kick"),
            bar(half_time).mark("Half time").tempo(140),
            bar(blast("rd", true)).mark("Blast beat").tempo(200),
            bar(blast("cr", false)),
            bar(china_hits).mark("China hits").tempo(140),
            bar(triplet_fill).mark("Triplet tom fill"),
            bar("2 bd cr, 2 r"),
        ],
    )?;
    Ok(s)
}

fn two_voices() -> R<Session> {
    let mut s = new_demo(
        "Two Voices",
        "Guitario",
        "A fingerstyle study in A minor: the melody is Voice 1 (stems up), the bass is Voice 2 (stems down). The G in bar 4 is tied over the bar line.",
        92,
        "Nylon Guitar",
        24,
        "standard",
    )?;
    let melody = [
        "4 2:1 @mp, 8 1:0, 8 2:1, 4 3:2, 4 2:1",
        "4 2:1, 8 1:1, 8 2:1, 4 3:2, 4 2:1",
        "4 2:1, 8 2:0, 8 2:1, 4 1:0, 4 2:1",
        "4 2:0, 4 3:0, 2 1:3",
        "2 1:3 ~, 4 2:1, 4 3:2",
        "4 1:1, 8 2:3, 8 2:1, 4 1:0, 4 2:3",
        "4 2:0, 8 3:1, 8 2:0, 4 1:0, 4 2:0",
        "1 2:1 +lr",
    ];
    let bass = [
        "2 5:0 +lr, 2 4:2",
        "2 6:1 +lr, 2 5:3",
        "2 5:3 +lr, 2 4:2",
        "2 6:3 +lr, 2 5:2",
        "2 5:0 +lr, 2 4:2",
        "2 4:0 +lr, 2 3:2",
        "2 6:0 +lr, 2 5:2",
        "1 5:0 +lr",
    ];
    let bars: Vec<Bar> =
        melody.iter().zip(bass).enumerate().map(|(i, (m, b))| if i == 0 { bar(m).with_voice2(b).mark("Verse") } else { bar(m).with_voice2(b) }).collect();
    play_track(&mut s, 0, &bars)?;
    Ok(s)
}

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos")
}

fn main() -> ExitCode {
    let dir = demo_dir();
    for sub in ["", "midi", "tab", "gp5"] {
        if let Err(e) = std::fs::create_dir_all(dir.join(sub)) {
            eprintln!("cannot create {}: {e}", dir.join(sub).display());
            return ExitCode::FAILURE;
        }
    }
    // Replace the whole suite: drop outputs of demos that no longer exist.
    for (sub, ext) in [("", "gt"), ("midi", "mid"), ("tab", "txt"), ("gp5", "gp5")] {
        if let Ok(entries) = std::fs::read_dir(dir.join(sub)) {
            for e in entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == ext)) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    type Make = fn() -> R<Session>;
    let all: [(&str, Make); 16] = [
        ("00-blank", blank),
        ("01-thrash-attack", thrash),
        ("02-funeral-procession", doom),
        ("03-burial-ground", death),
        ("04-steel-horizon", power_metal),
        ("05-seven-strings-deep", djent),
        ("06-odd-meter-assault", odd_meters),
        ("07-iron-gallop", nwobhm),
        ("08-frostbitten-dawn", black_metal),
        ("09-breakdown-protocol", breakdown),
        ("10-ashes-to-distortion", ballad),
        ("11-fretboard-fury", solo),
        ("12-metal-technique-showcase", techniques),
        ("13-bass-rumble", bass_rumble),
        ("14-drum-grooves", drum_grooves),
        ("15-two-voices", two_voices),
    ];
    let mut failed = false;
    for (name, make) in all {
        match make().and_then(|mut s| write(&mut s, &dir, name).map(|()| s)) {
            Ok(s) => {
                let score = s.score();
                println!("{name}: {} bars, {} track(s)", score.headers.len(), score.tracks.len());
            }
            Err(e) => {
                eprintln!("{name}: {e}");
                failed = true;
            }
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
