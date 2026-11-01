//! What the fretboard and keyboard views share, as in Guitar Pro 8: the side column with the
//! cursor arrows and the settings menu, the yellow note marks, and which beats a display mode
//! shows.

use crate::GuitarioApp;
use crate::icons::{Icon, draw};
use crate::theme::tokens;
use egui::{Color32, Rect, Sense, pos2, vec2};
use guitario_engine::Cursor;
use guitario_score::{Beat, Track};
use serde::{Deserialize, Serialize};

/// The side column's width.
pub const SIDE: f32 = 64.0;
const SIDE_FILL: Color32 = Color32::from_rgb(0x35, 0x3b, 0x42);
const BUTTON: Color32 = Color32::from_rgb(0x3f, 0x4d, 0x5b);
const BUTTON_HOVER: Color32 = Color32::from_rgb(0x52, 0x62, 0x72);
/// A note of the beat, as Guitar Pro marks it.
pub const NOTE: Color32 = Color32::from_rgb(0xec, 0xb9, 0x5b);
/// A note the display mode adds (the rest of the bar, or the next beat).
pub const NOTE_SOFT: Color32 = Color32::from_rgb(0x8a, 0x76, 0x4c);
/// The letter on a note mark.
pub const NOTE_INK: Color32 = Color32::from_rgb(0x2a, 0x22, 0x12);

/// What an instrument view shows besides the beat under the cursor.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum Display {
    #[default]
    Beat,
    /// The beat, and the rest of its bar dimmed.
    BeatAndBar,
    /// The beat, and the next one dimmed.
    BeatAndNext,
}

