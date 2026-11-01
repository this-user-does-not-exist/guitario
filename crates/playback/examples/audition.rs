//! Renders auditions of the modelled instruments (grand piano, violin family, string sections and
//! a string orchestra) to WAV files, through the same player the app uses:
//! `cargo run -p guitario-playback --release --example audition -- <dir> [name …]`.
//! `-- <dir> --keys C4@mf A0@ff …` renders single piano keys instead, each held until it has died
//! away, to measure against recordings of a real piano.
//!
//! Each audition is an original phrase written in a small notation: bars split by `|`, beats by
//! spaces. A beat is a note value (`1 2 4 8 16`, `.` dotted), then `:` and its notes as names
//! (`C4`, `F#2`, `Bb5`, comma-separated for a chord) or `r` for a rest, then optional `@` and a
//! dynamic (`pp p mp mf f ff fff`) and `!` flags: `l` let ring (the sustain pedal on a piano),
//! `v` vibrato, `w` wide vibrato, `h` legato from the note before on its string (a slur), `s`
//! staccato. Several parts are layered as voices of the bar.

// The auditions index fixed literal tables.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use guitario_score::{Beat, Dynamic, MeasureHeader, Note, Score, Track, Voice};
use guitario_theory::{Duration, NoteValue, TimeSignature, Tuning};
use std::path::PathBuf;

const RATE: u32 = 48_000;

fn pitch(name: &str) -> u8 {
    let mut chars = name.chars();
    let letter = chars.next().expect("note letter");
    let base: i32 = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => panic!("bad note {name}"),
    };
    let rest: String = chars.collect();
    let (acc, octave) = if let Some(r) = rest.strip_prefix('#') {
        (1, r)
    } else if let Some(r) = rest.strip_prefix('b') {
        (-1, r)
    } else {
        (0, rest.as_str())
    };
    let octave: i32 = octave.parse().expect("octave");
    (12 * (octave + 1) + base + acc) as u8
}

fn duration(s: &str) -> Duration {
    let dots = s.chars().filter(|c| *c == '.').count() as u8;
    let value = match s.trim_end_matches('.') {
        "1" => NoteValue::Whole,
        "2" => NoteValue::Half,
        "4" => NoteValue::Quarter,
        "8" => NoteValue::Eighth,
        "16" => NoteValue::Sixteenth,
        "32" => NoteValue::ThirtySecond,
        v => panic!("bad duration {v}"),
    };
    Duration::new(value).dotted(dots)
}

fn dynamic(s: &str) -> Dynamic {
    match s {
        "ppp" => Dynamic::Ppp,
        "pp" => Dynamic::Pp,
        "p" => Dynamic::P,
        "mp" => Dynamic::Mp,
        "mf" => Dynamic::Mf,
        "f" => Dynamic::F,
        "ff" => Dynamic::Ff,
        _ => Dynamic::Fff,
    }
}

/// The string and fret for each note of a chord: the string closest above, each used once.
fn place(tuning: &[u8], notes: &[u8]) -> Vec<(u8, u8)> {
    let mut used = vec![false; tuning.len()];
    let mut sorted = notes.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let mut out = Vec::new();
    for n in sorted {
        let best = (0..tuning.len()).filter(|&s| !used[s] && tuning[s] <= n && n - tuning[s] <= 24).min_by_key(|&s| n - tuning[s]);
        let s = best.unwrap_or_else(|| panic!("no string for {n} in {tuning:?}"));
        used[s] = true;
        out.push((s as u8, n - tuning[s]));
    }
    out
}

fn beat(tuning: &[u8], token: &str, current: &mut Dynamic) -> Beat {
    let (body, flags) = token.split_once('!').unwrap_or((token, ""));
    let (body, dyn_) = body.split_once('@').map_or((body, None), |(a, b)| (a, Some(b)));
    if let Some(d) = dyn_ {
        *current = dynamic(d);
    }
    let (dur, notes) = body.split_once(':').expect("dur:notes");
    let mut b = Beat::rest(duration(dur));
    b.dynamic = *current;
    if notes == "r" {
        return b;
    }
    b.rest = false;
    let pitches: Vec<u8> = notes.split(',').map(pitch).collect();
    for (string, fret) in place(tuning, &pitches) {
        let mut n = Note::new(string, fret);
        n.effects.let_ring = flags.contains('l');
        n.effects.vibrato = flags.contains('v');
        n.effects.wide_vibrato = flags.contains('w');
        n.effects.staccato = flags.contains('s');
        b.notes.push(n);
    }
    b
}

