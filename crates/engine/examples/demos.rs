//! Generates the demo suite in `demos/`: `cargo run -p guitario-engine --example demos`.
//!
//! Scores are written in a small tab mini-language and entered through the engine's own commands,
//! so every demo also exercises the editing path. The pieces are public-domain classical works
//! arranged here for metal band, with drum parts on every band piece; the technique showcase and
//! the drum grooves are original reference material.
//!
//! A bar is a comma-separated list of beats. A beat is a duration (`8`, `4.`, `2..`, `8t` for a
//! triplet), then notes as `string:fret` (string 1 is the high E, `x` is a dead note) or `r` for a
//! rest. Modifiers follow the note they belong to: `+pm` palm mute, `+lr` let ring, `+h`
//! hammer/pull, `+tr7` trill to fret 7 (`+tr7:32` and `+tr7:64` for faster ones), `+gr5` a grace
//! note from fret 5 (then `h` hammered, `s` slid, `b` bent, `o` on the beat, `x` dead, as in
//! `+gr5ho`), `+lf3`/`+rfi` left- and right-hand fingers, `+v` vibrato, `+g` ghost, `+a` accent, `+st` staccato, `+b`/`+b2`/`+b6`/`+b8` bend
//! (quarter tones), `+sl` shift slide, `+sll` legato slide, `+sli`/`+sla` slide in from
//! below/above, `+slo`/`+slu` slide out, `+nh`/`+ah`/`+ph` harmonics, `~` tie. `*pm` applies a
//! modifier to every note in the beat so far; `@mf` sets the beat's dynamic; `%Am` names the
//! beat's chord and `%Am=x02210` adds its diagram (frets from the low string, `x` not played); `&tp8`, `&tp16` and
//! `&tp32` make the beat tremolo-picked at that note value; `&fs`, `&fm` and `&fl` put a short,
//! medium or long fermata over the beat; `&wdive:-8` moves the whammy bar (a shape, then the depth
//! in quarter tones, negative for a dive) and `&wvs`/`&wvw` add slight or wide vibrato with the bar. On a drum track a beat
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
    /// Beam groups in the time signature's note value; empty keeps the usual grouping.
    beams: Vec<u8>,
}

