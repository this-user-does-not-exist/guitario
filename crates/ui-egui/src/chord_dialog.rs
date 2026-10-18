//! Note › Chord… (A), and Tools › Chord Diagrams…: Guitar Pro 8's Chord window. On the left the
//! chord is built (root, type, extension, altered 5th/9th/11th/13th, bass or inversion, omitted
//! tones); on the right are its name, the main diagram (click a fret to put a finger there, again to
//! lift it; above the nut to switch a string between open and not played) with its fingering and
//! notes, and every diagram that plays the built chord on this track's tuning, easiest first. OK
//! applies `beat.setChord`; a built chord keeps its structure, so it transposes.

use crate::GuitarioApp;
use crate::theme;
use egui::{Color32, Rect, RichText, Sense, Stroke, pos2, vec2};
use guitario_score::NoteKind;
use guitario_theory::{Alteration, ChordSpec, ChordType, Extension, Voicing, pitch_class_name};
use serde_json::{Value, json};

/// Frets the main diagram shows below its first fret.
const ROWS: u8 = 5;
/// Alternatives listed.
const ALTERNATIVES: usize = 48;

pub(crate) struct ChordDialog {
    pub(crate) open: bool,
    pub(crate) name: String,
    /// Fret per string, highest string first; `None` is a string not played.
    pub(crate) frets: Vec<Option<u8>>,
    /// Finger per string (1 index to 4 little).
    fingers: Vec<Option<u8>>,
    pub(crate) first_fret: u8,
    /// The beat already has a chord, so it can be removed.
    pub(crate) has_chord: bool,
    /// The cursor track's tuning (highest string first), capo and sound.
    tuning: Vec<u8>,
    capo: u8,
    program: u8,
    /// The diagram of the beat's played notes, for "From the beat".
    from_beat: Option<Vec<Option<u8>>>,
    /// The chord the left side builds.
    pub(crate) spec: ChordSpec,
    /// The diagram is the built chord's: OK keeps the structure. A hand-edited diagram is not.
    pub(crate) built: bool,
    pub(crate) alternatives: Vec<Voicing>,
    pub(crate) chosen: Option<usize>,
    highlight_root: bool,
    /// The chord should be heard (the play button was pressed).
    audition: bool,
}

impl Default for ChordDialog {
    fn default() -> Self {
        ChordDialog {
            open: false,
            name: String::new(),
            frets: vec![None; 6],
            fingers: Vec::new(),
            first_fret: 1,
            has_chord: false,
            tuning: Vec::new(),
            capo: 0,
            program: 25,
            from_beat: None,
            spec: ChordSpec::default(),
            built: false,
            alternatives: Vec::new(),
            chosen: None,
            highlight_root: true,
            audition: false,
        }
    }
}

impl ChordDialog {
    /// The name of the chord the diagram holds, if it makes one.
    fn suggested_name(&self) -> Option<String> {
        guitario_theory::chord_name(&self.pitches(), self.spec.flats)
    }

    /// Sounding pitches of the diagram, lowest string first.
    fn pitches(&self) -> Vec<u8> {
        self.frets.iter().zip(&self.tuning).rev().filter_map(|(f, open)| f.map(|f| open.saturating_add(self.capo).saturating_add(f))).collect()
    }

    /// Takes `frets` and starts the diagram at the nut when it fits in the first frets.
    fn set_frets(&mut self, frets: Vec<Option<u8>>) {
        let fretted = frets.iter().flatten().copied().filter(|f| *f > 0);
        self.first_fret = if fretted.clone().max().unwrap_or(0) <= ROWS { 1 } else { fretted.min().unwrap_or(1) };
        self.fingers = guitario_theory::fingering(&frets).map(|f| f.0).unwrap_or_default();
        self.frets = frets;
    }

    /// The built chord changed: its name, and its diagrams from the easiest.
    pub(crate) fn rebuild(&mut self) {
        self.alternatives = guitario_theory::voicings(&self.spec, &self.tuning, self.capo, ALTERNATIVES);
        self.name = self.spec.name();
        self.built = true;
        self.chosen = None;
        if !self.alternatives.is_empty() {
            self.choose(0);
        } else {
            self.frets = vec![None; self.tuning.len()];
            self.fingers.clear();
        }
    }