struct Part {
    name: &'static str,
    program: u8,
    tuning: Vec<u8>,
    /// Voices, each a string of bars.
    voices: Vec<&'static str>,
    /// Players of a section (0 is one), and the track's pan.
    players: u8,
    pan: i8,
}

impl Part {
    fn new(name: &'static str, program: u8, tuning: Vec<u8>, voices: Vec<&'static str>) -> Part {
        Part { name, program, tuning, voices, players: 0, pan: 0 }
    }

    /// Played by a section of `players`, placed at `pan`.
    fn section(self, players: u8, pan: i8) -> Part {
        Part { players, pan, ..self }
    }
}

const VIOLIN: [u8; 4] = [76, 69, 62, 55];
const VIOLA: [u8; 4] = [69, 62, 55, 48];
const CELLO: [u8; 4] = [57, 50, 43, 36];
const BASS: [u8; 4] = [43, 38, 33, 28];

fn score(tempo: u16, parts: &[Part]) -> Score {
    let bars = parts.iter().flat_map(|p| p.voices.iter()).map(|v| v.split('|').count()).max().unwrap_or(1);
    let mut s = Score::new();
    let mut h = MeasureHeader::new(TimeSignature { numerator: 4, denominator: 4 });
    h.tempo = Some(tempo);
    s.headers = vec![h];
    for _ in 1..bars {
        s.headers.push(MeasureHeader::new(TimeSignature { numerator: 4, denominator: 4 }));
    }
    s.tracks.clear();
    for p in parts {
        let mut t = Track::new(p.name, Tuning { name: p.name.into(), strings: p.tuning.clone() }, &s.headers);
        t.program = p.program;
        t.players = p.players;
        t.pan = p.pan;
        t.interpretation.stringed = !matches!(p.program, 0..=7);
        for (vi, text) in p.voices.iter().enumerate() {
            let mut dynamic = Dynamic::Mf;
            for (mi, bar) in text.split('|').enumerate() {
                let beats: Vec<Beat> = bar.split_whitespace().map(|tok| beat(&p.tuning, tok, &mut dynamic)).collect();
                let m = &mut t.measures[mi];
                while m.voices.len() <= vi {
                    m.voices.push(Voice::default());
                }
                m.voices[vi].beats = beats;
            }
        }
        s.tracks.push(t);
    }
    s
}

/// A piano "tuning": twelve strings a fifth or so apart, so any chord finds strings.
fn keyboard() -> Vec<u8> {
    vec![103, 96, 89, 82, 75, 68, 61, 54, 47, 40, 33, 21]
}

