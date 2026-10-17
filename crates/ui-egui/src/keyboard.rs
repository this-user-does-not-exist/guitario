//! The keyboard instrument view (View › Instrument View › Keyboard), drawn as Guitar Pro 8's: a
//! piano with the keys the instrument cannot reach greyed, the pitches of the beat under the cursor
//! in yellow with their names, and the display mode's other pitches dimmed. Clicking a key enters
//! that pitch on the best string.

use crate::theme::tokens;
use crate::{GuitarioApp, instrument};
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

/// Pitches sounding in `beat`.
fn pitches(track: &Track, beat: Option<&guitario_score::Beat>) -> Vec<u8> {
    beat.filter(|b| !b.rest).map(|b| b.notes.iter().filter_map(|n| pitch_of(track, n.string, n.fret)).collect()).unwrap_or_default()
}

/// Pitches sounding in the beat at the cursor (or the playing beat).
#[cfg(test)]
fn lit(track: &Track, measure: usize, voice: usize, beat: usize) -> Vec<u8> {
    let c = guitario_engine::Cursor { track: 0, measure, voice, beat, string: 0 };
    pitches(track, instrument::beats(track, c, instrument::Display::Beat).0)
}

/// The lowest and highest pitch the track can play: its open strings (with the capo) to its top
/// fret.
fn range(track: &Track) -> Option<(u8, u8)> {
    let open = track.tuning.strings.iter().filter_map(|p| p.checked_add(track.capo));
    let low = open.clone().min()?;
    let high = open.max()?.saturating_add(track.max_fret());
    Some((low, high))
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
    let inner = instrument::side(app, ui, rect, false);
    let p = ui.painter();
    p.rect_filled(inner, 0.0, t.chrome_dark);
    if track.is_drums() {
        p.text(inner.center(), egui::Align2::CENTER_CENTER, "A drum track has no keyboard: use the drum pad.", egui::FontId::proportional(12.0), t.text_dim);
        return;
    }
    let (beat, others) = instrument::beats(track, c, app.view.instrument);
    let sounding = pitches(track, beat);
    let soft: Vec<u8> = others.into_iter().flat_map(|b| pitches(track, Some(b))).collect();
    let reach = range(track);
    let playable = |m: u8| reach.is_some_and(|(lo, hi)| (lo..=hi).contains(&m));
    let whites: Vec<u8> = (LOW..=HIGH).filter(|m| !is_black(*m)).collect();
    let board = inner;
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
            // Guitar Pro greys the keys the instrument cannot play.
            let fill = match (on, soft.contains(&midi), white, playable(midi)) {
                (true, ..) => instrument::NOTE,
                (_, true, ..) => instrument::NOTE_SOFT,
                (_, _, true, true) => Color32::from_rgb(0xe9, 0xe9, 0xe9),
                (_, _, true, false) => Color32::from_rgb(0x82, 0x84, 0x88),
                (_, _, false, true) => Color32::from_rgb(0x1e, 0x1f, 0x22),
                (_, _, false, false) => Color32::from_rgb(0x46, 0x47, 0x4b),
            };
            p.rect_filled(r, 2.0, fill);
            p.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_rgb(0x57, 0x59, 0x5d)), egui::StrokeKind::Inside);
            if hover == Some(midi) {
                p.rect_stroke(r.shrink(1.0), 2.0, Stroke::new(2.0, instrument::NOTE), egui::StrokeKind::Inside);
            }
            if on {
                let y = if white { r.bottom() - 9.0 } else { r.bottom() - 8.0 };
                p.text(
                    pos2(r.center().x, y),
                    egui::Align2::CENTER_CENTER,
                    guitario_theory::note_name(midi),
                    crate::fonts::sans_bold(10.0),
                    instrument::NOTE_INK,
                );
            }
        }
    }
    if let Some(midi) = hover {
        resp.clone().on_hover_text(instrument::pitch_label(midi));
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
    fn the_range_runs_from_the_lowest_open_string_to_the_top_fret() {
        let mut t = guitar();
        let low = *t.tuning.strings.iter().min().unwrap();
        let high = *t.tuning.strings.iter().max().unwrap();
        assert_eq!(range(&t), Some((low, high + t.max_fret())));
        t.capo = 3;
        assert_eq!(range(&t).map(|r| r.0), Some(low + 3));
        t.tuning.strings.clear();
        assert_eq!(range(&t), None);
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