    /// Makes alternative `i` the main diagram.
    pub(crate) fn choose(&mut self, i: usize) {
        let Some(v) = self.alternatives.get(i).cloned() else { return };
        self.frets = v.frets.clone();
        self.fingers = v.fingers.clone();
        self.first_fret = v.first_fret();
        self.chosen = Some(i);
    }

    /// The diagram was changed by hand: it is named from its notes and no longer built.
    fn edited(&mut self) {
        self.built = false;
        self.chosen = None;
        self.fingers = guitario_theory::fingering(&self.frets).map(|f| f.0).unwrap_or_default();
        if let Some(n) = self.suggested_name() {
            self.name = n;
        }
    }

    /// What OK sends.
    pub(crate) fn params(&self) -> Value {
        let mut p = json!({"name": self.name.trim(), "frets": self.frets, "firstFret": self.first_fret});
        if self.fingers.iter().any(Option::is_some)
            && let Some(o) = p.as_object_mut()
        {
            o.insert("fingers".into(), json!(self.fingers));
        }
        if self.built
            && let Some(o) = p.as_object_mut()
        {
            o.insert("built".into(), guitario_engine::spec_json(&self.spec));
        }
        p
    }
}

impl GuitarioApp {
    /// Note › Chord…: on the beat under the cursor of a string track.
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
        let flats = score.headers.get(c.measure).is_some_and(|h| h.key < 0);
        let mut d = ChordDialog {
            open: true,
            tuning: track.tuning.strings.clone(),
            capo: track.capo,
            program: track.program,
            frets: vec![None; strings],
            spec: ChordSpec { flats, ..ChordSpec::default() },
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
                d.has_chord = true;
                if let Some(spec) = chord.built {
                    d.spec = spec;
                    d.alternatives = guitario_theory::voicings(&spec, &d.tuning, d.capo, ALTERNATIVES);
                    d.built = true;
                }
                d.name = chord.name;
                if !chord.frets.is_empty() {
                    d.frets = chord.frets;
                    d.frets.resize(strings, None);
                    d.first_fret = chord.first_fret.max(1);
                    d.fingers = chord.fingers;
                    d.chosen = d.alternatives.iter().position(|v| v.frets == d.frets);
                }
            }
            // A new chord starts from the beat's notes, named; or as the builder's chord.
            None => match d.from_beat.clone() {
                Some(frets) => {
                    d.set_frets(frets);
                    d.name = d.suggested_name().unwrap_or_default();
                    d.alternatives = guitario_theory::voicings(&d.spec, &d.tuning, d.capo, ALTERNATIVES);
                }
                None => d.rebuild(),
            },
        }
        self.chord = d;
    }

    /// Sets the open Chord window's builder to `spec`, as its controls would.
    pub fn build_chord(&mut self, spec: ChordSpec) {
        self.chord.spec = spec;
        self.chord.rebuild();
    }

    /// Plays a small score through the sound card without moving the transport: a chord or a
    /// scale from the tool windows.
    pub(crate) fn audition(&mut self, score: &guitario_score::Score) {
        if score.validate().is_err() {
            return;
        }
        if self.audio.is_none() {
            match guitario_audio::AudioOut::open_with(&self.audio_settings) {
                Ok(a) => self.audio = Some(a),
                Err(e) => {
                    self.status = format!("Cannot play: {e}");
                    self.status_is_error = true;
                    return;
                }
            }
        }
        let Some(audio) = &self.audio else { return };
        let mut player = guitario_playback::Player::new(score, audio.sample_rate, self.options());
        player.set_live(true);
        if let Some(bank) = self.sounds.as_ref() {
            bank.apply(&mut player, score);
        }
        player.play_from(0.0);
        *audio.slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(player);
    }

    /// Plays the main diagram as a strum, without moving the transport.
    fn audition_chord(&mut self) {
        let d = &self.chord;
        let mut score = guitario_score::Score::new();
        if let Some(t) = score.tracks.first_mut() {
            if let Ok(tuning) = guitario_theory::Tuning::new("Chord", d.tuning.clone()) {
                t.tuning = tuning;
            }
            t.capo = d.capo;
            t.program = d.program;
            let mut beat = guitario_score::Beat::rest(guitario_theory::Duration::WHOLE);
            beat.notes = d.frets.iter().enumerate().filter_map(|(s, f)| f.map(|f| guitario_score::Note::new(s as u8, f))).collect();
            beat.rest = beat.notes.is_empty();
            beat.brush = Some(guitario_score::Brush { kind: guitario_score::BrushKind::Brush, down: true, ticks: guitario_theory::TICKS_PER_QUARTER / 4 });
            if let Some(v) = t.measures.first_mut().and_then(|m| m.voices.first_mut()) {
                v.beats = vec![beat];
            }
        }
        self.audition(&score);
    }

    pub(crate) fn chord_dialog(&mut self, ctx: &egui::Context) {
        if !self.chord.open {
            return;
        }
        let (mut apply, mut remove, mut cancel) = (false, false, false);
        let d = &mut self.chord;
        egui::Modal::new(egui::Id::new("chord-dialog")).show(ctx, |ui| {
            ui.set_width(880.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(330.0);
                    builder(ui, d);
                });
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.set_width(520.0);
                    ui.horizontal(|ui| {
                        let edit = ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(420.0).char_limit(64).font(egui::TextStyle::Heading));
                        if edit.changed() {
                            // A typed name is only a name.
                            d.built = false;
                        }
                        if ui
                            .add_enabled(d.from_beat.is_some(), egui::Button::new("From the beat"))
                            .on_hover_text("The notes of the beat under the cursor")
                            .clicked()
                            && let Some(frets) = d.from_beat.clone()
                        {
                            d.set_frets(frets);
                            d.edited();
                        }
                        if ui.button(RichText::new("\u{25B6}").size(16.0)).on_hover_text("Play the chord").clicked() {
                            d.audition = true;
                        }
                    });
                    ui.add_space(6.0);
                    egui::Frame::new().fill(theme::tokens().page).inner_margin(8.0).show(ui, |ui| {
                        ui.set_width(504.0);
                        main_diagram(ui, d);
                    });
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut d.highlight_root, "Highlight root note");
                            ui.label("First fret");
                            ui.add(egui::DragValue::new(&mut d.first_fret).range(1..=32));
                        });
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new("ALTERNATIVES").strong().size(16.0));
                    alternatives(ui, d);
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Reset").on_hover_text("A plain major chord on the same root").clicked() {
                    d.spec = ChordSpec { root: d.spec.root, flats: d.spec.flats, ..ChordSpec::default() };
                    d.rebuild();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("OK").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                    if ui.add_enabled(d.has_chord, egui::Button::new("Delete")).clicked() {
                        remove = true;
                    }
                });
            });
        });
        if std::mem::take(&mut self.chord.audition) {
            self.audition_chord();
        }
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

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).strong().size(16.0));
    ui.add_space(2.0);
}