fn auditions() -> Vec<(&'static str, Score)> {
    let piano = |voices: Vec<&'static str>| Part::new("Piano", 0, keyboard(), voices);
    vec![
        (
            "piano-range",
            // Single keys up the keyboard at mezzo-forte, each held, then let go.
            score(
                60,
                &[piano(vec![
                    "2:A0 2:r | 2:E1 2:r | 2:C2 2:r | 2:G2 2:r | 2:C3 2:r | 2:E3 2:r | 2:A3 2:r | 2:C4 2:r | 2:E4 2:r | 2:A4 2:r | 2:C5 2:r | 2:E5 2:r | 2:A5 2:r | 2:C6 2:r | 2:E6 2:r | 2:A6 2:r | 2:C7 2:r | 2:G7 2:r | 2:C8 2:r",
                ])],
            ),
        ),
        (
            "piano-dynamics",
            // Middle C from ppp to fff: brighter as well as louder.
            score(80, &[piano(vec!["2:C4@ppp 2:C4@pp | 2:C4@p 2:C4@mp | 2:C4@mf 2:C4@f | 2:C4@ff 2:C4@fff | 1:C2,C3@pp | 1:C2,C3@fff"])]),
        ),
        (
            "piano-sustain",
            // A long chord left to ring, then the same struck again over it: beating, double decay.
            score(60, &[piano(vec!["1:E2,B2,E3,G3,B3@f!l | 1:r | 1:r | 1:E2,B2,E3,G3,B3@p!l | 1:r | 1:r"])]),
        ),
        (
            "piano-ballad",
            // A dark ballad in E minor: left hand rolling, right hand singing over it.
            score(
                72,
                &[piano(vec![
                    "4:B4@mf!l 4.:G4!l 8:A4!l | 2:B4!l 4:D5!l 4:C5!l | 4.:B4!l 8:A4!l 4:G4!l 4:F#4!l | 2.:E4!l 4:r | 4:B4@f!l 4.:E5!l 8:D5!l | 4:C5!l 4:B4!l 4:A4!l 4:G4!l | 4:F#4!l 4:G4!l 4:A4!l 4:B4!l | 1:E4@mp!l",
                    "8:E2@mp!l 8:B2!l 8:E3!l 8:G3!l 8:B3!l 8:G3!l 8:E3!l 8:B2!l | 8:C2!l 8:G2!l 8:C3!l 8:E3!l 8:G3!l 8:E3!l 8:C3!l 8:G2!l | 8:G1!l 8:D2!l 8:G2!l 8:B2!l 8:D3!l 8:B2!l 8:G2!l 8:D2!l | 8:B1!l 8:F#2!l 8:B2!l 8:D#3!l 8:F#3!l 8:D#3!l 8:B2!l 8:F#2!l | 8:E2@f!l 8:B2!l 8:E3!l 8:G3!l 8:B3!l 8:G3!l 8:E3!l 8:B2!l | 8:A1!l 8:E2!l 8:A2!l 8:C3!l 8:E3!l 8:C3!l 8:A2!l 8:E2!l | 8:B1!l 8:F#2!l 8:B2!l 8:D#3!l 8:F#3!l 8:D#3!l 8:B2!l 8:A2!l | 1:E1,E2,B2@mp!l",
                ])],
            ),
        ),
        (
            "piano-riff",
            // A palm-muted-guitar style riff on the piano: staccato low octaves, accents on top.
            score(
                132,
                &[piano(vec![
                    "8:E1,E2@f!s 8:E1,E2!s 8:E1,E2!s 8:G1,G2!s 8:E1,E2!s 8:E1,E2!s 8:A#1,A#2@ff 8:A1,A2 | 8:E1,E2@f!s 8:E1,E2!s 8:E1,E2!s 8:G1,G2!s 8:E1,E2!s 8:E1,E2!s 4:F1,F2@ff | 8:E1,E2@f!s 8:E1,E2!s 8:E1,E2!s 8:G1,G2!s 8:E1,E2!s 8:E1,E2!s 8:A#1,A#2@ff 8:A1,A2 | 2:E1,E2,B2,E3@fff!l 2:r",
                ])],
            ),
        ),
        (
            "violin-solo",
            // Long notes swell into vibrato, then détaché eighths (a bow change each), then a slur.
            score(
                76,
                &[Part::new("Violin", 40, vec![76, 69, 62, 55], vec![
                        "2:B4@mp 2:E5 | 4.:G5@mf 8:F#5 4:E5 4:D5 | 1:E5 | 8:B4@f 8:C5 8:D5 8:E5 8:F#5 8:G5 8:A5 8:B5 | 4:C6@ff 4:B5 4:A5 4:G5 | 8:F#5@mf 8:G5!h 8:F#5!h 8:E5!h 2:D#5 | 1:E5@p",
                    ])],
            ),
        ),
        (
            "cello-solo",
            score(
                66,
                &[Part::new("Cello", 42, vec![57, 50, 43, 36], vec!["2:E2@mp 2:B2 | 4.:G3@mf 8:F#3 4:E3 4:D3 | 1:E3 | 8:B2@f 8:C3 8:D3 8:E3 8:F#3 8:G3 8:A3 8:B3 | 2:C4@ff 2:B3 | 1:E3@p"])],
            ),
        ),
        (
            "viola-solo",
            score(
                72,
                &[Part::new("Viola", 41, vec![69, 62, 55, 48], vec!["2:E3@mp 2:B3 | 4.:G4@mf 8:F#4 4:E4 4:D4 | 1:E4 | 1:B3@p"])],
            ),
        ),
        (
            "double-bass",
            score(60, &[Part::new("Double Bass", 43, vec![43, 38, 33, 28], vec!["2:E1@mf 2:B1 | 2:C2 2:D2 | 1:E1@f | 1:E1@p"])]),
        ),
        (
            "string-section",
            // A string orchestra on one track: E minor, C, G, B, the voices spread over four
            // octaves, each note played by four players.
            score(
                60,
                &[Part::new("Strings", 48, vec![103, 96, 89, 82, 75, 68, 61, 54, 47, 40, 33, 21], vec![
                        "1:E2,B2,G3,E4,B4@mp | 1:C2,G2,E3,G4,C5@mf | 1:G1,D2,B3,G4,D5@f | 1:B1,F#2,D#4,F#4,B4@ff | 1:E2,B2,G3,E4,B4,E5@mp | 1:E2,B2,G3,E4,B4,E5@pp",
                    ])],
            ),
        ),
        (
            "string-tremolo",
            score(
                90,
                &[Part::new("Tremolo Strings", 44, vec![103, 96, 89, 82, 75, 68, 61, 54, 47, 40, 33, 21], vec!["1:E3,B3,E4,G4@p | 1:E3,B3,E4,G4@f | 1:F3,C4,F4,A4@ff | 1:E3,B3,E4,G#4@mp"])],
            ),
        ),
        (
            "violin-section",
            // The violin's phrase by sixteen first violins.
            score(
                76,
                &[Part::new(
                    "Violins I",
                    40,
                    VIOLIN.to_vec(),
                    vec![
                        "2:B4@mp 2:E5 | 4.:G5@mf 8:F#5 4:E5 4:D5 | 1:E5 | 8:B4@f 8:C5 8:D5 8:E5 8:F#5 8:G5 8:A5 8:B5 | 4:C6@ff 4:B5 4:A5 4:G5 | 8:F#5@mf 8:G5!h 8:F#5!h 8:E5!h 2:D#5 | 1:E5@p",
                    ],
                )
                .section(16, 0)],
            ),
        ),
        (
            "viola-section",
            score(72, &[Part::new("Violas", 41, VIOLA.to_vec(), vec!["2:E3@mp 2:B3 | 4.:G4@mf 8:F#4 4:E4 4:D4 | 1:E4 | 1:B3@p"]).section(12, 0)]),
        ),
        (
            "cello-section",
            score(
                66,
                &[Part::new("Cellos", 42, CELLO.to_vec(), vec!["2:E2@mp 2:B2 | 4.:G3@mf 8:F#3 4:E3 4:D3 | 1:E3 | 8:B2@f 8:C3 8:D3 8:E3 8:F#3 8:G3 8:A3 8:B3 | 2:C4@ff 2:B3 | 1:E3@p"])
                    .section(10, 0)],
            ),
        ),
        (
            "bass-section",
            score(60, &[Part::new("Basses", 43, BASS.to_vec(), vec!["2:E1@mf 2:B1 | 2:C2 2:D2 | 1:E1@f | 1:E1@p"]).section(8, 0)]),
        ),
        (
            "violins-divisi",
            // Sixteen violins dividing three- and two-note chords among them.
            score(66, &[Part::new("Violins I", 40, VIOLIN.to_vec(), vec!["1:E4,B4,G5@mp | 1:E4,C5,G5@mf | 1:F#4,B4,D#5@f | 2:G4,E5@mf 2:B4,E5@p"]).section(16, 0)]),
        ),
        (
            "string-orchestra",
            // An orchestra's five string sections, each on its track and seated: a dark chorale in
            // E minor.
            score(
                60,
                &[
                    Part::new("Violins I", 40, VIOLIN.to_vec(), vec!["2:B4@mp 2:E5 | 4.:G5@mf 8:F#5 4:E5 4:D5 | 2:C5 2:B4 | 1:B4 | 2:E5@f 2:G5 | 2:F#5 4:E5 4:D#5 | 1:E5@mp | 1:E5@pp"])
                        .section(16, -40),
                    Part::new("Violins II", 40, VIOLIN.to_vec(), vec!["2:G4@mp 2:G4 | 2:B4@mf 2:A4 | 2:E4 2:F#4 | 1:F#4 | 2:B4@f 2:B4 | 2:A4 2:F#4 | 1:G4@mp | 1:G4@pp"])
                        .section(14, -15),
                    Part::new("Violas", 41, VIOLA.to_vec(), vec!["2:E4@mp 2:B3 | 2:E4@mf 2:F#4 | 2:A3 2:D#4 | 1:D#4 | 2:G4@f 2:E4 | 2:C4 2:B3 | 1:B3@mp | 1:B3@pp"]).section(12, 15),
                    Part::new("Cellos", 42, CELLO.to_vec(), vec!["2:E3@mp 2:E3 | 2:E3@mf 2:D3 | 2:A2 2:B2 | 1:B2 | 2:E3@f 2:E3 | 2:A2 2:B2 | 1:E3@mp | 1:E2@pp"]).section(10, 35),
                    Part::new("Basses", 43, BASS.to_vec(), vec!["2:E2@mp 2:E2 | 2:C2@mf 2:D2 | 2:A1 2:B1 | 1:B1 | 2:C2@f 2:C2 | 2:A1 2:B1 | 1:E1@mp | 1:E1@pp"]).section(8, 45),
                ],
            ),
        ),
        (
            "low-strings-riff",
            // Cellos and basses driving a staccato riff, violins holding over it.
            score(
                120,
                &[
                    Part::new("Violins I", 40, VIOLIN.to_vec(), vec!["1:B4@mf | 1:C5 | 1:B4 | 2:E5@f 2:D#5"]).section(16, -40),
                    Part::new(
                        "Cellos",
                        42,
                        CELLO.to_vec(),
                        vec![
                            "8:E2@f!s 8:E2!s 8:E2!s 8:G2 8:E2!s 8:E2!s 8:A#2@ff 8:A2 | 8:E2@f!s 8:E2!s 8:E2!s 8:G2 8:E2!s 8:E2!s 4:F2@ff | 8:E2@f!s 8:E2!s 8:E2!s 8:G2 8:E2!s 8:E2!s 8:A#2@ff 8:A2 | 2:B2@ff 2:B2",
                        ],
                    )
                    .section(10, 35),
                    Part::new(
                        "Basses",
                        43,
                        BASS.to_vec(),
                        vec![
                            "8:E1@f!s 8:E1!s 8:E1!s 8:G1 8:E1!s 8:E1!s 8:A#1@ff 8:A1 | 8:E1@f!s 8:E1!s 8:E1!s 8:G1 8:E1!s 8:E1!s 4:F1@ff | 8:E1@f!s 8:E1!s 8:E1!s 8:G1 8:E1!s 8:E1!s 8:A#1@ff 8:A1 | 2:B1@ff 2:B1",
                        ],
                    )
                    .section(8, 45),
                ],
            ),
        ),
        (
            "guitar-reference",
            // The plucked-string synth at the same dynamics, to set the models' levels against.
            score(
                80,
                &[Part::new("Guitar", 25, vec![64, 59, 55, 50, 45, 40], vec!["2:E2@mf 2:B3 | 2:E4 2:G3 | 1:E2,B2,E3,G3,B3,E4 | 1:r"])],
            ),
        ),
        (
            "pizzicato",
            score(
                100,
                &[Part::new("Pizzicato Strings", 45, vec![103, 96, 89, 82, 75, 68, 61, 54, 47, 40, 33, 21], vec!["4:E3@mf 4:G3 4:B3 4:E4 | 4:C3 4:E3 4:G3 4:C4 | 4:D3 4:F#3 4:A3 4:D4 | 2:E2,B2,E3,G3@f 2:r"])],
            ),
        ),
    ]
}

