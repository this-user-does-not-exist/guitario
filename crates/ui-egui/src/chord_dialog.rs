//! Tools › Chord Diagrams…: name the chord over the beat under the cursor and draw its diagram.
//! Click a fret to put a finger there (again to lift it), click above the nut to switch a string
//! between open and not played. "From the beat" copies the notes under the cursor, "Name it" names
//! the diagram (E5, Am7, D/F#). OK applies `beat.setChord`.

use crate::GuitarioApp;
use egui::{Color32, Rect, Sense, Stroke, pos2, vec2};
use guitario_score::NoteKind;
use serde_json::{Value, json};

/// Frets the editor shows below the first fret.
const ROWS: u8 = 5;

pub(crate) struct ChordDialog {
    pub(crate) open: bool,
    name: String,
    /// Fret per string, highest string first; `None` is a string not played.
    frets: Vec<Option<u8>>,
    first_fret: u8,
    /// The beat already has a chord, so it can be removed.
    has_chord: bool,
    /// The cursor beat's tuning and capo, for naming.
    tuning: Vec<u8>,
    capo: u8,
    /// Spell names with flats (the bar's key has flats).
    flats: bool,
    /// The diagram of the beat's played notes, for "From the beat".
    from_beat: Option<Vec<Option<u8>>>,
}

impl Default for ChordDialog {
    fn default() -> Self {
        ChordDialog {
            open: false,
            name: String::new(),
            frets: vec![None; 6],
            first_fret: 1,
            has_chord: false,
            tuning: Vec::new(),
            capo: 0,
            flats: false,
            from_beat: None,
        }
    }
}

impl ChordDialog {
    /// The name of the chord the diagram holds, if it makes one.
    fn suggested_name(&self) -> Option<String> {
        let pitches: Vec<u8> =
            self.frets.iter().zip(&self.tuning).filter_map(|(f, open)| f.map(|f| open.saturating_add(self.capo).saturating_add(f))).collect();
        guitario_theory::chord_name(&pitches, self.flats)
    }

    /// Takes `frets` and starts the diagram at the nut when it fits in the first frets.
    fn set_frets(&mut self, frets: Vec<Option<u8>>) {
        let fretted = frets.iter().flatten().copied().filter(|f| *f > 0);
        self.first_fret = if fretted.clone().max().unwrap_or(0) <= ROWS { 1 } else { fretted.min().unwrap_or(1) };
        self.frets = frets;
    }

    /// What OK sends.
    fn params(&self) -> Value {
        json!({"name": self.name.trim(), "frets": self.frets, "firstFret": self.first_fret})
    }
}

impl GuitarioApp {
    /// Tools › Chord Diagrams…: on the beat under the cursor of a string track.
    pub fn open_chord(&mut self) {
        let c = self.session.cursor();
        let score = self.session.score();
        let Some(track) = score.tracks.get(c.track).filter(|t| !t.is_drums()) else {
            self.status = "Chord diagrams belong to string tracks; select one first.".into();
            self.status_is_error = true;
            return;
        };
        let beat = track.measures.get(c.measure).and_then(|m| m.voices.get(c.voice)).and_then(|v| v.beats.get(c.beat));
        let strings = track.string_count();
        let mut d = ChordDialog {
            open: true,
            tuning: track.tuning.strings.clone(),
            capo: track.capo,
            flats: score.headers.get(c.measure).is_some_and(|h| h.key < 0),
            frets: vec![None; strings],
            ..ChordDialog::default()
        };
        if let Some(b) = beat.filter(|b| !b.rest) {
            let mut frets = vec![None; strings];
            for n in b.notes.iter().filter(|n| n.kind != NoteKind::Dead) {
                if let Some(slot) = frets.get_mut(usize::from(n.string)) {
                    *slot = Some(n.fret);
                }
            }
            d.from_beat = frets.iter().any(Option::is_some).then_some(frets);
        }
        match beat.and_then(|b| b.chord.clone()) {
            Some(chord) => {
                d.name = chord.name;
                d.has_chord = true;
                if chord.frets.is_empty() {
                    d.frets = vec![None; strings];
                } else {
                    d.frets = chord.frets;
                    d.frets.resize(strings, None);
                    d.first_fret = chord.first_fret.max(1);
                }
            }
            // A new chord starts from the beat's notes, named.
            None => {
                if let Some(frets) = d.from_beat.clone() {
                    d.set_frets(frets);
                    d.name = d.suggested_name().unwrap_or_default();
                }
            }
        }
        self.chord = d;
    }