/// Note names for the root and bass lists.
fn note_names(flats: bool) -> Vec<&'static str> {
    (0..12).map(|pc| pitch_class_name(pc, flats)).collect()
}

/// The left column: naming, alterations, inversions, omissions.
fn builder(ui: &mut egui::Ui, d: &mut ChordDialog) {
    let before = d.spec;
    ui.label(RichText::new("CHORD").strong().size(20.0));
    heading(ui, "NAMING");
    egui::Grid::new("chord-naming").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Tonality:");
        ui.horizontal(|ui| {
            let names = note_names(d.spec.flats);
            egui::ComboBox::from_id_salt("chord-root").width(60.0).selected_text(names.get(usize::from(d.spec.root)).copied().unwrap_or("C")).show_ui(
                ui,
                |ui| {
                    for (pc, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut d.spec.root, pc as u8, *n);
                    }
                },
            );
            if ui.add(egui::Button::new("b").selected(d.spec.flats).min_size(vec2(28.0, 20.0))).on_hover_text("Spell with flats").clicked() {
                d.spec.flats = true;
            }
            if ui.add(egui::Button::new("#").selected(!d.spec.flats).min_size(vec2(28.0, 20.0))).on_hover_text("Spell with sharps").clicked() {
                d.spec.flats = false;
            }
        });
        ui.end_row();
        ui.label("Type:");
        egui::ComboBox::from_id_salt("chord-type").width(80.0).selected_text(d.spec.kind.label()).show_ui(ui, |ui| {
            for t in ChordType::ALL {
                ui.selectable_value(&mut d.spec.kind, t, t.label());
            }
        });
        ui.end_row();
        ui.label("Extension:");
        let ext_label = |e: Extension| if e == Extension::None { "\u{2014}" } else { e.label() };
        egui::ComboBox::from_id_salt("chord-extension").width(80.0).selected_text(ext_label(d.spec.extension)).show_ui(ui, |ui| {
            for e in Extension::ALL {
                ui.selectable_value(&mut d.spec.extension, e, ext_label(e));
            }
        });
        ui.end_row();
    });
    // A new type or extension changes which tones exist: start their alterations and omissions afresh.
    if d.spec.kind != before.kind || d.spec.extension != before.extension {
        let [five, nine, eleven, thirteen] = d.spec.alterable();
        let keep = |a: Alteration, ok: bool| if ok { a } else { Alteration::None };
        d.spec.fifth = keep(d.spec.fifth, five);
        d.spec.ninth = keep(d.spec.ninth, nine);
        d.spec.eleventh = keep(d.spec.eleventh, eleven);
        d.spec.thirteenth = keep(d.spec.thirteenth, thirteen);
        d.spec.omit = 0;
        d.spec.bass = None;
    }
    heading(ui, "ALTERATIONS");
    let alterable = d.spec.alterable();
    egui::Grid::new("chord-alterations").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        for (i, (label, n)) in [("5th:", "5"), ("9th:", "9"), ("11th:", "11"), ("13th:", "13")].into_iter().enumerate() {
            ui.label(label);
            let ok = alterable.get(i).copied().unwrap_or(false);
            let slot = match i {
                0 => &mut d.spec.fifth,
                1 => &mut d.spec.ninth,
                2 => &mut d.spec.eleventh,
                _ => &mut d.spec.thirteenth,
            };
            ui.horizontal(|ui| {
                for (alt, mark) in [(Alteration::Flat, "-"), (Alteration::Sharp, "+")] {
                    let on = *slot == alt;
                    if ui.add_enabled(ok, egui::Button::new(format!("/{n}{mark}")).selected(on).min_size(vec2(52.0, 22.0))).clicked() {
                        *slot = if on { Alteration::None } else { alt };
                    }
                }
            });
            ui.end_row();
        }
    });
    heading(ui, "INVERSIONS");
    egui::Grid::new("chord-inversions").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Bass:");
        let names = note_names(d.spec.flats);
        let shown = d.spec.bass.and_then(|b| names.get(usize::from(b)).copied()).unwrap_or("\u{2014}");
        egui::ComboBox::from_id_salt("chord-bass").width(60.0).selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut d.spec.bass, None, "\u{2014}");
            for (pc, n) in names.iter().enumerate() {
                ui.selectable_value(&mut d.spec.bass, Some(pc as u8), *n);
            }
        });
        ui.end_row();
        ui.label("Inversion:");
        let mut inv = d.spec.inversion();
        let count = d.spec.tones().len().min(4);
        let label = |i: usize| if i == 0 { "\u{2014}".to_string() } else { format!("#{i}") };
        egui::ComboBox::from_id_salt("chord-inversion").width(60.0).selected_text(label(inv)).show_ui(ui, |ui| {
            for i in 0..count {
                ui.selectable_value(&mut inv, i, label(i));
            }
        });
        if inv != d.spec.inversion() {
            d.spec.invert(inv);
        }
        ui.end_row();
    });
    if d.spec.bass == Some(d.spec.root) {
        d.spec.bass = None;
    }
    heading(ui, "OMISSIONS");
    let tones = d.spec.tones();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, t) in tones.iter().enumerate() {
            ui.vertical(|ui| {
                ui.label(RichText::new(pitch_class_name(t.pitch_class(d.spec.root), d.spec.flats)).small());
                let kept = !t.omitted;
                // Something must still sound.
                let last = kept && tones.iter().filter(|x| !x.omitted).count() <= 1;
                if ui.add_enabled(!last, egui::Button::new(t.label).selected(kept).min_size(vec2(44.0, 30.0))).clicked() && i < 16 {
                    d.spec.omit ^= 1 << i;
                }
            });
        }
    });
    if d.spec != before {
        d.rebuild();
    }
}

