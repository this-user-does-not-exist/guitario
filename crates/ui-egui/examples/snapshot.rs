//! Renders the editor offscreen to a PNG (no window, no focus stealing):
//! `cargo run -p guitario-ui-egui --example snapshot -- out.png [width height] [--tab-only] [--print] [--preview] [--page-setup] [--trill] [--tremolo] [--grace] [--chord] [--paper letter] [--landscape]`

use guitario_engine::Session;
use guitario_ui_egui::GuitarioApp;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, p: Value) {
    if let Err(e) = s.run(id, &p) {
        eprintln!("{id} {p}: {e}");
    }
}

/// One beat: (duration divisor, dots, [(string, fret)], effects).
type DemoBeat<'a> = (u32, u8, &'a [(u8, u8)], &'a [&'a str]);

fn enter(s: &mut Session, beats: &[DemoBeat]) {
    for (div, dots, notes, fx) in beats {
        run(s, "beat.setDuration", json!({"value": div, "dots": dots}));
        if notes.is_empty() {
            run(s, "beat.toggleRest", Value::Null);
        }
        for (string, fret) in notes.iter() {
            run(s, "cursor.set", json!({"string": string}));
            run(s, "note.setFret", json!({"fret": fret}));
            for f in fx.iter() {
                match *f {
                    "bend" => run(s, "note.setBend", json!({"quarterTones": 4})),
                    "slide" => run(s, "note.setSlide", json!({"slide": "shift"})),
                    "slideIn" => run(s, "note.setSlide", json!({"slide": "intoFromBelow"})),
                    "dead" => run(s, "note.toggleDead", Value::Null),
                    e => run(s, "note.toggleEffect", json!({"effect": e})),
                }
            }
        }
        run(s, "cursor.right", Value::Null);
    }
}