/// Single long notes for comparing with recordings of real players: each instrument at a few
/// pitches across its range, alone and as an orchestra's section (`ref-cello-10-a2` is ten
/// cellos on A2), at mezzo-forte, five seconds each. Rendered only when asked for (`ref`, or by
/// name).
fn references() -> Vec<(String, Score)> {
    type Instrument<'a> = (&'a str, u8, &'a [u8], u8, &'a [&'a str]);
    let instruments: [Instrument; 4] = [
        ("violin", 40, &VIOLIN, 16, &["G3", "C#4", "E4", "A#4", "C#5", "E5", "G5", "A#5"]),
        ("viola", 41, &VIOLA, 12, &["C3", "D#3", "F#3", "A3", "C4", "D#4", "F#4", "A4"]),
        ("cello", 42, &CELLO, 10, &["C2", "D#2", "F#2", "A2", "C3", "D#3", "F#3", "A3", "C4"]),
        ("bass", 43, &BASS, 8, &["F#1", "A1", "C2", "D#2", "F#2", "A2", "C3"]),
    ];
    let mut out = Vec::new();
    for (name, program, tuning, section, notes) in instruments {
        for players in [1, section] {
            for note in notes {
                let voice: &'static str = Box::leak(format!("1:{note}@mf | 1:r").into_boxed_str());
                let part = Part::new("Strings", program, tuning.to_vec(), vec![voice]).section(players, 0);
                out.push((format!("ref-{name}-{players}-{}", note.to_lowercase()), score(48, &[part])));
            }
        }
    }
    out
}

/// Single piano keys, each struck once and held until it has died away, to set beside recordings
/// of a real piano (`--keys C4@mf A0@ff …`): one WAV per key, `piano.<dynamic>.<key>.wav`.
fn keys(list: &[String]) -> Vec<(String, Score)> {
    list.iter()
        .map(|k| {
            let (note, dyn_) = k.split_once('@').unwrap_or((k, "mf"));
            // Bars of 8 s: the bass rings for half a minute, the treble for a few seconds.
            let bars = match pitch(note) {
                0..=47 => 5,
                48..=71 => 3,
                _ => 2,
            };
            let text: &'static str = Box::leak(format!("1:{note}@{dyn_}!l{}", " | 1:r".repeat(bars - 1)).into_boxed_str());
            (format!("piano.{dyn_}.{note}"), score(30, &[Part::new("Piano", 0, keyboard(), vec![text])]))
        })
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| "auditions".into()));
    let mut only: Vec<String> = args.collect();
    std::fs::create_dir_all(&dir).expect("output directory");
    let list = if only.first().is_some_and(|a| a == "--keys") {
        let list = keys(&only[1..]);
        only.clear();
        list
    } else {
        auditions().into_iter().map(|(n, s)| (n.to_string(), s)).chain(references()).collect()
    };
    for (name, score) in list {
        let reference = name.starts_with("ref-");
        let asked = only.iter().any(|o| *o == name || (reference && o == "ref"));
        if (only.is_empty() && reference) || (!only.is_empty() && !asked) {
            continue;
        }
        if let Err(e) = score.validate() {
            eprintln!("{name}: invalid score: {e}");
            continue;
        }
        let started = std::time::Instant::now();
        let samples = guitario_playback::render_wav(&score, RATE);
        let secs = samples.len() as f64 / 2.0 / f64::from(RATE);
        let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let path = dir.join(format!("{name}.wav"));
        std::fs::write(&path, guitario_playback::wav_bytes(&samples, RATE)).expect("write");
        println!("{name}: {secs:.1} s of sound in {:.2} s, peak {peak:.2} -> {}", started.elapsed().as_secs_f64(), path.display());
    }
}