/// Fret positions in a diagram: strings left (lowest) to right (highest), frets down from the top.
struct Grid {
    left: f32,
    top: f32,
    gap: f32,
    row: f32,
    strings: usize,
}

impl Grid {
    fn x(&self, string: usize) -> f32 {
        self.left + (self.strings.saturating_sub(1).saturating_sub(string)) as f32 * self.gap
    }

    fn y(&self, fret_row: f32) -> f32 {
        self.top + fret_row * self.row
    }
}

/// Which dot is the root, and its colour, for "Highlight root note".
type RootDot<'a> = Option<(Color32, &'a dyn Fn(usize, u8) -> bool)>;

/// Paints a diagram: grid, nut or first-fret number, open and muted marks, dots with fingers.
#[allow(clippy::too_many_arguments)]
fn paint_diagram(
    p: &egui::Painter,
    g: &Grid,
    frets: &[Option<u8>],
    fingers: &[Option<u8>],
    first_fret: u8,
    ink: Color32,
    dot: Color32,
    root_dot: RootDot,
    scale: f32,
) {
    let n = g.strings;
    if n == 0 {
        return;
    }
    let thin = Stroke::new(1.0, ink);
    for k in 0..=ROWS {
        let y = g.y(f32::from(k));
        p.line_segment([pos2(g.x(n - 1), y), pos2(g.x(0), y)], thin);
    }
    for i in 0..n {
        p.line_segment([pos2(g.x(i), g.y(0.0)), pos2(g.x(i), g.y(f32::from(ROWS)))], thin);
    }
    if first_fret <= 1 {
        p.line_segment([pos2(g.x(n - 1), g.y(0.0)), pos2(g.x(0), g.y(0.0))], Stroke::new(3.0 * scale, ink));
    } else {
        p.text(
            pos2(g.x(n - 1) - g.row * 0.4 - 4.0 * scale, g.y(0.5)),
            egui::Align2::RIGHT_CENTER,
            first_fret.to_string(),
            egui::FontId::proportional(13.0 * scale),
            ink,
        );
    }
    let mark = 3.5 * scale;
    // A barre: one finger across the lowest fret on two or more strings.
    let barre = guitario_theory::fingering(frets).and_then(|f| f.1);
    if let Some(b) = barre.filter(|b| *b >= first_fret && *b - first_fret < ROWS) {
        let on: Vec<usize> = frets.iter().enumerate().filter(|(_, f)| **f == Some(b)).map(|(i, _)| i).collect();
        if let (Some(lo), Some(hi)) = (on.iter().min(), on.iter().max()) {
            let y = g.y(f32::from(b - first_fret) + 0.5);
            p.line_segment([pos2(g.x(*hi), y), pos2(g.x(*lo), y)], Stroke::new(g.row * 0.55, dot));
        }
    }
    for (i, f) in frets.iter().enumerate() {
        let x = g.x(i);
        match f {
            None => {
                let c = pos2(x, g.y(0.0) - 9.0 * scale);
                p.line_segment([c - vec2(mark, mark), c + vec2(mark, mark)], thin);
                p.line_segment([c - vec2(mark, -mark), c + vec2(mark, -mark)], thin);
            }
            Some(0) => {
                p.circle_stroke(pos2(x, g.y(0.0) - 9.0 * scale), mark, thin);
            }
            Some(f) if *f >= first_fret && *f - first_fret < ROWS => {
                let c = pos2(x, g.y(f32::from(*f - first_fret) + 0.5));
                let colour = root_dot.as_ref().filter(|(_, is_root)| is_root(i, *f)).map_or(dot, |(c, _)| *c);
                p.circle_filled(c, g.row * 0.33, colour);
                if let Some(finger) = fingers.get(i).copied().flatten() {
                    let label = if finger == 0 { "T".to_string() } else { finger.to_string() };
                    p.text(c, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(g.row * 0.45), Color32::WHITE);
                }
            }
            Some(_) => {}
        }
    }
}