    pub(crate) fn chord_dialog(&mut self, ctx: &egui::Context) {
        if !self.chord.open {
            return;
        }
        let (mut apply, mut remove, mut cancel) = (false, false, false);
        let d = &mut self.chord;
        egui::Modal::new(egui::Id::new("chord-dialog")).show(ctx, |ui| {
            ui.set_width(300.0);
            ui.heading("Chord Diagram");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(140.0).char_limit(64));
                if ui.add_enabled(d.suggested_name().is_some(), egui::Button::new("Name it")).clicked()
                    && let Some(n) = d.suggested_name()
                {
                    d.name = n;
                }
            });
            ui.horizontal(|ui| {
                ui.label("First fret");
                ui.add(egui::DragValue::new(&mut d.first_fret).range(1..=32));
                if ui.add_enabled(d.from_beat.is_some(), egui::Button::new("From the beat")).clicked()
                    && let Some(frets) = d.from_beat.clone()
                {
                    d.set_frets(frets);
                    d.name = d.suggested_name().unwrap_or_default();
                }
                if ui.button("Clear").clicked() {
                    d.frets.iter_mut().for_each(|f| *f = None);
                }
            });
            ui.add_space(4.0);
            diagram_editor(ui, d);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    apply = true;
                }
                if ui.add_enabled(d.has_chord, egui::Button::new("Remove")).clicked() {
                    remove = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        });
        let request = if apply {
            Some(self.chord.params())
        } else if remove {
            Some(json!({"name": ""}))
        } else {
            None
        };
        if let Some(params) = request {
            if self.run("beat.setChord", params).is_some() {
                self.chord.open = false;
            }
        } else if cancel {
            self.chord.open = false;
        }
    }
}

