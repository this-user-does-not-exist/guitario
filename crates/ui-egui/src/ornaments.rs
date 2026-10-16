//! Note › Trill… and Note › Tremolo Picking…: small dialogs that apply `note.setTrill` and
//! `beat.setTremolo` to the note or beat under the cursor (or the selection, for tremolo picking).

use crate::GuitarioApp;
use guitario_score::{TremoloSpeed, TrillSpeed};
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Trill,
    Tremolo,
}

pub(crate) struct OrnamentDialog {
    open: Option<Kind>,
    /// The trill's other fret.
    fret: u8,
    trill_speed: TrillSpeed,
    /// `None` clears tremolo picking.
    tremolo: Option<TremoloSpeed>,
    /// The cursor note already has a trill, so it can be removed.
    has_trill: bool,
}

impl Default for OrnamentDialog {
    fn default() -> Self {
        OrnamentDialog { open: None, fret: 2, trill_speed: TrillSpeed::Sixteenth, tremolo: Some(TremoloSpeed::Sixteenth), has_trill: false }
    }
}

impl OrnamentDialog {
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    fn trill_label(speed: TrillSpeed) -> &'static str {
        match speed {
            TrillSpeed::Sixteenth => "Sixteenth notes",
            TrillSpeed::ThirtySecond => "Thirty-second notes",
            TrillSpeed::SixtyFourth => "Sixty-fourth notes",
        }
    }

    fn tremolo_label(speed: Option<TremoloSpeed>) -> &'static str {
        match speed {
            None => "None",
            Some(TremoloSpeed::Eighth) => "Eighth notes (1 slash)",
            Some(TremoloSpeed::Sixteenth) => "Sixteenth notes (2 slashes)",
            Some(TremoloSpeed::ThirtySecond) => "Thirty-second notes (3 slashes)",
        }
    }
}

impl GuitarioApp {
    /// Note › Trill…: needs the cursor on a played note.
    pub fn open_trill(&mut self) {
        let c = self.session.cursor();
        let note = self
            .session
            .score()
            .tracks
            .get(c.track)
            .and_then(|t| t.measures.get(c.measure))
            .and_then(|m| m.voices.get(c.voice))
            .and_then(|v| v.beats.get(c.beat))
            .and_then(|b| b.notes.iter().find(|n| usize::from(n.string) == c.string))
            .copied();
        let max = self.session.score().tracks.get(c.track).map_or(36, guitario_score::Track::max_fret);
        let Some(note) = note else {
            self.status = "Put the cursor on a note to add a trill.".into();
            self.status_is_error = true;
            return;
        };
        let d = &mut self.ornaments;
        *d = OrnamentDialog { open: Some(Kind::Trill), ..OrnamentDialog::default() };
        match note.effects.trill {
            Some(t) => {
                d.fret = t.fret;
                d.trill_speed = t.speed;
                d.has_trill = true;
            }
            // Two frets up is the usual trill, or one down at the top of the neck.
            None => d.fret = if note.fret + 2 <= max { note.fret + 2 } else { note.fret.saturating_sub(2) },
        }
    }

    /// Note › Tremolo Picking…: on the beat under the cursor, or every selected beat.
    pub fn open_tremolo(&mut self) {
        let c = self.session.cursor();
        let beat = self
            .session
            .score()
            .tracks
            .get(c.track)
            .and_then(|t| t.measures.get(c.measure))
            .and_then(|m| m.voices.get(c.voice))
            .and_then(|v| v.beats.get(c.beat))
            .cloned();
        let Some(beat) = beat.filter(|b| !b.rest) else {
            self.status = "Put the cursor on a note to pick it with a tremolo.".into();
            self.status_is_error = true;
            return;
        };
        self.ornaments = OrnamentDialog { open: Some(Kind::Tremolo), tremolo: beat.tremolo.or(Some(TremoloSpeed::Sixteenth)), ..OrnamentDialog::default() };
    }