/// The main diagram, large and clickable, with the chord's name and its notes below.
fn main_diagram(ui: &mut egui::Ui, d: &mut ChordDialog) {
    let n = d.frets.len();
    if n == 0 {
        return;
    }
    let ink = theme::GP.ink;
    let (gap, row) = (44.0, 46.0);
    let size = vec2(504.0, 120.0 + row * f32::from(ROWS));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter_at(rect);
    let width = gap * (n.saturating_sub(1)) as f32;
    let g = Grid { left: rect.center().x - width / 2.0, top: rect.top() + 80.0, gap, row, strings: n };
    p.text(pos2(rect.center().x, rect.top() + 26.0), egui::Align2::CENTER_CENTER, d.name.trim(), egui::FontId::proportional(36.0), ink);
    let root = d.spec.root;
    let (tuning, capo, built) = (d.tuning.clone(), d.capo, d.built);
    let is_root = move |s: usize, f: u8| built && tuning.get(s).is_some_and(|o| (u16::from(*o) + u16::from(capo) + u16::from(f)) % 12 == u16::from(root));
    let highlight: RootDot = d.highlight_root.then_some((theme::GP.accent, &is_root as &dyn Fn(usize, u8) -> bool));
    paint_diagram(&p, &g, &d.frets, &d.fingers, d.first_fret, ink, ink, highlight, 1.6);
    // The notes, lowest string first, under their strings.
    for (i, (f, open)) in d.frets.iter().zip(&d.tuning).enumerate() {
        if let Some(f) = f {
            let pc = (u16::from(*open) + u16::from(d.capo) + u16::from(*f)) % 12;
            p.text(
                pos2(g.x(i), g.y(f32::from(ROWS)) + 18.0),
                egui::Align2::CENTER_CENTER,
                pitch_class_name(pc as u8, d.spec.flats),
                egui::FontId::proportional(16.0),
                ink,
            );
        }
    }
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && let Some(i) = (0..n).min_by(|a, b| (g.x(*a) - pos.x).abs().total_cmp(&(g.x(*b) - pos.x).abs()))
        && (g.x(i) - pos.x).abs() <= gap / 2.0
    {
        let hit = Rect::from_min_max(pos2(rect.left(), g.y(0.0)), pos2(rect.right(), g.y(f32::from(ROWS))));
        let first = d.first_fret;
        if let Some(slot) = d.frets.get_mut(i) {
            if pos.y < g.y(0.0) && pos.y > g.y(0.0) - 24.0 {
                // Above the nut: open, or not played.
                *slot = if *slot == Some(0) { None } else { Some(0) };
            } else if hit.contains(pos) {
                let fret = first.saturating_add(((pos.y - g.y(0.0)) / row).floor().clamp(0.0, f32::from(ROWS - 1)) as u8);
                *slot = if *slot == Some(fret) { None } else { Some(fret.min(36)) };
            }
        }
        d.edited();
    }
}