/// The clickable diagram: strings left (lowest) to right (highest), the first fret at the top.
fn diagram_editor(ui: &mut egui::Ui, d: &mut ChordDialog) {
    let n = d.frets.len();
    if n == 0 {
        return;
    }
    let (gap, row, top_room) = (24.0, 26.0, 22.0);
    let size = vec2(gap * n as f32 + 40.0, top_room + row * f32::from(ROWS) + 8.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter_at(rect);
    let ink = ui.visuals().text_color();
    let left = rect.left() + 32.0 + gap / 2.0;
    let y0 = rect.top() + top_room;
    let x_of = |i: usize| left + (n - 1 - i) as f32 * gap;
    let thin = Stroke::new(1.0, ink);
    for k in 0..=ROWS {
        let y = y0 + f32::from(k) * row;
        p.line_segment([pos2(x_of(n - 1), y), pos2(x_of(0), y)], thin);
    }
    for i in 0..n {
        p.line_segment([pos2(x_of(i), y0), pos2(x_of(i), y0 + f32::from(ROWS) * row)], thin);
    }
    if d.first_fret == 1 {
        p.line_segment([pos2(x_of(n - 1), y0), pos2(x_of(0), y0)], Stroke::new(4.0, ink));
    } else {
        p.text(pos2(x_of(n - 1) - 10.0, y0 + row / 2.0), egui::Align2::RIGHT_CENTER, format!("{}fr", d.first_fret), egui::FontId::proportional(12.0), ink);
    }
    for (i, f) in d.frets.iter().enumerate() {
        let x = x_of(i);
        match f {
            None => {
                let c = pos2(x, y0 - 11.0);
                p.line_segment([c - vec2(4.0, 4.0), c + vec2(4.0, 4.0)], thin);
                p.line_segment([c - vec2(4.0, -4.0), c + vec2(4.0, -4.0)], thin);
            }
            Some(0) => {
                p.circle_stroke(pos2(x, y0 - 11.0), 4.5, thin);
            }
            Some(f) if *f >= d.first_fret && *f - d.first_fret < ROWS => {
                p.circle_filled(pos2(x, y0 + (f32::from(*f - d.first_fret) + 0.5) * row), 7.0, Color32::from_rgb(40, 110, 220));
            }
            Some(_) => {}
        }
    }
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && let Some(i) = (0..n).min_by(|a, b| (x_of(*a) - pos.x).abs().total_cmp(&(x_of(*b) - pos.x).abs()))
        && (x_of(i) - pos.x).abs() <= gap / 2.0
        && let Some(slot) = d.frets.get_mut(i)
    {
        let hit = Rect::from_min_max(pos2(rect.left(), y0), pos2(rect.right(), y0 + f32::from(ROWS) * row));
        if pos.y < y0 {
            // Above the nut: open, or not played.
            *slot = if *slot == Some(0) { None } else { Some(0) };
        } else if hit.contains(pos) {
            let fret = d.first_fret.saturating_add(((pos.y - y0) / row).floor().clamp(0.0, f32::from(ROWS - 1)) as u8);
            *slot = if *slot == Some(fret) { None } else { Some(fret.min(36)) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;
    use guitario_score::Chord;

    fn app_with_chord_shape(frets: &[(u64, u64)]) -> GuitarioApp {
        let mut s = Session::new();
        for (string, fret) in frets {
            s.run("cursor.set", &json!({"string": string})).unwrap();
            s.run("note.setFret", &json!({"fret": fret})).unwrap();
        }
        GuitarioApp::new(s, None)
    }

    #[test]
    fn a_new_chord_starts_from_the_beat_named_and_applies_what_it_shows() {
        // An E5 power chord on the low strings: 0 2 2 from the low E.
        let mut app = app_with_chord_shape(&[(5, 0), (4, 2), (3, 2)]);
        app.open_chord();
        assert!(app.chord.open);
        assert_eq!(app.chord.name, "E5");
        assert_eq!(app.chord.frets, vec![None, None, None, Some(2), Some(2), Some(0)]);
        assert!(app.run("beat.setChord", app.chord.params()).is_some());
        let chord = app.session.score().tracks[0].measures[0].voices[0].beats[0].chord.clone();
        assert_eq!(chord, Some(Chord { name: "E5".into(), frets: vec![None, None, None, Some(2), Some(2), Some(0)], first_fret: 1 }));
        // Opened again it shows that chord, which can be removed.
        app.open_chord();
        assert!(app.chord.has_chord && app.chord.name == "E5");
    }

    #[test]
    fn up_the_neck_the_diagram_starts_at_the_lowest_fret_and_a_drum_track_says_why_not() {
        let mut app = app_with_chord_shape(&[(5, 7), (4, 9), (3, 9)]);
        app.open_chord();
        assert_eq!((app.chord.first_fret, app.chord.name.as_str()), (7, "B5"));
        let mut s = Session::new();
        s.run("track.addDrums", &json!({"name": "Drums"})).unwrap();
        s.run("track.select", &json!({"index": 1})).unwrap();
        let mut drums = GuitarioApp::new(s, None);
        drums.open_chord();
        assert!(!drums.chord.open && drums.status_is_error, "{}", drums.status);
    }

    #[test]
    fn the_dialog_draws_and_keeps_its_state_across_frames() {
        let mut app = app_with_chord_shape(&[(4, 0), (3, 2), (2, 2), (1, 1), (0, 0)]);
        app.open_chord();
        assert_eq!(app.chord.name, "Am");
        let ctx = egui::Context::default();
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.chord_dialog(ui.ctx())).textures_delta.clear();
        }
        assert!(app.chord.open);
        assert!(app.session.score().tracks[0].measures[0].voices[0].beats[0].chord.is_none(), "nothing applied without OK");
    }
}