impl Display {
    const ALL: [(Display, &'static str); 3] =
        [(Display::Beat, "Current beat"), (Display::BeatAndBar, "Current beat and bar"), (Display::BeatAndNext, "Current and next beat")];
}

/// How the instrument view shows a drum track: Guitar Pro's pads, or the step grid of the bar.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct DrumView {
    pub grid: bool,
    /// The grid's step: 8, 16, 32 or 64.
    pub resolution: u8,
    pub triplet: bool,
}

impl Default for DrumView {
    fn default() -> Self {
        DrumView { grid: false, resolution: 16, triplet: false }
    }
}

impl DrumView {
    pub fn sanitized(self) -> DrumView {
        let resolution = if [8, 16, 32, 64].contains(&self.resolution) { self.resolution } else { 16 };
        DrumView { resolution, ..self }
    }
}

/// The beat at `c` and the beats `mode` adds, rests left out of the second.
pub fn beats(track: &Track, c: Cursor, mode: Display) -> (Option<&Beat>, Vec<&Beat>) {
    let voice = |m: usize| track.measures.get(m).and_then(|m| m.voices.get(c.voice));
    let beat = voice(c.measure).and_then(|v| v.beats.get(c.beat));
    let others: Vec<&Beat> = match mode {
        Display::Beat => Vec::new(),
        Display::BeatAndBar => voice(c.measure).map(|v| v.beats.iter().enumerate().filter(|(i, _)| *i != c.beat).map(|(_, b)| b).collect()).unwrap_or_default(),
        Display::BeatAndNext => {
            let next = voice(c.measure).and_then(|v| v.beats.get(c.beat.saturating_add(1)));
            let next = next.or_else(|| voice(c.measure.saturating_add(1)).and_then(|v| v.beats.first()));
            next.into_iter().collect()
        }
    };
    (beat, others.into_iter().filter(|b| !b.rest).collect())
}

/// A pitch's name and octave, as the views label it on hover: C4 is middle C.
pub fn pitch_label(midi: u8) -> String {
    format!("{}{}", guitario_theory::note_name(midi), i32::from(midi / 12) - 1)
}

/// The side column at the left of `rect`: the arrows move the cursor, the gear opens the view's
/// settings. `fretboard` adds the left-handed switch, a drum track the pads or step grid choice. Returns the rest of `rect`.
pub fn side(app: &mut GuitarioApp, ui: &mut egui::Ui, rect: Rect, fretboard: bool) -> Rect {
    let p = ui.painter().clone();
    let col = Rect::from_min_size(rect.min, vec2(SIDE, rect.height()));
    p.rect_filled(col, 0.0, SIDE_FILL);
    let text = tokens().text;
    let size = vec2(24.0, 24.0);
    let left = Rect::from_min_size(pos2(col.center().x - size.x, col.top() + 8.0), size);
    let right = Rect::from_min_size(pos2(col.center().x, col.top() + 8.0), size);
    let gear = Rect::from_center_size(pos2(col.center().x, left.bottom() + 18.0), vec2(24.0, 22.0));
    let id = ui.id();
    for (r, icon, cmd, tip, corners) in [
        (left, Icon::ChevronLeft, "cursor.left", "Previous beat", egui::CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 }),
        (right, Icon::ChevronRight, "cursor.right", "Next beat", egui::CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }),
    ] {
        let resp = ui.interact(r, id.with(cmd), Sense::click()).on_hover_text(tip);
        p.rect_filled(r, corners, if resp.hovered() { BUTTON_HOVER } else { BUTTON });
        draw(&p, r.shrink(6.0), icon, text);
        if resp.clicked() {
            app.run(cmd, serde_json::Value::Null);
        }
    }
    p.line_segment([left.right_top(), left.right_bottom()], egui::Stroke::new(1.0, SIDE_FILL));
    let resp = ui.interact(gear, id.with("instrument-settings"), Sense::click()).on_hover_text("Settings");
    p.rect_filled(gear, 4.0, if resp.hovered() { BUTTON_HOVER } else { BUTTON });
    draw(&p, gear.shrink(5.0), Icon::Gear, text);
    egui::Popup::menu(&resp).show(|ui| {
        for (mode, label) in Display::ALL {
            if ui.radio(app.view.instrument == mode, label).clicked() {
                app.view.instrument = mode;
                ui.close();
            }
        }
        if fretboard {
            ui.separator();
            ui.checkbox(&mut app.view.left_handed, "Left-handed");
        }
        let drums = app.session.score().tracks.get(app.session.cursor().track).is_some_and(Track::is_drums);
        if drums {
            ui.separator();
            let d = &mut app.view.drums;
            ui.radio_value(&mut d.grid, false, "Pads");
            ui.radio_value(&mut d.grid, true, "Step grid");
            if d.grid {
                ui.indent("grid-steps", |ui| {
                    for (r, label) in [(8, "Eighths"), (16, "Sixteenths"), (32, "Thirty-seconds")] {
                        ui.radio_value(&mut d.resolution, r, label);
                    }
                    ui.checkbox(&mut d.triplet, "Triplets");
                });
            }
        }
    });
    Rect::from_min_max(pos2(col.right(), rect.top()), rect.max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;
    use serde_json::json;

    fn riff() -> Session {
        let mut s = Session::new();
        // Three quarter notes in the first bar.
        for fret in [3, 5, 7] {
            s.run("beat.setDuration", &json!({"value": 4, "dots": 0})).unwrap();
            s.run("note.setFret", &json!({"fret": fret})).unwrap();
            s.run("cursor.right", &json!(null)).unwrap();
        }
        s.run("cursor.set", &json!({"measure": 0, "beat": 0})).unwrap();
        s
    }

    fn frets(bs: &[&Beat]) -> Vec<u8> {
        bs.iter().flat_map(|b| b.notes.iter().map(|n| n.fret)).collect()
    }

    #[test]
    fn display_modes_add_the_bar_or_the_next_beat() {
        let s = riff();
        let (score, c) = (s.score().clone(), s.cursor());
        let track = &score.tracks[0];
        let (beat, others) = beats(track, c, Display::Beat);
        assert_eq!(beat.map(|b| b.notes[0].fret), Some(3));
        assert!(others.is_empty());
        assert_eq!(frets(&beats(track, c, Display::BeatAndNext).1), [5]);
        assert_eq!(frets(&beats(track, c, Display::BeatAndBar).1), [5, 7]);
    }

    #[test]
    fn the_next_beat_crosses_into_the_next_bar_and_stops_at_the_end() {
        let mut s = riff();
        s.run("measure.append", &json!(null)).unwrap();
        let (score, mut c) = (s.score().clone(), s.cursor());
        let track = &score.tracks[0];
        let last = track.measures[0].voices[0].beats.len() - 1;
        c.beat = last;
        // The next bar's first beat is a rest, which the view leaves out.
        assert!(beats(track, c, Display::BeatAndNext).1.iter().all(|b| !b.rest));
        // Past the last bar, or a cursor out of range, shows nothing rather than failing.
        c.measure = 999;
        let (beat, others) = beats(track, c, Display::BeatAndBar);
        assert!(beat.is_none() && others.is_empty());
    }

    #[test]
    fn pitch_labels_name_the_octave() {
        assert_eq!(pitch_label(60), "C4");
        assert_eq!(pitch_label(40), "E2");
        assert_eq!(pitch_label(0), "C-1");
    }
}