fn bar(beats: impl AsRef<str>) -> Bar {
    Bar { beats: beats.as_ref().to_string(), voice2: None, ts: None, key: None, tempo: None, marker: None, open: false, close: 0, beams: Vec::new() }
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
    fn beams(mut self, groups: &[u8]) -> Self {
        self.beams = groups.to_vec();
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
        t if t.starts_with("tr") => {
            // `tr7` or `tr7:32`: the other fret, and the speed as a note value (16, 32 or 64).
            let (fret, speed) = t[2..].split_once(':').unwrap_or((&t[2..], "16"));
            let speed = match speed {
                "16" => "sixteenth",
                "32" => "thirty_second",
                "64" => "sixty_fourth",
                other => return Err(format!("bad trill speed `{other}`")),
            };
            run(s, "note.setTrill", json!({"fret": fret.parse::<u8>().map_err(|_| format!("bad trill fret `{t}`"))?, "speed": speed}))
        }
        f if f.starts_with("lf") || f.starts_with("rf") => {
            // `lf3` the left-hand finger (t or 1-4), `rfi` the right-hand finger (p i m a c).
            let finger = match &f[2..] {
                "t" | "p" => "thumb",
                "1" | "i" => "index",
                "2" | "m" => "middle",
                "3" | "a" => "ring",
                "4" | "c" => "little",
                other => return Err(format!("bad finger `{other}`")),
            };
            let hand = if f.starts_with('l') { "left" } else { "right" };
            run(s, "note.setFingering", json!({ hand: finger }))
        }
        g if g.starts_with("gr") => {
            // `gr5`, then letters: h hammer, s slide, b bend, o on the beat, x dead.
            let digits: String = g[2..].chars().take_while(char::is_ascii_digit).collect();
            let fret: u8 = digits.parse().map_err(|_| format!("bad grace note `{g}`"))?;
            let flags = &g[2 + digits.len()..];
            let transition = match (flags.contains('h'), flags.contains('s'), flags.contains('b')) {
                (true, _, _) => "hammer",
                (_, true, _) => "slide",
                (_, _, true) => "bend",
                _ => "none",
            };
            if let Some(bad) = flags.chars().find(|c| !"hsbox".contains(*c)) {
                return Err(format!("bad grace note flag `{bad}` in `{g}`"));
            }
            run(s, "note.setGrace", json!({"fret": fret, "transition": transition, "onBeat": flags.contains('o'), "dead": flags.contains('x')}))
        }
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
        } else if let Some(chord) = tok.strip_prefix('%') {
            // `%Am`, or `%Am=x02210` with a diagram: frets from the low string, `x` not played.
            match chord.split_once('=') {
                None => run(s, "beat.setChord", json!({"name": chord}))?,
                Some((name, shape)) => {
                    let frets: Vec<Value> = shape.chars().rev().map(|c| c.to_digit(10).map_or(Value::Null, |f| json!(f))).collect();
                    run(s, "beat.setChord", json!({"name": name, "frets": frets}))?;
                }
            }
        } else if let Some(v) = tok.strip_prefix("&wv") {
            let vibrato = match v {
                "s" => "slight",
                "w" => "wide",
                other => return Err(format!("bad bar vibrato `{other}`")),
            };
            run(s, "beat.setBarVibrato", json!({"vibrato": vibrato}))?;
        } else if let Some(w) = tok.strip_prefix("&w") {
            // `&wdive:-8`: the whammy bar's shape and depth in quarter tones (negative dives).
            let (kind, depth) = w.split_once(':').ok_or_else(|| format!("bad whammy `{tok}`"))?;
            let depth: i8 = depth.parse().map_err(|_| format!("bad whammy depth in `{tok}`"))?;
            run(s, "beat.setWhammy", json!({"depth": depth, "kind": kind}))?;
        } else if let Some(kind) = tok.strip_prefix("&f") {
            let kind = match kind {
                "s" => "short",
                "m" => "medium",
                "l" => "long",
                other => return Err(format!("bad fermata `{other}`")),
            };
            run(s, "beat.setFermata", json!({"kind": kind}))?;
        } else if let Some(speed) = tok.strip_prefix("&tp") {
            let speed = match speed {
                "8" => "eighth",
                "16" => "sixteenth",
                "32" => "thirty_second",
                other => return Err(format!("bad tremolo speed `{other}`")),
            };
            run(s, "beat.setTremolo", json!({"speed": speed}))?;
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
            if !b.beams.is_empty() {
                run(s, "measure.setBeaming", json!({"groups": b.beams}))?;
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
    run(s, "file.exportMusicXml", json!({"path": path("musicxml", "musicxml")}))?;
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

// ---- writing lines with note names -------------------------------------------------------

/// Open-string pitches, top string first, for the tunings the arrangements write by note name.
const STANDARD: [u8; 6] = [64, 59, 55, 50, 45, 40];
const DROP_C: [u8; 6] = [62, 57, 53, 48, 43, 36];
const DROP_B: [u8; 6] = [61, 56, 52, 47, 42, 35];
const BASS: [u8; 4] = [43, 38, 33, 28];

/// The MIDI pitch of a note name: `E2`, `F#4`, `Bb3`.
fn midi(name: &str) -> Option<u8> {
    let mut chars = name.chars();
    let step: i32 = match chars.next()? {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let (shift, octave) = match rest.strip_prefix('#') {
        Some(o) => (1, o),
        None => rest.strip_prefix('b').map_or((0, rest), |o| (-1, o)),
    };
    let octave: i32 = octave.parse().ok()?;
    u8::try_from((octave + 1) * 12 + step + shift).ok()
}

/// Where to play `pitch` on `strings` (open pitches, top string first): the string that keeps the
/// fret in or nearest `lo..=lo + 4`, so a phrase stays in one hand position.
fn place(strings: &[u8], lo: u8, pitch: u8) -> Option<String> {
    let off = |f: u8| lo.saturating_sub(f).max(f.saturating_sub(lo + 4));
    strings
        .iter()
        .enumerate()
        .filter_map(|(i, open)| pitch.checked_sub(*open).filter(|f| *f <= 24).map(|f| (i, f)))
        .min_by_key(|(i, f)| (off(*f), *i))
        .map(|(i, f)| format!("{}:{f}", i + 1))
}

/// Applies `f` to every token of every beat in a line.
fn map_tokens(line: &str, f: impl Fn(&str) -> String) -> String {
    let beats: Vec<String> = line.split(',').map(|beat| beat.split_whitespace().map(&f).collect::<Vec<_>>().join(" ")).collect();
    j(&beats)
}

/// A line written with note names (`8 G4, 4. Bb3 +v`), turned into `string:fret` on `strings`.
/// Tokens that are not note names, `5:5` included, pass through unchanged.
fn on(strings: &[u8], lo: u8, line: &str) -> String {
    map_tokens(line, |tok| midi(tok).and_then(|p| place(strings, lo, p)).unwrap_or_else(|| tok.to_string()))
}

/// Adds `fx` after every note of a line, such as `+pm` to palm mute a whole riff.
fn every(line: &str, fx: &str) -> String {
    map_tokens(line, |tok| if tok.contains(':') && !tok.starts_with(['&', '%']) { format!("{tok} {fx}") } else { tok.to_string() })
}

/// Sixteenth notes from a list of note names: `sixteenths("G1 D2 B2")`.
fn sixteenths(notes: &str) -> String {
    let beats: Vec<String> = notes.split_whitespace().map(|n| format!("16 {n}")).collect();
    j(&beats)
}

// ---- the demos --------------------------------------------------------------------------

fn blank() -> R<Session> {
    let mut s = new_demo("Blank score", "", "Eight empty bars in 4/4. Click a beat and type a fret number to start.", 120, "Guitar", 30, "standard")?;
    for _ in 0..7 {
        run(&mut s, "measure.append", Value::Null)?;
    }
    Ok(s)
}

fn bumblebee() -> R<Session> {
    let mut s = new_demo(
        "Flight of the Bumblebee",
        "Nikolai Rimsky-Korsakov",
        "From The Tale of Tsar Saltan (1900), arranged for metal band: the chromatic line in sixteenths over palm-muted chugs. Try it at 50% first.",
        150,
        "Lead Guitar",
        29,
        "standard",
    )?;
    let fall_hi = on(&STANDARD, 7, &sixteenths("E5 D#5 D5 C#5 D5 C#5 C5 B4 C5 B4 A#4 A4 G#4 G4 F#4 F4"));
    let fall_lo = on(&STANDARD, 0, &sixteenths("E4 D#4 D4 C#4 D4 C#4 C4 B3 C4 B3 A#3 A3 G#3 G3 F#3 F3"));
    let theme_e = on(&STANDARD, 8, &sixteenths("E5 D#5 D5 C#5 C5 F5 E5 D#5 E5 D#5 D5 C#5 C5 C#5 D5 D#5"));
    let theme_a = on(&STANDARD, 1, &sixteenths("A4 G#4 G4 F#4 F4 A#4 A4 G#4 A4 G#4 G4 F#4 F4 F#4 G4 G#4"));
    play_track(
        &mut s,
        0,
        &[
            bar(&fall_hi).mark("Intro"),
            bar(&fall_lo),
            bar(&theme_e).mark("Theme").open(),
            bar(&theme_e),
            bar(&theme_a),
            bar(&theme_a).close(1),
            bar(&theme_e),
            bar("4 1:5 +v @ff, 4 r, 2 r"),
        ],
    )?;
    add_track(&mut s, "Rhythm Guitar", "standard", 30)?;
    let chug = |string: u8| bar(format!("16 {string}:0 +pm +a, {}", rep(15, &format!("16 {string}:0 +pm"))));
    let end = format!("4 {} *a, 4 r, 2 r", p5(5, 0));
    play_track(&mut s, 1, &[chug(6), chug(6), chug(6), chug(6), chug(5), chug(5), chug(6), bar(&end)])?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let e8 = rep(8, "8 4:0");
    let a8 = rep(8, "8 3:0");
    play_track(&mut s, 2, &[bar(&e8), bar(&e8), bar(&e8), bar(&e8), bar(&a8), bar(&a8), bar(&e8), bar("4 3:0, 4 r, 2 r")])?;
    let fill = format!("{}, {}, {}, {}", strokes(4, 16, "sd"), strokes(4, 16, "t1"), strokes(4, 16, "t2"), strokes(4, 16, "ft"));
    drums(
        &mut s,
        &[bar(skank(true)), bar(fill), bar(skank(true)), bar(skank(false)), bar(skank(false)), bar(skank(false)), bar(skank(true)), bar("4 bd cr, 4 r, 2 r")],
    )?;
    Ok(s)
}

fn funeral_march() -> R<Session> {
    let mut s = new_demo(
        "Funeral March",
        "Frédéric Chopin",
        "The march from the Piano Sonata No. 2 (1839), taken down to C minor for doom in Drop C. Let every chord ring and lean on the vibrato.",
        56,
        "Rhythm Guitar",
        30,
        "drop c",
    )?;
    // Chopin's left hand rocks between the tonic and the chord a third below it: C minor and A-flat.
    let rock = format!("2 {} *lr *v @ff, 2 {} *lr", pd(0), pd(8));
    let mut rh: Vec<Bar> = (0..7).map(|_| bar(&rock)).collect();
    rh[0] = bar(&rock).mark("Procession");
    rh.push(bar(format!("1 {} *lr *v", pd(0))));
    play_track(&mut s, 0, &rh)?;
    add_track(&mut s, "Lead Guitar", "drop c", 29)?;
    let m1 = on(&DROP_C, 8, "4 C5, 8. C5, 16 C5, 2 C5 +v");
    let m2 = on(&DROP_C, 8, "4. Eb5, 8 D5, 8. D5, 16 C5, 8. C5, 16 B4");
    let ending = on(&DROP_C, 8, "2 Bb4 +b +v, 2 C5 +v");
    play_track(&mut s, 1, &[bar("1 r"), bar("1 r"), bar(&m1).mark("March"), bar(&m2), bar(&m1), bar(&m2), bar(&ending), bar(on(&DROP_C, 8, "1 C5 +v"))])?;
    add_track(&mut s, "Bass", "bass", 33)?;
    let b = "2 4:8 +lr, 2 4:4 +lr";
    play_track(&mut s, 2, &[bar(b), bar(b), bar(b), bar(b), bar(b), bar(b), bar(b), bar("1 4:8 +lr")])?;
    let open = "4 bd cr @ff, 4 rd, 4 sd rd, 4 rd";
    let groove = "4 bd rd, 4 rd, 4 sd rd, 4 bd rd";
    let fill = "4 bd rd, 4 rd, 8 t1, 8 t2, 8 ft, 8 ft";
    drums(&mut s, &[bar(open), bar(fill), bar(open), bar(groove), bar(groove), bar(fill), bar(groove), bar("1 bd cr")])?;
    Ok(s)
}

fn mountain_king() -> R<Session> {
    let mut s = new_demo(
        "In the Hall of the Mountain King",
        "Edvard Grieg",
        "From Peer Gynt (1875), in its own key of B minor on a Drop B guitar: palm-muted at 112, then the reprise at 200 in tremolo-picked thirty-seconds over blast beats.",
        112,
        "Rhythm Guitar",
        30,
        "drop b",
    )?;
    let theme = [
        "8 B1, 8 C#2, 8 D2, 8 E2, 8 F#2, 8 D2, 4 F#2",
        "8 F2, 8 C#2, 4 F2, 8 E2, 8 C2, 4 E2",
        "8 B1, 8 C#2, 8 D2, 8 E2, 8 F#2, 8 D2, 8 F#2, 8 B2",
        "8 A2, 8 F#2, 8 D2, 8 F#2, 2 A2",
    ];
    let fsharp = [
        "8 F#2, 8 G#2, 8 A#2, 8 B2, 8 C#3, 8 A#2, 4 C#3",
        "8 D3, 8 A#2, 4 D3, 8 C#3, 8 A#2, 4 C#3",
        "8 F#2, 8 G#2, 8 A#2, 8 B2, 8 C#3, 8 A#2, 4 C#3",
        "8 D3, 8 A#2, 4 D3, 2 C#3",
    ];
    let gtr = |line: &str, fx: &str| every(&on(&DROP_B, 0, line), fx);
    let mut g: Vec<Bar> = theme.iter().chain(&fsharp).map(|l| bar(gtr(l, "+pm"))).collect();
    g.extend(theme.iter().map(|l| bar(gtr(l, "+pm &tp32"))));
    g.push(bar(format!("4 {} *a @ff, 4 r, 4 {} *a, 4 r", pd(0), pd(0))));
    g[0] = bar(gtr(theme[0], "+pm")).mark("In the hall").key(2);
    g[4] = bar(gtr(fsharp[0], "+pm")).mark("F-sharp major");
    g[8] = bar(gtr(theme[0], "+pm &tp32")).mark("Accelerando").tempo(200);
    play_track(&mut s, 0, &g)?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let mut b: Vec<Bar> = theme.iter().chain(&fsharp).chain(&theme).map(|l| bar(on(&BASS, 0, l))).collect();
    b.push(bar(on(&BASS, 0, "4 B1, 4 r, 4 B1, 4 r")));
    play_track(&mut s, 1, &b)?;
    let march = |crash| bar(eighths("hh", crash));
    let mut d: Vec<Bar> = vec![march(true), march(false), march(false), march(false), march(true), march(false), march(false), march(false)];
    d.extend([bar(blast("rd", true)), bar(blast("rd", false)), bar(blast("rd", false)), bar(blast("cr", false)), bar("4 bd cr, 4 r, 4 bd cr, 4 r")]);
    drums(&mut s, &d)?;
    Ok(s)
}

fn summer() -> R<Session> {
    let mut s = new_demo(
        "Summer: Presto",
        "Antonio Vivaldi",
        "The storm that opens the last movement of Summer from The Four Seasons (1725), an octave down for twin guitars in thirds over a galloping rhythm section.",
        150,
        "Lead Guitar",
        29,
        "standard",
    )?;
    // Repeated notes under a falling top line, then scales: the violin part, an octave lower.
    let pedal = |top: &str, under: &str, lo: u8| on(&STANDARD, lo, &format!("16 {top}, {}", rep(11, &format!("16 {under}"))));
    let pedal_end = |top: &str, under: &str, lo: u8| on(&STANDARD, lo, &format!("16 {top}, {}, 8 {under}, 8 r", rep(7, &format!("16 {under}"))));
    let lead = [
        pedal("G4", "G3", 0),
        pedal("F4", "G3", 0),
        pedal("Eb4", "G3", 0),
        pedal_end("D4", "G3", 0),
        "2. r &fm".to_string(),
        pedal("D5", "D4", 0),
        pedal("C5", "D4", 0),
        pedal("Bb4", "D4", 0),
        pedal_end("A4", "D4", 0),
        on(&STANDARD, 5, "16 r, 16 D5, 16 C5, 16 Bb4, 16 A4, 16 G4, 16 F#4, 16 E4, 16 D4, 16 C4, 16 Bb3, 16 A3"),
        rep(12, "16 3:0"),
        on(&STANDARD, 10, "16 3:0, 16 G5, 16 F5, 16 Eb5, 16 D5, 16 C5, 16 Bb4, 16 A4, 16 G4, 16 F4, 16 Eb4, 16 D4"),
        rep(12, "16 2:1"),
        on(&STANDARD, 13, "16 2:1, 16 Bb5, 16 A5, 16 G5, 16 F5, 16 Eb5, 16 D5, 16 C5, 16 Bb4, 16 A4, 16 G4, 16 F4"),
        rep(12, "16 2:4"),
        "2. 1:3 +v".to_string(),
    ];
    let mut l: Vec<Bar> = lead.iter().map(bar).collect();
    l[0] = bar(&lead[0]).ts(3, 4).key(-2).mark("Presto");
    l[9] = bar(&lead[9]).mark("Storm");
    play_track(&mut s, 0, &l)?;
    add_track(&mut s, "Harmony Guitar", "standard", 29)?;
    let harmony = [
        pedal("Bb4", "Bb3", 3),
        pedal("A4", "Bb3", 3),
        pedal("G4", "Bb3", 3),
        pedal_end("F4", "Bb3", 3),
        "2. r".to_string(),
        pedal("F5", "F4", 5),
        pedal("Eb5", "F4", 5),
        pedal("D5", "F4", 5),
        pedal_end("C5", "F4", 5),
        on(&STANDARD, 8, "16 r, 16 F5, 16 Eb5, 16 D5, 16 C5, 16 Bb4, 16 A4, 16 G4, 16 F4, 16 Eb4, 16 D4, 16 C4"),
        on(&STANDARD, 3, &rep(12, "16 Bb3")),
        on(&STANDARD, 13, "16 3:3, 16 Bb5, 16 A5, 16 G5, 16 F5, 16 Eb5, 16 D5, 16 C5, 16 Bb4, 16 A4, 16 G4, 16 F4"),
        on(&STANDARD, 3, &rep(12, "16 Eb4")),
        on(&STANDARD, 15, "16 2:4, 16 D6, 16 C6, 16 Bb5, 16 A5, 16 G5, 16 F5, 16 Eb5, 16 D5, 16 C5, 16 Bb4, 16 A4"),
        on(&STANDARD, 3, &rep(12, "16 G4")),
        "2. 1:6 +v".to_string(),
    ];
    play_track(&mut s, 1, &harmony.iter().map(bar).collect::<Vec<_>>())?;
    add_track(&mut s, "Rhythm Guitar", "standard", 30)?;
    // G for the first phrase, D for the second, then the scales' G, C and E-flat; (0, 0) rests.
    let roots: [(u8, u8); 15] = [(6, 3), (6, 3), (6, 3), (6, 3), (0, 0), (5, 5), (5, 5), (5, 5), (5, 5), (5, 5), (6, 3), (6, 3), (5, 3), (5, 3), (5, 6)];
    let mut r: Vec<Bar> = roots.iter().map(|&(st, f)| if st == 0 { bar("2. r") } else { bar(rep(3, &gallop(st, f))) }).collect();
    r.push(bar(format!("2. {} *lr", p5(6, 3))));
    play_track(&mut s, 2, &r)?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let on_bass = |root: (u8, u8)| match root {
        (6, 3) => (4, 3),
        (5, 5) => (2, 0),
        (5, 3) => (3, 3),
        _ => (3, 6),
    };
    let mut b: Vec<Bar> = roots
        .iter()
        .map(|&root| {
            if root.0 == 0 {
                bar("2. r")
            } else {
                let (st, f) = on_bass(root);
                bar(rep(3, &gallop_open(st, f)))
            }
        })
        .collect();
    b.push(bar("2. 4:3"));
    play_track(&mut s, 3, &b)?;
    // Double kick in 3/4, the snare on beat 2.
    let dk = |crash: bool| {
        let first = if crash { "16 bd cr" } else { "16 bd hh" };
        bar(format!("{first}, 16 bd, 16 bd hh, 16 bd, 16 bd sd hh, 16 bd, 16 bd hh, 16 bd, 16 bd hh, 16 bd, 16 bd hh, 16 bd"))
    };
    let mut d: Vec<Bar> = (0..16).map(|i| dk(matches!(i, 0 | 5 | 9 | 12))).collect();
    d[4] = bar("2. r");
    d[15] = bar("2. bd cr");
    drums(&mut s, &d)?;
    Ok(s)
}

fn toccata() -> R<Session> {
    let mut s = new_demo(
        "Toccata and Fugue in D minor",
        "Johann Sebastian Bach",
        "The opening of BWV 565 on a 7-string in drop A: the call in two octaves, the same falling line chugged on the low strings, then a 3+3+3+3+4 djent figure in D.",
        60,
        "7-String Guitar",
        30,
        "7 string drop a",
    )?;
    // Sixteen sixteenths split into accented groups on the low string.
    let grp = |groups: &[(usize, u8)]| {
        let mut beats = Vec::new();
        for (len, fret) in groups {
            beats.push(format!("16 7:{fret} +pm +a"));
            beats.extend((1..*len).map(|_| format!("16 7:{fret} +pm")));
        }
        j(&beats)
    };
    let p1 = grp(&[(3, 5), (3, 5), (3, 5), (3, 5), (4, 5)]);
    let p2 = grp(&[(3, 5), (3, 5), (3, 8), (3, 5), (4, 4)]);
    play_track(
        &mut s,
        0,
        &[
            bar("32 1:17, 32 1:15, 4. 1:17 &fl, 16 1:15, 16 1:13, 16 1:12, 16 1:10, 16 1:9, 4 1:10 +v &fl").mark("Adagio"),
            bar("32 1:5, 32 1:3, 4. 1:5 &fl, 16 1:3, 16 1:1, 16 1:0, 16 2:3, 16 2:2, 4 2:3 +v &fl"),
            bar("32 5:0, 32 6:3, 4. 7:0 6:0 5:0 *lr &fl, 16 6:3 +pm, 16 6:1 +pm, 16 6:0 +pm, 16 7:5 +pm, 16 7:4 +pm, 4 7:5 6:5 5:5 *lr &fl"),
            bar(&p1).mark("3-3-3-3-4").tempo(128).open(),
            bar(&p2).close(1),
            bar(&p1),
            bar("8 7:5 +pm, 8 7:5 +pm, 8 7:8 +pm, 8 7:5 +pm, 4. 3:7 +ph +v, 8 r"),
            bar("1 7:5 6:5 5:5 *lr *v"),
        ],
    )?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let b8 = rep(8, "8 2:0");
    play_track(
        &mut s,
        1,
        &[bar("1 r"), bar("1 r"), bar("2 3:0, 2 2:0"), bar(&b8), bar(&b8), bar(&b8), bar("8 2:0, 8 2:0, 8 2:3, 8 2:0, 4. 2:0, 8 r"), bar("1 2:0")],
    )?;
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
    let k = locked(&[3, 3, 3, 3, 4]);
    drums(
        &mut s,
        &[bar("1 r"), bar("1 r"), bar("2 bd cr, 2 bd cr"), bar(&k), bar(&k), bar(&k), bar("8 bd hh, 8 bd hh, 8 bd hh, 8 bd hh, 4. bd cr, 8 r"), bar("1 bd cr")],
    )?;
    Ok(s)
}

fn mars() -> R<Session> {
    let mut s = new_demo(
        "Mars, the Bringer of War",
        "Gustav Holst",
        "The 5/4 ostinato that opens The Planets (1914-16): a triplet, two quarters, two eighths and a quarter, all on G. It starts quietly on one string and grows into power chords.",
        150,
        "Rhythm Guitar",
        30,
        "drop d",
    )?;
    let ost = |ch: &str, dynamic: &str| format!("8t {ch} *pm {dynamic}, 8t {ch} *pm, 8t {ch} *pm, 4 {ch} *a, 4 {ch} *a, 8 {ch} *pm, 8 {ch} *pm, 4 {ch} *a");
    let g = "6:5";
    let chord = pd(5);
    play_track(
        &mut s,
        0,
        &[
            bar(ost(g, "@p")).ts(5, 4).mark("Ostinato"),
            bar(ost(g, "")),
            bar(ost(g, "@mf")),
            bar(ost(g, "")),
            bar(ost(&chord, "@f")).mark("Full force"),
            bar(ost(&chord, "")),
            bar(ost(&chord, "@ff")),
            bar(ost(&chord, "")),
            bar(format!("2 {chord} *lr *a, 2. r")),
        ],
    )?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let b = ost("4:3", "").replace(" *pm", "").replace(" *a", "");
    play_track(&mut s, 1, &[bar(&b), bar(&b), bar(&b), bar(&b), bar(&b), bar(&b), bar(&b), bar(&b), bar("2 4:3, 2. r")])?;
    let quiet = "8t sd, 8t sd, 8t sd, 4 bd, 4 bd, 8 sd, 8 sd, 4 bd";
    let loud = |crash: &str| format!("8t bd sd {crash}, 8t sd, 8t sd, 4 bd ch, 4 bd ch, 8 sd, 8 sd, 4 bd sd ch");
    drums(&mut s, &[bar(quiet), bar(quiet), bar(quiet), bar(quiet), bar(loud("cr")), bar(loud("hh")), bar(loud("cr")), bar(loud("hh")), bar("2 bd cr, 2. r")])?;
    Ok(s)
}

fn little_fugue() -> R<Session> {
    let mut s = new_demo(
        "Little Fugue in G minor",
        "Johann Sebastian Bach",
        "The exposition of BWV 578 for twin guitars: the subject alone, then the answer in D minor under the countersubject, over a galloping bass.",
        100,
        "Lead Guitar",
        29,
        "standard",
    )?;
    let subject = [
        on(&STANDARD, 5, "4 G4, 4 D5, 4. Bb4, 8 A4"),
        on(&STANDARD, 5, "8 G4, 8 Bb4, 8 A4, 8 G4, 8 F#4, 8 A4, 4 D4"),
        on(&STANDARD, 5, "8 G4, 8 D4, 8 A4, 8 D4, 8 Bb4, 16 A4, 16 G4, 8 A4, 8 D4"),
        on(&STANDARD, 5, "8 G4, 16 D4, 16 G4, 8 A4, 16 D4, 16 A4, 8 Bb4, 16 A4, 16 G4, 16 A4, 16 D4, 16 D5, 16 C5"),
        on(&STANDARD, 5, &sixteenths("Bb4 A4 G4 Bb4 A4 G4 F#4 A4 G4 D4 G4 A4 Bb4 C5 D5 E5")),
    ];
    let countersubject = [
        on(&STANDARD, 9, "16 F5, 16 E5, 16 D5, 16 F5, 16 E5, 16 D5, 16 C#5, 16 E5, 8 D5, 8 A4, 8 D5, 8 E5"),
        on(
            &STANDARD,
            13,
            "16 F5, 16 G5, 16 F5, 16 G5, 32 A5, 32 G5, 32 A5, 32 G5, 32 A5, 32 G5, 32 F5, 32 G5, 16 A5, 16 G5, 16 A5, 16 Bb5, 16 A5, 16 G5, 16 F5, 16 E5",
        ),
        on(&STANDARD, 9, &sixteenths("F5 A5 G5 A5 C#5 A5 G5 A5 D5 A5 G5 A5 C#5 A5 G5 A5")),
        on(&STANDARD, 9, &sixteenths("F5 D5 C#5 D5 G5 D5 C#5 D5 A5 D5 C#5 D5 G5 D5 C#5 D5")),
        on(&STANDARD, 5, "8 A4, 8 F5, 8 G4, 8 E5, 8 F4, 8 A4, 8 D5, 8 F5"),
    ];
    let mut l1: Vec<Bar> = subject.iter().chain(&countersubject).map(bar).collect();
    l1[0] = bar(&subject[0]).mark("Subject").key(-2);
    l1[5] = bar(&countersubject[0]).mark("Answer");
    l1.push(bar(on(&STANDARD, 9, "1 D5 +v")));
    play_track(&mut s, 0, &l1)?;
    add_track(&mut s, "Harmony Guitar", "standard", 29)?;
    let answer = [
        "4 D4, 4 A4, 4. F4, 8 E4",
        "8 D4, 8 F4, 8 E4, 8 D4, 8 C#4, 8 E4, 4 A3",
        "8 D4, 8 A3, 8 E4, 8 A3, 8 F4, 16 E4, 16 D4, 8 E4, 8 A3",
        "8 D4, 16 A3, 16 D4, 8 E4, 16 A3, 16 E4, 8 F4, 16 E4, 16 D4, 16 E4, 16 A3, 16 A4, 16 G4",
        "16 F4, 16 E4, 16 D4, 16 F4, 16 E4, 16 D4, 16 C#4, 16 E4, 16 D4, 16 A3, 16 D4, 16 E4, 16 F4, 16 G4, 16 A4, 16 B4",
    ];
    let mut l2: Vec<Bar> = (0..5).map(|_| bar("1 r")).collect();
    l2.extend(answer.iter().map(|a| bar(on(&STANDARD, 3, a))));
    l2.push(bar(on(&STANDARD, 3, "1 A4 +v")));
    play_track(&mut s, 1, &l2)?;
    add_track(&mut s, "Bass", "bass", 34)?;
    // G and D under the subject, D and A under the answer.
    let g = |st: u8, f: u8| bar(rep(4, &gallop_open(st, f)));
    play_track(&mut s, 2, &[g(4, 3), g(2, 0), g(4, 3), g(2, 0), g(4, 3), g(2, 0), g(3, 0), g(2, 0), g(3, 0), g(2, 0), bar("1 2:0")])?;
    let k = |crash| bar(kick_gallop(crash));
    drums(&mut s, &[k(true), k(false), k(false), k(false), k(false), k(true), k(false), k(false), k(false), k(false), bar("1 bd cr")])?;
    Ok(s)
}

fn moonlight() -> R<Session> {
    let mut s = new_demo(
        "Moonlight Sonata: Presto agitato",
        "Ludwig van Beethoven",
        "The rising arpeggios of the last movement of Op. 27 No. 2 (1801) in C# minor (four sharps), then the same harmony tremolo picked in thirty-seconds over blast beats.",
        150,
        "Tremolo Guitar",
        30,
        "standard",
    )?;
    // Each bar climbs a chord's tones in overlapping groups of four, as Beethoven's right hand does.
    let up = |t: [&str; 9]| {
        let i = [0, 1, 2, 3, 1, 2, 3, 4, 2, 3, 4, 5, 3, 4, 5];
        format!("16 r, {}", j(&i.map(|k| format!("16 {}", t[k]))))
    };
    let top = |t: [&str; 9], chord: &str| {
        let i = [6, 4, 5, 6, 7, 5, 6, 7, 8, 6, 7, 8];
        format!("{}, 8 {chord} *a, 8 {chord} *a", j(&i.map(|k| format!("16 {}", t[k]))))
    };
    let csm = ["6:4", "5:4", "4:2", "4:6", "3:6", "2:5", "2:9", "1:9", "1:12"];
    let gsharp = ["6:4", "5:3", "4:1", "4:6", "3:5", "2:4", "2:9", "1:8", "1:11"];
    let t = |frets: [&str; 4]| j(&frets.map(|f| format!("4 {f} &tp32")));
    play_track(
        &mut s,
        0,
        &[
            bar(up(csm)).mark("Arpeggios").key(4),
            bar(top(csm, "3:6 2:5 1:4")),
            bar(up(gsharp)),
            bar(top(gsharp, "3:5 2:4 1:4")),
            bar(t(["1:9", "1:12", "1:9", "2:9"])).mark("Tremolo").open(),
            bar(t(["1:8", "1:11", "1:8", "2:9"])).close(1),
            bar("1 5:4 4:6 3:6 *lr *v"),
        ],
    )?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let cs = rep(8, "8 3:4");
    let gs = rep(8, "8 4:4");
    play_track(&mut s, 1, &[bar(&cs), bar(&cs), bar(&gs), bar(&gs), bar(&cs), bar(&gs), bar("1 3:4")])?;
    let d_beat = "8 bd hh, 8 bd hh, 8 sd hh, 8 bd hh, 8 bd hh, 8 bd hh, 8 sd hh, 8 hh";
    drums(&mut s, &[bar(d_beat), bar(d_beat), bar(d_beat), bar(d_beat), bar(blast("rd", true)), bar(blast("cr", false)), bar("1 bd cr")])?;
    Ok(s)
}

fn fifth_symphony() -> R<Session> {
    let mut s = new_demo(
        "Symphony No. 5",
        "Ludwig van Beethoven",
        "The opening of the Fifth (1808) as a metalcore riff in Drop B: the four-note motto with its fermatas, then the same motto as a half-time breakdown with dead notes and a pinch harmonic.",
        108,
        "Rhythm Guitar",
        30,
        "drop b",
    )?;
    let three = |note: &str| on(&DROP_B, 0, &format!("8 r, 8 {note} +pm, 8 {note} +pm, 8 {note} +pm"));
    play_track(
        &mut s,
        0,
        &[
            bar(three("G2")).ts(2, 4).mark("Allegro con brio"),
            bar(format!("2 {} *lr &fl", pd(4))),
            bar(three("F2")),
            bar(format!("2 {} *lr", pd(3))),
            bar("2 6:3 ~ 5:3 ~ 4:3 ~ &fl"),
            bar(format!("{}, 2 {} *a", three("G2"), pd(4))).ts(4, 4).mark("Breakdown").tempo(140),
            bar(format!("{}, 2 {} *a", three("F2"), pd(3))),
            bar("8 6:0 +pm, 8 6:x, 8 6:0 +pm, 8 6:x, 4 3:7 +ph +v, 4 r"),
            bar(format!("1 {} *lr *a", pd(1))),
        ],
    )?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let b = |line: &str| bar(on(&BASS, 0, line));
    play_track(
        &mut s,
        1,
        &[
            b("8 r, 8 G1, 8 G1, 8 G1"),
            b("2 Eb2"),
            b("8 r, 8 F1, 8 F1, 8 F1"),
            b("2 D2"),
            b("2 D2 ~"),
            b("8 r, 8 G1, 8 G1, 8 G1, 2 Eb2"),
            b("8 r, 8 F1, 8 F1, 8 F1, 2 D2"),
            b("8 B1, 8 r, 8 B1, 8 r, 4 B1, 4 r"),
            b("1 C2"),
        ],
    )?;
    let motto = "8 r, 8 bd sd, 8 bd sd, 8 bd sd";
    let half = "8 r, 8 bd, 8 bd, 8 bd, 2 bd sd ch";
    drums(
        &mut s,
        &[
            bar(motto),
            bar("2 bd cr"),
            bar(motto),
            bar("2 bd cr"),
            bar("2 r"),
            bar(half),
            bar(half),
            bar("8 bd hh, 8 sd, 8 bd hh, 8 sd, 4 bd cr, 4 r"),
            bar("1 bd cr"),
        ],
    )?;
    Ok(s)
}

fn lagrima() -> R<Session> {
    let mut s = new_demo(
        "Lágrima",
        "Francisco Tárrega",
        "Tárrega's prelude for guitar: the E major half on nylon strings with chord names, then the E minor half as a distorted lead with slides over the full band.",
        72,
        "Nylon Guitar",
        24,
        "standard",
    )?;
    let e = "8 1:4 +lr @mp %E=022100, 8 2:0, 8 1:5, 8 2:0, 8 1:7, 8 2:0";
    let b7 = "8 1:2 4:1 *lr %B7=x21202, 8 2:0, 8 3:2, 8 2:0, 8 5:2, 8 2:0";
    let mut n: Vec<Bar> = vec![
        bar(e).mark("Andante").ts(3, 4).key(4),
        bar(b7),
        bar(e),
        bar(b7),
        bar("4 1:12, 4 1:11, 4 1:9"),
        bar("8 1:7, 8 1:4, 8 2:2, 8 1:0, 8 1:2, 8 1:5"),
        bar("8 1:4, 8 1:0, 8 2:2, 8 1:2, 8 1:2 ~, 8 2:4 3:2"),
        bar("2 1:0 3:1 *lr %E=022100, 4 r"),
    ];
    n.extend((0..8).map(|_| bar("2. r")));
    n[8] = bar("2. r").mark("Heavy").key(1).tempo(112);
    play_track(&mut s, 0, &n)?;
    add_track(&mut s, "Lead Guitar", "standard", 29)?;
    let mut l: Vec<Bar> = (0..8).map(|_| bar("2. r")).collect();
    l.extend(
        [
            "8 2:8 +sl, 8 2:13, 8 2:12, 8 2:5, 8 2:7 +sl, 8 2:8",
            "2. 2:5 +v",
            "8 r, 8 1:12, 8 r, 8 1:8, 8 r, 8 1:5",
            "2 2:7 +v, 8 r, 8 2:7",
            "4 2:8, 4 2:10, 4 2:12",
            "8 2:15, 8 2:13, 8 2:12, 8 2:10, 8 2:8, 8 2:7",
            "4 2:5, 4 2:7, 4 3:4",
            "2 2:5 +v, 4 r",
        ]
        .map(bar),
    );
    play_track(&mut s, 1, &l)?;
    add_track(&mut s, "Rhythm Guitar", "standard", 30)?;
    let chug = |chord: String| bar(format!("8 {chord} *pm @f, 8 {chord} *pm, 8 {chord} *pm, 8 {chord} *pm, 4 {chord} *a"));
    let mut r: Vec<Bar> = (0..8).map(|_| bar("2. r")).collect();
    r.extend([p5(5, 3), p5(5, 0), p5(5, 0), p5(5, 2), p5(6, 0), p5(5, 3), p5(5, 2)].map(chug));
    r.push(bar(format!("2. {} *lr", p5(6, 0))));
    play_track(&mut s, 2, &r)?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let mut b: Vec<Bar> = (0..8).map(|_| bar("2. r")).collect();
    b.extend(["3:3", "3:0", "3:0", "3:2", "4:0", "3:3", "3:2"].map(|n| bar(rep(6, &format!("8 {n}")))));
    b.push(bar("2. 4:0"));
    play_track(&mut s, 3, &b)?;
    let groove = |crash: bool| bar(format!("8 bd {}, 8 hh, 8 sd hh, 8 hh, 8 bd hh, 8 bd hh", if crash { "cr" } else { "hh" }));
    let mut d: Vec<Bar> = (0..8).map(|_| bar("2. r")).collect();
    d.extend([groove(true), groove(false), groove(false), groove(false), groove(true), groove(false), groove(false), bar("2. bd cr")]);
    drums(&mut s, &d)?;
    Ok(s)
}

fn caprice() -> R<Session> {
    let mut s = new_demo(
        "Caprice No. 24",
        "Niccolò Paganini",
        "The theme of the last of the 24 Caprices for solo violin (1817) as a lead-guitar solo in A minor: legato pull-offs, a bend and a shift slide over a galloping band.",
        112,
        "Lead Guitar",
        29,
        "standard",
    )?;
    let theme = [
        "8 1:5, 16 r, 16 1:5, 16 1:5, 16 1:8 +h, 16 1:7 +h, 16 1:5",
        "8 1:12, 16 r, 16 1:0, 16 1:0, 16 1:4 +h, 16 1:2 +h, 16 1:0",
        "8 1:5, 16 r, 16 1:5, 16 1:5, 16 1:8 +h, 16 1:7 +h, 16 1:5",
        "4 1:10 +b +v, 8 1:0, 8 r",
        "8 1:17, 16 r, 16 1:17, 16 1:17, 16 1:18 +h, 16 1:17 +h, 16 1:15",
        "8 1:13, 16 r, 16 1:10, 16 1:10, 16 1:13 +h, 16 1:12 +h, 16 1:10",
        "8 1:15, 16 r, 16 1:15, 16 1:15, 16 1:17 +h, 16 1:15 +h, 16 1:13",
        "8 1:12, 16 r, 16 1:8, 16 1:8, 16 1:12 +h, 16 1:10 +h, 16 1:8",
        "8 1:13, 16 r, 16 1:7, 16 1:7, 16 1:10 +h, 16 1:8 +h, 16 1:7",
        "8 1:12, 16 r, 16 1:5, 16 1:5, 16 1:8 +h, 16 1:7 +h, 16 1:5",
        "8 1:1, 16 r, 16 1:11, 16 1:0, 16 1:12 +sl, 16 1:10, 16 1:7",
        "4 1:5 +v, 8 3:2, 8 r",
        "2 1:17 +v &fl",
    ];
    let mut l: Vec<Bar> = theme.iter().map(bar).collect();
    l[0] = bar(theme[0]).ts(2, 4).mark("Tema").open();
    l[3] = bar(theme[3]).close(1);
    play_track(&mut s, 0, &l)?;
    add_track(&mut s, "Rhythm Guitar", "standard", 30)?;
    // A, E, A, E; then A, D, G, C, G, A, E, A.
    let roots = [(5u8, 0u8), (6, 0), (5, 0), (6, 0), (5, 0), (5, 5), (6, 3), (5, 3), (6, 3), (5, 0), (6, 0), (5, 0)];
    let mut r: Vec<Bar> = roots.iter().map(|&(st, f)| bar(rep(2, &gallop(st, f)))).collect();
    r.push(bar(format!("2 {} *lr", p5(5, 0))));
    play_track(&mut s, 1, &r)?;
    add_track(&mut s, "Bass", "bass", 34)?;
    let on_bass = |root: (u8, u8)| match root {
        (5, 0) => (3, 0),
        (6, 0) => (4, 0),
        (5, 5) => (2, 0),
        (6, 3) => (4, 3),
        _ => (3, 3),
    };
    let mut b: Vec<Bar> = roots.iter().map(|&root| on_bass(root)).map(|(st, f)| bar(rep(2, &gallop_open(st, f)))).collect();
    b.push(bar("2 3:0"));
    play_track(&mut s, 2, &b)?;
    let kick = |crash: bool| bar(format!("8 bd {}, 16 bd, 16 bd, 8 sd hh, 16 bd, 16 bd", if crash { "cr" } else { "hh" }));
    let mut d: Vec<Bar> = (0..12).map(|i| kick(i % 4 == 0)).collect();
    d.push(bar("2 bd cr"));
    drums(&mut s, &d)?;
    Ok(s)
}

fn techniques() -> R<Session> {
    let mut s = new_demo(
        "Metal Technique Showcase",
        "Guitario",
        "One bar per technique, then the odd meters: use it to check how every effect and time signature is drawn and played back.",
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
            bar("4 6:0 +pm &tp32, 4 6:1 +pm &tp32, 4 6:3 +pm &tp32, 4 6:1 +pm &tp32").mark("Tremolo picking"),
            bar("4 5:0 &tp8, 4 5:0 &tp16, 4 5:0 &tp32, 4 5:0 4:2 *pm &tp16").mark("Tremolo speeds"),
            bar("4 1:12 +tr14, 4 1:12 +tr14:32, 4 1:10 +tr12:64, 4 r").mark("Trills"),
            bar("4 6:0 &wdive:-8, 4 6:0 ~ &wreturn:-8, 4 3:9 &wdip:-6, 8 3:12 &wpredive:-2, 8 3:12 &wvw").mark("Whammy bar"),
            bar("4 3:7 +gr5, 4 3:7 +gr5h, 4 3:9 +gr7s, 4 2:8 +gr6bo").mark("Grace notes"),
            bar("4 3:5 +lf1 +rfp, 4 3:7 +lf3 +rfi, 4 2:5 +lf1 +rfm, 4 2:6 +lf2 +rfa").mark("Fingering"),
            bar("8 6:0 +pm, 8 6:x, 8 5:2 +g, 8 6:x, 8 6:0, 8 6:x, 8 5:2 +a, 8 6:x").mark("Dead, ghost, accent"),
            bar("4 1:5 @p, 4 1:5 ~, 2 1:7 @ff").mark("Ties and dynamics"),
            bar("8t 1:0, 8t 1:3, 8t 1:5, 8t 1:0, 8t 1:3, 8t 1:5, 4 1:7, 4 r").mark("Triplets"),
            bar(format!("8 6:0 +pm, 8 6:0 +pm, 8 {} *a, 8 6:0 +pm, 8 {} *a, 8 6:0 +pm, 8 {} *a", p5(6, 3), p5(6, 5), p5(6, 7)))
                .ts(7, 8)
                .beams(&[3, 2, 2])
                .mark("7/8 (3+2+2)"),
            bar(format!(
                "{}, 16 {} *a, {}, 16 {} *a, {}, 16 {} *a",
                rep(3, "16 6:0 +pm"),
                p5(6, 3),
                rep(3, "16 6:0 +pm"),
                p5(6, 5),
                rep(4, "16 6:0 +pm"),
                p5(6, 7)
            ))
            .ts(13, 16)
            .mark("13/16 (4+4+5)"),
            bar(format!("4. {} *lr, 8 {}, 8 {}, 8 {}", p5(6, 0), p5(6, 3), p5(6, 5), p5(6, 3))).ts(6, 8).mark("6/8"),
            bar(format!("4. {} *lr, 4. {}, 4. {}, 4. {} *a", p5(6, 0), p5(6, 0), p5(6, 3), p5(6, 5))).ts(12, 8).mark("12/8"),
            bar("4 6:0 5:2 4:2 3:1 2:0 1:0 *lr @mf, 4 r, 2 r").ts(4, 4),
        ],
    )?;
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

fn cello_prelude() -> R<Session> {
    let mut s = new_demo(
        "Cello Suite No. 1: Prelude",
        "Johann Sebastian Bach",
        "The opening of the Prelude from BWV 1007 played on a picked bass, an octave below the cello, over a ride beat. Practise slowly with the metronome on.",
        84,
        "Picked Bass",
        34,
        "bass",
    )?;
    let twice = |notes: &str| on(&BASS, 0, &sixteenths(&format!("{notes} {notes}")));
    play_track(
        &mut s,
        0,
        &[
            bar(twice("G1 D2 B2 A2 B2 D2 B2 D2")).mark("Prelude").key(1),
            bar(twice("G1 E2 C3 B2 C3 E2 C3 E2")),
            bar(twice("G1 F#2 C3 B2 C3 F#2 C3 F#2")),
            bar(on(&BASS, 0, &sixteenths("G1 G2 B2 A2 B2 G2 B2 G2 G1 G2 B2 A2 B2 G2 B2 F#2"))),
            bar(on(&BASS, 0, &sixteenths("G1 E2 B2 A2 B2 G2 F#2 G2 E2 G2 F#2 G2 B1 D2 C#2 B1"))),
            bar(on(&BASS, 0, "1 G1 +lr")),
        ],
    )?;
    let d = |crash| bar(eighths("rd", crash));
    drums(&mut s, &[d(true), d(false), d(false), d(false), d(false), bar("1 bd cr")])?;
    Ok(s)
}

fn bourree() -> R<Session> {
    let mut s = new_demo(
        "Bourrée in E minor",
        "Johann Sebastian Bach",
        "The first half of the Bourrée from the Lute Suite BWV 996: the melody is Voice 1 (stems up), the bass is Voice 2 (stems down), on one staff. It starts with a one-beat pickup, so the last bar is three beats long.",
        120,
        "Nylon Guitar",
        24,
        "standard",
    )?;
    let melody = [
        "8 E4 @mp, 8 F#4",
        "4 G4, 8 F#4, 8 E4, 4 D#4, 8 E4, 8 F#4",
        "4 B3, 8 C#4, 8 D#4, 4 E4, 8 D4, 8 C4",
        "4 B3, 8 A3, 8 G3, 4 F#3, 8 G3, 8 A3",
        "8 B3, 8 A3, 8 G3, 8 F#3, 4 E3, 8 E4, 8 F#4",
        "4 G4, 8 F#4, 8 E4, 4 D#4, 8 E4, 8 F#4",
        "4 B3, 8 C#4, 8 D#4, 4 E4, 8 D4, 8 C4",
        "4 B3, 8 A3, 8 G3, 4. F#3, 8 G3",
        "2. B2 D3 G3 *lr",
    ];
    // Bach's low D at the end of bar 8 is below the guitar, so both Ds sound an octave up, on the
    // A string to leave the D string to the melody's F#.
    let bass = [
        "8 G2, 8 F#2",
        "4 E2, 4 A2, 4 B2, 4 A2",
        "4 G2, 4 F#2, 4 E2, 4 F#2",
        "4 G2, 4 A2, 4 B2, 4 A2",
        "4 G2, 4 B2, 8 E2, 8 F#2, 8 G2, 8 F#2",
        "4 E2, 4 A2, 4 B2, 4 A2",
        "4 G2, 4 F#2, 4 E2, 4 F#2",
        "4 G2, 4 C3, 4 5:5, 4 5:5",
        "2. G2",
    ];
    let bars: Vec<Bar> = melody
        .iter()
        .zip(bass)
        .enumerate()
        .map(|(i, (m, b))| {
            let v = bar(on(&STANDARD, 0, m)).with_voice2(on(&STANDARD, 0, b));
            match i {
                0 => v.ts(1, 4).key(1).mark("Bourrée"),
                1 => v.ts(2, 2),
                8 => v.ts(3, 4),
                _ => v,
            }
        })
        .collect();
    play_track(&mut s, 0, &bars)?;
    Ok(s)
}

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos")
}

fn main() -> ExitCode {
    let dir = demo_dir();
    for sub in ["", "midi", "tab", "gp5", "musicxml"] {
        if let Err(e) = std::fs::create_dir_all(dir.join(sub)) {
            eprintln!("cannot create {}: {e}", dir.join(sub).display());
            return ExitCode::FAILURE;
        }
    }
    // Replace the whole suite: drop outputs of demos that no longer exist.
    for (sub, ext) in [("", "gt"), ("midi", "mid"), ("tab", "txt"), ("gp5", "gp5"), ("musicxml", "musicxml")] {
        if let Ok(entries) = std::fs::read_dir(dir.join(sub)) {
            for e in entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == ext)) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    type Make = fn() -> R<Session>;
    let all: [(&str, Make); 16] = [
        ("00-blank", blank),
        ("01-flight-of-the-bumblebee", bumblebee),
        ("02-funeral-march", funeral_march),
        ("03-hall-of-the-mountain-king", mountain_king),
        ("04-summer-presto", summer),
        ("05-toccata-and-fugue", toccata),
        ("06-mars", mars),
        ("07-little-fugue", little_fugue),
        ("08-moonlight-sonata", moonlight),
        ("09-fifth-symphony", fifth_symphony),
        ("10-lagrima", lagrima),
        ("11-caprice-24", caprice),
        ("12-metal-technique-showcase", techniques),
        ("13-cello-prelude", cello_prelude),
        ("14-drum-grooves", drum_grooves),
        ("15-bourree-in-e-minor", bourree),
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
