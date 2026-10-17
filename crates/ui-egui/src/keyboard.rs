//! The keyboard instrument view (View › Instrument View › Keyboard): a piano showing the pitches
//! of the beat under the cursor; clicking a key enters that pitch on the best string.

use crate::GuitarioApp;
use crate::theme::tokens;
use egui::{Color32, Rect, Stroke, pos2, vec2};
use guitario_score::Track;
use serde_json::json;

/// C2 to C7: the range of a guitar with a few frets to spare.
const LOW: u8 = 36;
const HIGH: u8 = 96;

fn is_black(midi: u8) -> bool {
    matches!(midi % 12, 1 | 3 | 6 | 8 | 10)
}

/// The sounding pitch of a fret on string `string` of `track`.
fn pitch_of(track: &Track, string: u8, fret: u8) -> Option<u8> {
    let open = *track.tuning.strings.get(usize::from(string))?;
    open.checked_add(track.capo)?.checked_add(fret)
}

/// Where `pitch` can be played: the cursor's string when it reaches, else the string that needs
/// the lowest fret. `None` when no string reaches it.
pub(crate) fn place(track: &Track, pitch: u8, prefer: usize) -> Option<(usize, u8)> {
    let reach = |s: usize| -> Option<u8> {
        let open = track.tuning.strings.get(s)?.checked_add(track.capo)?;
        let fret = pitch.checked_sub(open)?;
        (fret <= track.max_fret()).then_some(fret)
    };
    if let Some(f) = reach(prefer) {
        return Some((prefer, f));
    }
    (0..track.tuning.strings.len()).filter_map(|s| reach(s).map(|f| (s, f))).min_by_key(|&(_, f)| f)
}

/// Pitches sounding in the beat at the cursor (or the playing beat).
fn lit(track: &Track, measure: usize, voice: usize, beat: usize) -> Vec<u8> {
    let b = track.measures.get(measure).and_then(|m| m.voices.get(voice)).and_then(|v| v.beats.get(beat));
    b.filter(|b| !b.rest).map(|b| b.notes.iter().filter_map(|n| pitch_of(track, n.string, n.fret)).collect()).unwrap_or_default()
}

impl GuitarioApp {
    /// Shows the keyboard view (the snapshot example uses it).
    pub fn show_keyboard_view(&mut self) {
        self.panels.keyboard = true;
    }
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    let score = app.session.score().clone();
    let mut c = app.session.cursor();
    if let Some((m, b)) = app.transport.beat.filter(|_| app.transport.playing) {
        c.measure = m;
        c.beat = b;
    }
    let Some(track) = score.tracks.get(c.track) else { return };
    let avail = ui.available_size();
    let (rect, resp) = ui.allocate_exact_size(vec2(avail.x, avail.y.clamp(70.0, 150.0)), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, t.chrome_dark);
    if track.is_drums() {
        p.text(rect.center(), egui::Align2::CENTER_CENTER, "A drum track has no keyboard: use the drum pad.", egui::FontId::proportional(12.0), t.text_dim);
        return;
    }
    let sounding = lit(track, c.measure, c.voice, c.beat);
    let whites: Vec<u8> = (LOW..=HIGH).filter(|m| !is_black(*m)).collect();
    let board = Rect::from_min_max(pos2(rect.left() + 12.0, rect.top() + 8.0), pos2(rect.right() - 12.0, rect.bottom() - 8.0));
    let kw = board.width() / whites.len().max(1) as f32;
    // White keys sit side by side; a black key straddles the line after the white key below it.
    let white_x = |midi: u8| -> f32 {
        let idx = whites.iter().position(|w| *w >= midi).unwrap_or(whites.len());
        board.left() + idx as f32 * kw
    };
    let key_rect = |midi: u8| -> Rect {
        if is_black(midi) {
            Rect::from_min_size(pos2(white_x(midi) - kw * 0.3, board.top()), vec2(kw * 0.6, board.height() * 0.62))
        } else {
            Rect::from_min_size(pos2(white_x(midi), board.top()), vec2(kw, board.height()))
        }
    };
    // Which key is under the pointer: black keys are on top.
    let key_at = |pos: egui::Pos2| -> Option<u8> {
        (LOW..=HIGH).filter(|m| is_black(*m)).chain((LOW..=HIGH).filter(|m| !is_black(*m))).find(|m| key_rect(*m).contains(pos))
    };
    let hover = resp.hover_pos().and_then(key_at);
    for white in [true, false] {
        for midi in (LOW..=HIGH).filter(|m| is_black(*m) != white) {
            let r = key_rect(midi);
            let on = sounding.contains(&midi);
            let fill = if on {
                t.accent
            } else if hover == Some(midi) {
                t.accent.gamma_multiply(0.45)
            } else if white {
                Color32::from_rgb(0xf2, 0xf0, 0xe8)
            } else {
                Color32::from_rgb(0x18, 0x18, 0x1a)
            };
            p.rect_filled(r, 2.0, fill);
            p.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_rgb(0x30, 0x30, 0x34)), egui::StrokeKind::Inside);
            if white && midi % 12 == 0 {
                p.text(
                    pos2(r.center().x, r.bottom() - 9.0),
                    egui::Align2::CENTER_CENTER,
                    format!("C{}", i32::from(midi / 12) - 1),
                    egui::FontId::proportional(9.5),
                    if on { Color32::WHITE } else { Color32::from_rgb(0x55, 0x55, 0x5a) },
                );
            }
        }
    }
    if resp.clicked()
        && let Some(midi) = resp.interact_pointer_pos().and_then(key_at)
    {
        match place(track, midi, c.string) {
            Some((s, f)) => {
                app.run("cursor.set", json!({"string": s}));
                app.run("note.setFret", json!({"fret": f}));
            }
            None => app.status = "No string reaches that key.".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::Score;

    fn guitar() -> Track {
        Score::new().tracks.remove(0)
    }

    #[test]
    fn a_key_goes_on_the_cursors_string_when_it_reaches_else_the_lowest_fret() {
        let t = guitar();
        let high_e = *t.tuning.strings.first().unwrap();
        // The pitch of the 5th fret of the top string, asked for on that string.
        assert_eq!(place(&t, high_e + 5, 0), Some((0, 5)));
        // Asked for on the lowest string, it still fits there with a higher fret.
        let low = t.tuning.strings.len() - 1;
        assert_eq!(place(&t, high_e + 5, low).map(|(s, _)| s), Some(low));
        // Below the lowest open string nothing reaches; far above the top fret neither.
        assert_eq!(place(&t, 20, 0), None);
        assert_eq!(place(&t, 127, 0), None);
    }

    #[test]
    fn a_beat_lights_the_pitches_of_its_notes() {
        let mut t = guitar();
        let open = *t.tuning.strings.first().unwrap();
        if let Some(b) = t.measures.first_mut().and_then(|m| m.voices.first_mut()).and_then(|v| v.beats.first_mut()) {
            b.rest = false;
            b.notes = vec![guitario_score::Note::new(0, 3)];
        }
        assert_eq!(lit(&t, 0, 0, 0), vec![open + 3]);
        assert!(lit(&t, 9, 0, 0).is_empty());
        t.capo = 2;
        assert_eq!(lit(&t, 0, 0, 0), vec![open + 5]);
    }
}
