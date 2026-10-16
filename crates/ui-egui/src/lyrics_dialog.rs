//! Track › Lyrics…: Guitar Pro's lyrics editor. Five lines per track, each typed as text with the
//! bar its first syllable falls in; OK applies `track.setLyrics`.

use crate::GuitarioApp;
use guitario_score::{LyricLine, MAX_LYRIC_LINES};
use serde_json::{Value, json};

pub(crate) struct LyricsDialog {
    pub(crate) open: bool,
    /// The line being edited, 0-based.
    line: usize,
    /// Every line, always five; `measure` is 0-based.
    lines: Vec<LyricLine>,
    /// Bars in the score, for the start bar's range.
    bars: usize,
    track_name: String,
}

impl Default for LyricsDialog {
    fn default() -> Self {
        LyricsDialog { open: false, line: 0, lines: vec![LyricLine::default(); MAX_LYRIC_LINES], bars: 1, track_name: String::new() }
    }
}

impl LyricsDialog {
    /// What OK sends: the five lines, the command drops empty ones at the end.
    fn params(&self) -> Value {
        let lines: Vec<Value> = self.lines.iter().map(|l| json!({"text": l.text, "measure": l.measure.min(self.bars.saturating_sub(1))})).collect();
        json!({ "lines": lines })
    }
}

impl GuitarioApp {
    /// Track › Lyrics…: the lyrics of the track under the cursor.
    pub fn open_lyrics(&mut self) {
        let c = self.session.cursor();
        let score = self.session.score();
        let Some(track) = score.tracks.get(c.track) else { return };
        let mut lines = track.lyrics.clone();
        lines.resize(MAX_LYRIC_LINES, LyricLine::default());
        self.lyrics = LyricsDialog { open: true, line: 0, lines, bars: score.headers.len().max(1), track_name: track.name.clone() };
    }

    pub(crate) fn lyrics_dialog(&mut self, ctx: &egui::Context) {
        if !self.lyrics.open {
            return;
        }
        let (mut apply, mut cancel) = (false, false);
        let d = &mut self.lyrics;
        egui::Modal::new(egui::Id::new("lyrics-dialog")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.heading(format!("Lyrics — {}", d.track_name));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for i in 0..MAX_LYRIC_LINES {
                    ui.selectable_value(&mut d.line, i, format!("Line {}", i + 1));
                }
            });
            let bars = d.bars;
            if let Some(line) = d.lines.get_mut(d.line) {
                ui.horizontal(|ui| {
                    ui.label("Starts at bar");
                    let mut bar = line.measure + 1;
                    if ui.add(egui::DragValue::new(&mut bar).range(1..=bars)).changed() {
                        line.measure = bar.saturating_sub(1);
                    }
                });
                ui.add(egui::TextEdit::multiline(&mut line.text).desired_rows(8).desired_width(f32::INFINITY).char_limit(guitario_score::MAX_LYRICS_TEXT));
            }
            ui.label(
                egui::RichText::new("One syllable per note: split words with spaces or hyphens (Wa-ter); in+one sings two words on one note; a lone - skips a note; [text] is a comment.")
                    .small()
                    .weak(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    apply = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if apply {
            if self.run("track.setLyrics", self.lyrics.params()).is_some() {
                self.lyrics.open = false;
            }
        } else if cancel {
            self.lyrics.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn the_dialog_shows_the_track_lyrics_and_applies_what_it_shows() {
        let mut s = Session::new();
        s.run("measure.append", &Value::Null).unwrap();
        s.run("track.setLyrics", &json!({"lines": [{"text": "first"}, {"text": "second", "measure": 1}]})).unwrap();
        let mut app = GuitarioApp::new(s, None);
        app.open_lyrics();
        assert!(app.lyrics.open);
        assert_eq!(app.lyrics.lines.len(), MAX_LYRIC_LINES);
        assert_eq!((app.lyrics.lines[1].text.as_str(), app.lyrics.lines[1].measure), ("second", 1));
        app.lyrics.lines[0].text = "Steel ho-ri-zon".into();
        app.lyrics.lines[1].text.clear();
        app.lyrics.lines[2].text = "third".into();
        assert!(app.run("track.setLyrics", app.lyrics.params()).is_some());
        let l = &app.session.score().tracks[0].lyrics;
        assert_eq!(l.len(), 3, "an empty line between keeps its place; empty lines at the end go");
        assert_eq!((l[0].text.as_str(), l[1].text.as_str(), l[2].text.as_str()), ("Steel ho-ri-zon", "", "third"));
        // Nothing changes while the dialog is only drawn.
        app.open_lyrics();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.lyrics_dialog(ui.ctx())).textures_delta.clear();
        }
        assert!(app.lyrics.open);
        assert_eq!(app.session.score().tracks[0].lyrics[0].text, "Steel ho-ri-zon");
    }
}
