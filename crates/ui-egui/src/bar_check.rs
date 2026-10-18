//! Tools › Check Bar Durations (F4): a window listing every bar whose voice does not fill its time
//! signature, naming the track, bar and voice and how much it holds against what it should.
//! Clicking a row puts the cursor there; Check Again runs the check on the score as it is now.

use crate::GuitarioApp;
use guitario_theory::TICKS_PER_QUARTER;
use serde_json::{Value, json};

/// One bar that does not add up.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BadBar {
    pub track: usize,
    pub measure: usize,
    pub voice: usize,
    pub ticks: u64,
    pub expected: u64,
}

#[derive(Default)]
pub(crate) struct BarCheck {
    pub(crate) open: bool,
    pub(crate) bars: Vec<BadBar>,
    truncated: bool,
}

/// A length in quarter notes as a short text: 3, 3½, 2¾, 0.33.
pub(crate) fn quarters(ticks: u64) -> String {
    let q = u64::from(TICKS_PER_QUARTER);
    let (whole, rest) = (ticks / q, ticks % q);
    let frac = match rest * 4 {
        0 => Some(""),
        x if x == q => Some("¼"),
        x if x == 2 * q => Some("½"),
        x if x == 3 * q => Some("¾"),
        _ => None,
    };
    match frac {
        Some(f) if whole == 0 && !f.is_empty() => f.to_string(),
        Some(f) => format!("{whole}{f}"),
        None => format!("{:.2}", ticks as f64 / q as f64),
    }
}

/// What a row says about a bar: "3 of 4 quarters", "too long: 4½ of 4 quarters".
pub(crate) fn describe(b: &BadBar) -> String {
    let what = format!("{} of {} quarters", quarters(b.ticks), quarters(b.expected));
    if b.ticks > b.expected { format!("too long: {what}") } else { format!("too short: {what}") }
}

fn parse(r: &Value) -> (Vec<BadBar>, bool) {
    let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
    let bars = r
        .get("bars")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|b| BadBar {
                    track: usize::try_from(num(b, "track")).unwrap_or(0),
                    measure: usize::try_from(num(b, "measure")).unwrap_or(0),
                    voice: usize::try_from(num(b, "voice")).unwrap_or(0),
                    ticks: num(b, "ticks"),
                    expected: num(b, "expected"),
                })
                .collect()
        })
        .unwrap_or_default();
    (bars, r.get("truncated").and_then(Value::as_bool).unwrap_or(false))
}

impl GuitarioApp {
    /// Tools › Check Bar Durations: runs the check and shows the window, or says every bar is complete.
    pub(crate) fn check_bar_durations(&mut self) {
        let Some(r) = self.run("tools.checkBarDurations", Value::Null) else { return };
        let (bars, truncated) = parse(&r);
        if bars.is_empty() {
            self.bar_check = BarCheck::default();
            self.status = "Every bar is complete.".into();
            self.status_is_error = false;
            return;
        }
        self.bar_check = BarCheck { open: true, bars, truncated };
    }

    /// Puts the cursor on `b`'s bar and voice.
    pub(crate) fn go_to_bad_bar(&mut self, b: &BadBar) {
        self.run("cursor.set", json!({"track": b.track, "measure": b.measure, "beat": 0, "voice": b.voice}));
    }

    pub(crate) fn bar_check_window(&mut self, ctx: &egui::Context) {
        if !self.bar_check.open {
            return;
        }
        let names: Vec<String> = self.session.score().tracks.iter().map(|t| t.name.clone()).collect();
        let mut open = true;
        let (mut go, mut again) = (None, false);
        let d = &self.bar_check;
        egui::Window::new("Check Bar Durations").open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            ui.label(format!("{} bar(s) do not match their time signature.{}", d.bars.len(), if d.truncated { " Only the first are listed." } else { "" }));
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                egui::Grid::new("bad-bars").num_columns(4).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                    for h in ["Track", "Bar", "Voice", ""] {
                        ui.label(egui::RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (i, b) in d.bars.iter().enumerate() {
                        let track = names.get(b.track).map_or_else(|| format!("{}", b.track + 1), |n| format!("{}. {n}", b.track + 1));
                        if ui.link(track).clicked() {
                            go = Some(i);
                        }
                        ui.label(format!("{}", b.measure + 1));
                        ui.label(format!("{}", b.voice + 1));
                        ui.label(describe(b));
                        ui.end_row();
                    }
                });
            });
            ui.add_space(6.0);
            if ui.button("Check Again").clicked() {
                again = true;
            }
        });
        if let Some(b) = go.and_then(|i| self.bar_check.bars.get(i).cloned()) {
            self.go_to_bad_bar(&b);
        }
        if again {
            self.check_bar_durations();
        } else if !open {
            self.bar_check.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn lengths_read_as_quarters() {
        let q = u64::from(TICKS_PER_QUARTER);
        assert_eq!([quarters(4 * q), quarters(3 * q + q / 2), quarters(q / 4), quarters(q / 3)], ["4", "3½", "¼", "0.33"]);
        let short = BadBar { track: 0, measure: 2, voice: 0, ticks: 3 * q, expected: 4 * q };
        assert_eq!(describe(&short), "too short: 3 of 4 quarters");
        assert_eq!(describe(&BadBar { ticks: 5 * q, ..short }), "too long: 5 of 4 quarters");
    }

    #[test]
    fn a_complete_score_opens_nothing_and_a_short_bar_is_listed_and_reached() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.check_bar_durations();
        assert!(!app.bar_check.open && !app.status_is_error, "{}", app.status);
        // A bar with only a quarter in it: the rest of the bar is missing.
        let mut score = (**app.session.score()).clone();
        while score.headers.len() < 3 {
            let h = score.headers[0].clone();
            score.headers.push(h.clone());
            for t in &mut score.tracks {
                t.measures.push(guitario_score::Measure::empty_for(&h));
            }
        }
        score.tracks[0].measures[2].voices[0].beats = vec![guitario_score::Beat::rest(guitario_theory::Duration::QUARTER)];
        let mut app = GuitarioApp::new(Session::with_score(score), None);
        app.check_bar_durations();
        assert!(app.bar_check.open);
        assert_eq!(app.bar_check.bars.len(), 1);
        let bad = app.bar_check.bars[0].clone();
        assert_eq!((bad.track, bad.measure, bad.voice), (0, 2, 0));
        app.go_to_bad_bar(&bad);
        assert_eq!(app.session.cursor().measure, 2);
        let ctx = egui::Context::default();
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.bar_check_window(ui.ctx())).textures_delta.clear();
        }
        assert!(app.bar_check.open);
    }
}