/// The diagrams that play the built chord, four to a row; a click makes one the main diagram.
fn alternatives(ui: &mut egui::Ui, d: &mut ChordDialog) {
    let mut picked = None;
    egui::Frame::new().fill(theme::tokens().page).inner_margin(4.0).show(ui, |ui| {
        ui.set_width(512.0);
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            if d.alternatives.is_empty() {
                ui.label(RichText::new("No diagram plays this chord on this tuning.").color(theme::GP.ink));
                return;
            }
            let cell = vec2(124.0, 128.0);
            for (r, row) in d.alternatives.chunks(4).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (k, v) in row.iter().enumerate() {
                        let i = r * 4 + k;
                        let (rect, resp) = ui.allocate_exact_size(cell, Sense::click());
                        let p = ui.painter_at(rect);
                        let chosen = d.chosen == Some(i);
                        if chosen {
                            p.rect_filled(rect.shrink(2.0), 4.0, theme::GP.accent);
                        } else if resp.hovered() {
                            p.rect_filled(rect.shrink(2.0), 4.0, Color32::from_gray(225));
                        }
                        let ink = if chosen { Color32::WHITE } else { theme::GP.ink };
                        let strings = v.frets.len();
                        let gap = 12.0;
                        let g = Grid {
                            left: rect.center().x - gap * (strings.saturating_sub(1)) as f32 / 2.0 + 6.0,
                            top: rect.top() + 26.0,
                            gap,
                            row: 16.0,
                            strings,
                        };
                        paint_diagram(&p, &g, &v.frets, &[], v.first_fret(), ink, ink, None, 0.8);
                        // Fingers under the strings, as Guitar Pro lists them.
                        for (s, f) in v.fingers.iter().enumerate() {
                            if let Some(f) = f {
                                p.text(
                                    pos2(g.x(s), g.y(f32::from(ROWS)) + 9.0),
                                    egui::Align2::CENTER_CENTER,
                                    f.to_string(),
                                    egui::FontId::proportional(9.0),
                                    ink,
                                );
                            }
                        }
                        if resp.clicked() {
                            picked = Some(i);
                        }
                    }
                });
            }
        });
    });
    if let Some(i) = picked {
        d.choose(i);
        // A chosen voicing of the built chord is still the built chord.
        if d.built || d.alternatives.get(i).is_some() {
            d.built = true;
            d.name = d.spec.name();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;
    use guitario_score::Chord;
    use guitario_theory::{ChordType, Extension};

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
        let frets = vec![None, None, None, Some(2), Some(2), Some(0)];
        let fingers = vec![None, None, None, Some(1), Some(1), None];
        assert_eq!(chord, Some(Chord { name: "E5".into(), frets, first_fret: 1, fingers, ..Chord::default() }));
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
    fn the_builder_names_lists_and_applies_a_built_chord_that_reopens_as_built() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.open_chord();
        // An empty beat starts from the builder: C major, open C first.
        assert_eq!(app.chord.name, "C");
        assert_eq!(app.chord.frets, vec![Some(0), Some(1), Some(0), Some(2), Some(3), None]);
        app.chord.spec = ChordSpec { root: 3, kind: ChordType::MinorSeventh, extension: Extension::Add11, ..ChordSpec::default() };
        app.chord.rebuild();
        assert_eq!(app.chord.name, "D#m7add11");
        assert!(app.chord.alternatives.len() > 3 && app.chord.built);
        app.chord.choose(2);
        let chosen = app.chord.alternatives[2].frets.clone();
        let params = app.chord.params();
        assert_eq!(params["built"]["type"], "m7");
        assert!(app.run("beat.setChord", params).is_some());
        let chord = app.session.score().tracks[0].measures[0].voices[0].beats[0].chord.clone().unwrap();
        assert_eq!((chord.name.as_str(), &chord.frets, chord.built.map(|b| b.root)), ("D#m7add11", &chosen, Some(3)));
        app.open_chord();
        assert!(app.chord.built && app.chord.has_chord);
        assert_eq!((app.chord.spec.kind, app.chord.chosen), (ChordType::MinorSeventh, Some(2)));
        // A hand edit is only named.
        app.chord.frets = vec![Some(0), Some(0), Some(0), Some(2), Some(2), Some(0)];
        app.chord.edited();
        assert!(!app.chord.built && app.chord.params().get("built").is_none());
        assert_eq!(app.chord.name, "Em");
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