fn demo() -> Session {
    let mut s = Session::new();
    run(&mut s, "score.setMeta", json!({"title": "Demo Riff", "artist": "Guitario", "author": "Guitario contributors"}));
    run(&mut s, "score.setTempo", json!({"bpm": 104}));
    run(&mut s, "track.rename", json!({"name": "Electric Guitar"}));
    run(&mut s, "track.setProgram", json!({"program": 29}));
    run(&mut s, "measure.setMarker", json!({"text": "Intro"}));
    run(&mut s, "measure.setRepeat", json!({"open": true}));
    let pm: &[&str] = &["palmMute"];
    let none: &[&str] = &[];
    // Bar 1: palm-muted eighths on the low E with a power chord.
    enter(
        &mut s,
        &[
            (8, 0, &[(5, 0)], pm),
            (8, 0, &[(5, 0)], pm),
            (8, 0, &[(5, 3), (4, 5)], none),
            (8, 0, &[(5, 0)], pm),
            (8, 0, &[(5, 0)], pm),
            (8, 0, &[(5, 5), (4, 7)], none),
            (4, 0, &[(5, 3), (4, 5)], &["vibrato"]),
        ],
    );
    // Bar 2: sixteenths, hammer-on and a slide.
    enter(
        &mut s,
        &[
            (16, 0, &[(3, 5)], &["hammer"]),
            (16, 0, &[(3, 7)], none),
            (16, 0, &[(2, 5)], none),
            (16, 0, &[(2, 8)], &["slide"]),
            (8, 0, &[(2, 10)], none),
            (8, 0, &[(1, 8)], none),
            (4, 1, &[(1, 10)], &["bend"]),
            (8, 0, &[], none),
        ],
    );
    run(&mut s, "cursor.set", json!({"measure": 1}));
    run(&mut s, "measure.setRepeat", json!({"close": 1}));
    run(&mut s, "cursor.set", json!({"measure": 2, "beat": 0}));
    // Bar 3: chords and a triplet group.
    run(&mut s, "measure.setMarker", json!({"text": "Verse"}));
    enter(
        &mut s,
        &[
            (2, 0, &[(5, 0), (4, 2), (3, 2), (2, 1), (1, 0), (0, 0)], &["letRing"]),
            (4, 0, &[(4, 0), (3, 2), (2, 2), (1, 2), (0, 0)], &["letRing"]),
            (8, 0, &[(1, 3)], none),
            (8, 0, &[(0, 3)], none),
        ],
    );
    run(&mut s, "measure.setTempo", json!({"bpm": 112}));
    enter(
        &mut s,
        &[(4, 0, &[(2, 7)], &["slideIn"]), (8, 0, &[(2, 5)], none), (8, 0, &[(3, 7)], none), (4, 0, &[(3, 5)], none), (4, 0, &[(4, 7)], &["accent"])],
    );
    for _ in 0..3 {
        enter(
            &mut s,
            &[
                (8, 0, &[(5, 0)], pm),
                (8, 0, &[(5, 0)], pm),
                (8, 0, &[(5, 7)], &["dead"]),
                (8, 0, &[(5, 0)], pm),
                (4, 0, &[(5, 5), (4, 7), (3, 7)], none),
                (4, 0, &[(5, 3), (4, 5), (3, 5)], none),
            ],
        );
    }
    run(&mut s, "cursor.set", json!({"measure": 1, "beat": 6, "string": 1}));
    run(&mut s, "track.add", json!({"name": "Bass", "tuning": "bass"}));
    run(&mut s, "track.select", json!({"index": 0}));
    run(&mut s, "cursor.set", json!({"measure": 1, "beat": 6, "string": 1}));
    s
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| "snapshot.png".into());
    let w: f32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(1440.0);
    let h: f32 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(900.0);
    // `--file x.gt` renders a saved score instead of the built-in demo; `--zoom z` and
    // `--track n` pick the view.
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let mut session = demo();
    if let Some(path) = value("--file") {
        session = Session::new();
        run(&mut session, "file.open", json!({"path": path}));
    }
    if let Some(t) = value("--track").and_then(|t| t.parse::<usize>().ok()) {
        run(&mut session, "track.select", json!({"index": t}));
    }
    // `--voice n` puts the cursor in Voice n (1-4); `--bar b` and `--beat k` (1-based) move it.
    if let Some(v) = value("--voice").and_then(|v| v.parse::<u64>().ok()) {
        run(&mut session, "edit.setVoice", json!({"voice": v}));
    }
    let number = |flag: &str| value(flag).and_then(|v| v.parse::<usize>().ok()).and_then(|v| v.checked_sub(1));
    if let Some(bar) = number("--bar") {
        run(&mut session, "cursor.set", json!({"measure": bar, "beat": number("--beat").unwrap_or(0)}));
    }
    // `--paper letter` and `--landscape` set the page; `--preview`, `--page-setup` and `--print` open dialogs.
    if let Some(paper) = value("--paper") {
        run(&mut session, "score.setPage", json!({"paper": paper}));
    }
    if args.iter().any(|a| a == "--landscape") {
        run(&mut session, "score.setPage", json!({"landscape": true}));
    }
    let mut app = GuitarioApp::new(session, None);
    if let Some(z) = value("--zoom").and_then(|z| z.parse::<f32>().ok()) {
        app.view.zoom = z;
    }
    if args.iter().any(|a| a == "--tab-only") {
        app.view.notation = false;
    }
    if args.iter().any(|a| a == "--selection") {
        let mut s = demo();
        run(&mut s, "select.set", json!({"fromMeasure": 0, "fromBeat": 3, "toMeasure": 1, "toBeat": 3}));
        app = GuitarioApp::new(s, None);
    }
    if args.iter().any(|a| a == "--playing") {
        // Shows the playback cursor without opening an audio device.
        app.transport.playing = true;
        app.transport.beat = Some((2, 1));
        app.transport.position = 5.1;
        app.transport.metronome = true;
    }
    let print = args.iter().any(|a| a == "--print");
    if print {
        app.open_print();
    }
    if args.iter().any(|a| a == "--preview") {
        app.open_preview();
    }
    if args.iter().any(|a| a == "--page-setup") {
        app.open_page_setup();
    }
    if args.iter().any(|a| a == "--trill") {
        app.open_trill();
    }
    if args.iter().any(|a| a == "--tremolo") {
        app.open_tremolo();
    }
    if args.iter().any(|a| a == "--grace") {
        app.open_grace();
    }
    if args.iter().any(|a| a == "--chord") {
        app.open_chord();
    }
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(w, h)).build_ui(move |ui| app.show(ui));
    harness.run_steps(4);
    if print {
        // Give the printer lookup a moment to answer.
        std::thread::sleep(std::time::Duration::from_millis(500));
        harness.run_steps(3);
    }
    match harness.render() {
        Ok(img) => {
            if let Err(e) = img.save(&out) {
                eprintln!("could not save {out}: {e}");
            } else {
                println!("wrote {out}");
            }
        }
        Err(e) => eprintln!("render failed: {e}"),
    }
}