    pub(crate) fn ornament_dialog(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.ornaments.open else { return };
        let max = self.session.score().tracks.get(self.session.cursor().track).map_or(36, guitario_score::Track::max_fret);
        let (mut apply, mut remove, mut cancel) = (false, false, false);
        let d = &mut self.ornaments;
        egui::Modal::new(egui::Id::new("ornament-dialog")).show(ctx, |ui| {
            ui.set_width(330.0);
            ui.heading(if kind == Kind::Trill { "Trill" } else { "Tremolo Picking" });
            ui.add_space(6.0);
            match kind {
                Kind::Trill => {
                    egui::Grid::new("trill-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("Alternate with fret");
                        ui.add(egui::DragValue::new(&mut d.fret).range(0..=max));
                        ui.end_row();
                        ui.label("Speed");
                        egui::ComboBox::from_id_salt("trill-speed").selected_text(OrnamentDialog::trill_label(d.trill_speed)).show_ui(ui, |ui| {
                            for speed in TrillSpeed::ALL {
                                ui.selectable_value(&mut d.trill_speed, speed, OrnamentDialog::trill_label(speed));
                            }
                        });
                        ui.end_row();
                    });
                }
                Kind::Tremolo => {
                    ui.label("Pick the notes again and again at:");
                    for speed in [None, Some(TremoloSpeed::Eighth), Some(TremoloSpeed::Sixteenth), Some(TremoloSpeed::ThirtySecond)] {
                        ui.radio_value(&mut d.tremolo, speed, OrnamentDialog::tremolo_label(speed));
                    }
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    apply = true;
                }
                if kind == Kind::Trill && ui.add_enabled(d.has_trill, egui::Button::new("Remove")).clicked() {
                    remove = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        });
        let d = &self.ornaments;
        let request: Option<(&str, Value)> = match (kind, apply, remove) {
            (Kind::Trill, true, _) => Some(("note.setTrill", json!({"fret": d.fret, "speed": d.trill_speed.name()}))),
            (Kind::Trill, _, true) => Some(("note.setTrill", json!({"fret": null}))),
            (Kind::Tremolo, true, _) => Some(("beat.setTremolo", json!({"speed": d.tremolo.map_or("none", TremoloSpeed::name)}))),
            _ => None,
        };
        if let Some((id, params)) = request {
            // A refusal (a trill to the note's own fret) is shown in the status bar and the dialog stays open.
            if self.run(id, params).is_some() {
                self.ornaments.open = None;
            }
        } else if cancel {
            self.ornaments.open = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    fn app_with_note(fret: u64) -> GuitarioApp {
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": fret})).unwrap();
        GuitarioApp::new(s, None)
    }

    fn first_beat(app: &GuitarioApp) -> guitario_score::Beat {
        app.session.score().tracks[0].measures[0].voices[0].beats[0].clone()
    }

    #[test]
    fn the_trill_dialog_starts_two_frets_up_and_applies_what_it_shows() {
        let mut app = app_with_note(5);
        app.open_trill();
        assert!(app.ornaments.is_open() && app.ornaments.fret == 7 && !app.ornaments.has_trill);
        app.ornaments.fret = 9;
        app.ornaments.trill_speed = TrillSpeed::ThirtySecond;
        let params = json!({"fret": app.ornaments.fret, "speed": app.ornaments.trill_speed.name()});
        assert!(app.run("note.setTrill", params).is_some());
        let trill = first_beat(&app).notes[0].effects.trill.unwrap();
        assert_eq!((trill.fret, trill.speed), (9, TrillSpeed::ThirtySecond));
        // Opened again, it shows the trill the note has.
        app.open_trill();
        assert!(app.ornaments.has_trill && app.ornaments.fret == 9 && app.ornaments.trill_speed == TrillSpeed::ThirtySecond);
    }

    #[test]
    fn near_the_top_of_the_neck_the_default_trill_goes_down() {
        let mut app = app_with_note(36);
        app.open_trill();
        assert_eq!(app.ornaments.fret, 34);
    }

    #[test]
    fn without_a_note_nothing_opens_and_the_status_says_why() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.open_trill();
        assert!(!app.ornaments.is_open() && app.status_is_error && app.status.contains("trill"), "{}", app.status);
        app.status.clear();
        app.open_tremolo();
        assert!(!app.ornaments.is_open() && app.status.contains("tremolo"), "{}", app.status);
    }

    #[test]
    fn the_tremolo_dialog_applies_a_speed_and_none_clears_it() {
        let mut app = app_with_note(3);
        app.open_tremolo();
        assert_eq!(app.ornaments.tremolo, Some(TremoloSpeed::Sixteenth));
        for (want, name) in [(Some(TremoloSpeed::ThirtySecond), "thirty_second"), (None, "none")] {
            assert!(app.run("beat.setTremolo", json!({"speed": name})).is_some());
            assert_eq!(first_beat(&app).tremolo, want);
        }
        app.run("beat.setTremolo", json!({"speed": "eighth"}));
        app.open_tremolo();
        assert_eq!(app.ornaments.tremolo, Some(TremoloSpeed::Eighth), "it shows what the beat has");
    }

    #[test]
    fn no_dialog_state_survives_frames_without_a_click() {
        let mut app = app_with_note(5);
        app.open_trill();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.ornament_dialog(ui.ctx())).textures_delta.clear();
        }
        assert!(app.ornaments.is_open());
        assert!(first_beat(&app).notes[0].effects.trill.is_none());
    }
}
