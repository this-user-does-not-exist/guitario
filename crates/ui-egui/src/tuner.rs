//! The tuner, as Guitar Pro 8's toolbar opens it (the tuning fork, or Tools › Tuner): the
//! sound card's input, listened to a few times a second. The upper part shows the note one string
//! plays on a needle; the lower part, a bar per string of the track's tuning (lowest first) for a
//! strum, red while out of tune and green within a few cents. Clicking a string there makes the
//! needle measure against it.

use crate::GuitarioApp;
use crate::theme::tokens;
use egui::{Color32, Rect, RichText, Sense, Stroke, pos2, vec2};
use guitario_playback::tuner;

/// Within this many cents a string counts as in tune.
const IN_TUNE: f32 = 3.0;
/// The needle and bars span this many cents either side.
const RANGE: f32 = 50.0;
/// Seconds between two readings.
const EVERY: f64 = 0.1;
const GREEN: Color32 = Color32::from_rgb(0x2b, 0xc2, 0x8d);
const RED: Color32 = Color32::from_rgb(0xd0, 0x7a, 0x78);

#[derive(Default)]
pub(crate) struct Tuner {
    pub(crate) open: bool,
    input: Option<guitario_audio::AudioIn>,
    pub(crate) error: Option<String>,
    /// The note one string plays (MIDI number) and its cents, smoothed.
    pub(crate) single: Option<(u8, f32)>,
    /// Each string's cents in a strum, lowest first.
    pub(crate) strings: Vec<Option<f32>>,
    /// The string the needle measures against, lowest first.
    pub(crate) focus: Option<usize>,
    last: f64,
    /// Shows a reading given to it instead of listening (offscreen renders and tests).
    offline: bool,
}

impl Tuner {
    /// Reads `samples` against `open` (string pitches, lowest first), smoothing what it shows.
    pub(crate) fn listen(&mut self, samples: &[f32], rate: u32, open: &[u8]) {
        let targets: Vec<f32> = open.iter().map(|k| tuner::frequency_of(f32::from(*k))).collect();
        let heard = tuner::strings(samples, rate, &targets);
        self.strings.resize(open.len(), None);
        for (shown, now) in self.strings.iter_mut().zip(heard) {
            *shown = match (*shown, now) {
                (Some(a), Some(b)) if (a - b).abs() < 20.0 => Some(a * 0.5 + b * 0.5),
                (_, now) => now,
            };
        }
        let reading = tuner::detect(samples, rate).filter(|d| d.clarity > 0.6).map(|d| match self.focus.and_then(|i| targets.get(i).zip(open.get(i))) {
            Some((target, key)) => (*key, tuner::cents(d.frequency, *target)),
            None => tuner::nearest_note(d.frequency),
        });
        self.single = match (self.single, reading) {
            (Some((k, a)), Some((k2, b))) if k == k2 => Some((k, a * 0.6 + b * 0.4)),
            (_, now) => now,
        };
    }
}

fn note_and_octave(key: u8) -> (&'static str, i32) {
    (guitario_theory::note_name(key), i32::from(key / 12) - 1)
}

impl GuitarioApp {
    /// The toolbar's tuning fork, and Tools › Tuner: opens or closes the tuner.
    pub fn toggle_tuner(&mut self) {
        self.tuner.open = !self.tuner.open;
        if !self.tuner.open {
            // Closing stops listening.
            self.tuner.input = None;
        }
    }

    /// Opens the tuner showing what it hears in `samples` instead of listening to the sound card.
    pub fn tuner_reading(&mut self, samples: &[f32], rate: u32) {
        self.tuner.open = true;
        self.tuner.offline = true;
        let open = self.tuner_strings();
        self.tuner.listen(samples, rate, &open);
    }

    /// The cursor track's strings, lowest first, with its capo; empty on a drum track.
    fn tuner_strings(&self) -> Vec<u8> {
        let c = self.session.cursor();
        self.session
            .score()
            .tracks
            .get(c.track)
            .filter(|t| !t.is_drums())
            .map(|t| t.tuning.strings.iter().rev().map(|o| o.saturating_add(t.capo)).collect())
            .unwrap_or_default()
    }

    pub(crate) fn tuner_panel(&mut self, ctx: &egui::Context) {
        if !self.tuner.open {
            return;
        }
        let open = self.tuner_strings();
        let now = ctx.input(|i| i.time);
        if self.tuner.input.is_none() && self.tuner.error.is_none() && !self.tuner.offline {
            match guitario_audio::AudioIn::open() {
                Ok(input) => self.tuner.input = Some(input),
                Err(e) => self.tuner.error = Some(format!("No audio input: {e}")),
            }
        }
        if let Some(input) = &self.tuner.input
            && now - self.tuner.last >= EVERY
        {
            let samples = input.latest((input.sample_rate as f32 * tuner::WINDOW) as usize);
            let rate = input.sample_rate;
            self.tuner.last = now;
            self.tuner.listen(&samples, rate, &open);
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(EVERY));
        let screen = ctx.content_rect();
        let mut close = false;
        egui::Area::new(egui::Id::new("tuner")).order(egui::Order::Foreground).fixed_pos(pos2(screen.right() - 340.0, 80.0)).show(ctx, |ui| {
            egui::Frame::new().fill(tokens().chrome).stroke(Stroke::new(1.0, tokens().border)).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
                ui.set_width(300.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("TUNER").strong().size(20.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("x").on_hover_text("Close the tuner").clicked() {
                            close = true;
                        }
                    });
                });
                if let Some(e) = &self.tuner.error {
                    ui.label(RichText::new(e).color(tokens().text_dim));
                }
                needle(ui, &self.tuner);
                ui.add_space(8.0);
                if let Some(i) = bars(ui, &self.tuner, &open) {
                    self.tuner.focus = if self.tuner.focus == Some(i) { None } else { Some(i) };
                }
                if open.is_empty() {
                    ui.label(RichText::new("Select a string track to tune it.").color(tokens().text_dim));
                }
            });
        });
        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.toggle_tuner();
        }
    }
}

/// The upper part: a needle over ±50 cents, the note with its octave, and the cents.
fn needle(ui: &mut egui::Ui, t: &Tuner) {
    let (rect, _) = ui.allocate_exact_size(vec2(300.0, 200.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, tokens().chrome_dark);
    let hub = pos2(rect.center().x, rect.top() + 150.0);
    let radius = 110.0;
    let angle = |cents: f32| (cents.clamp(-RANGE, RANGE) / RANGE) * 0.9;
    let at = |a: f32, r: f32| pos2(hub.x + r * a.sin(), hub.y - r * a.cos());
    let dim = tokens().text_dim.gamma_multiply(0.5);
    for k in -10..=10 {
        let a = angle(k as f32 * 5.0);
        let inner = if k % 5 == 0 { radius - 18.0 } else { radius - 10.0 };
        p.line_segment([at(a, inner), at(a, radius)], Stroke::new(1.0, dim));
    }
    for (c, label) in [(-RANGE, "-50"), (0.0, "0"), (RANGE, "+50")] {
        p.text(at(angle(c), radius + 12.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), tokens().text_dim);
    }
    match t.single {
        Some((key, cents)) => {
            let colour = if cents.abs() <= IN_TUNE { GREEN } else { RED };
            p.line_segment([at(angle(cents), radius * 0.45), at(angle(cents), radius - 2.0)], Stroke::new(4.0, colour));
            let (name, octave) = note_and_octave(key);
            let galley = p.layout_no_wrap(name.to_string(), egui::FontId::proportional(54.0), colour);
            let w = galley.size().x;
            p.galley(pos2(hub.x - w / 2.0 - 8.0, hub.y - 62.0), galley, colour);
            p.text(pos2(hub.x + w / 2.0 - 2.0, hub.y - 14.0), egui::Align2::LEFT_BOTTOM, octave.to_string(), egui::FontId::proportional(26.0), colour);
            p.text(pos2(hub.x, hub.y + 22.0), egui::Align2::CENTER_CENTER, format!("{cents:+.1}"), egui::FontId::proportional(22.0), tokens().text_dim);
        }
        None => {
            p.text(pos2(hub.x, hub.y - 20.0), egui::Align2::CENTER_CENTER, "Play a string", egui::FontId::proportional(16.0), tokens().text_dim);
        }
    }
}

/// The lower part: a bar per string, up when sharp and down when flat; returns the string clicked.
fn bars(ui: &mut egui::Ui, t: &Tuner, open: &[u8]) -> Option<usize> {
    let (rect, resp) = ui.allocate_exact_size(vec2(300.0, 200.0), Sense::click());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, tokens().chrome_dark);
    let n = open.len().max(1);
    let plot = Rect::from_min_max(pos2(rect.left() + 8.0, rect.top() + 14.0), pos2(rect.right() - 40.0, rect.bottom() - 40.0));
    let grid = tokens().text_dim.gamma_multiply(0.25);
    for k in 0..=8 {
        let y = plot.top() + plot.height() * k as f32 / 8.0;
        p.line_segment([pos2(plot.left(), y), pos2(plot.right(), y)], Stroke::new(0.5, grid));
    }
    let y_of = |c: f32| plot.center().y - (c.clamp(-RANGE, RANGE) / RANGE) * plot.height() / 2.0;
    for (c, label) in [(RANGE, "+50"), (0.0, "0"), (-RANGE, "-50")] {
        p.text(pos2(plot.right() + 6.0, y_of(c)), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(11.0), tokens().text_dim);
    }
    let slot = plot.width() / n as f32;
    let mut clicked = None;
    for (i, key) in open.iter().enumerate() {
        let x0 = plot.left() + slot * i as f32 + 3.0;
        let x1 = x0 + slot - 6.0;
        let cents = t.strings.get(i).copied().flatten();
        let colour = if cents.is_some_and(|c| c.abs() <= IN_TUNE) { GREEN } else { RED };
        if let Some(c) = cents {
            let (y0, y1) = (y_of(0.0), y_of(c));
            let r = Rect::from_min_max(pos2(x0, y0.min(y1) - 1.5), pos2(x1, y0.max(y1) + 1.5));
            p.rect_filled(r, 1.0, colour);
        }
        let (name, octave) = note_and_octave(*key);
        let label = if cents.is_some_and(|c| c.abs() <= IN_TUNE) { GREEN } else { tokens().text };
        let focused = t.focus == Some(i);
        let cx = (x0 + x1) / 2.0;
        p.text(pos2(cx - 3.0, rect.bottom() - 20.0), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(20.0), label);
        p.text(pos2(cx + 8.0, rect.bottom() - 14.0), egui::Align2::LEFT_CENTER, octave.to_string(), egui::FontId::proportional(12.0), label);
        if focused {
            p.rect_stroke(
                Rect::from_min_max(pos2(x0, rect.bottom() - 34.0), pos2(x1, rect.bottom() - 6.0)),
                3.0,
                Stroke::new(1.0, tokens().accent),
                egui::StrokeKind::Inside,
            );
        }
        if resp.clicked() && resp.interact_pointer_pos().is_some_and(|pos| pos.x >= x0 - 3.0 && pos.x <= x1 + 3.0) {
            clicked = Some(i);
        }
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strum(rate: u32, keys: &[(u8, f32)]) -> Vec<f32> {
        let n = (rate as f32 * tuner::WINDOW) as usize;
        (0..n)
            .map(|i| {
                keys.iter()
                    .map(|(k, off)| {
                        let f = tuner::frequency_of(f32::from(*k) + off / 100.0);
                        (2.0 * std::f32::consts::PI * f * i as f32 / rate as f32).sin() * 0.2
                    })
                    .sum()
            })
            .collect()
    }

    #[test]
    fn a_strum_lights_each_string_and_a_focused_string_measures_the_needle() {
        let mut t = Tuner::default();
        let open = [40u8, 45, 50, 55, 59, 64];
        t.listen(&strum(48_000, &[(40, 0.0), (45, -15.0), (55, 6.0)]), 48_000, &open);
        assert!(t.strings[0].is_some_and(|c| c.abs() < IN_TUNE));
        assert!(t.strings[1].is_some_and(|c| (c + 15.0).abs() < 3.0));
        assert!(t.strings[3].is_some_and(|c| (c - 6.0).abs() < 3.0));
        // One string alone: the needle names it.
        let mut t = Tuner::default();
        t.listen(&strum(48_000, &[(45, -10.0)]), 48_000, &open);
        assert_eq!(t.single.map(|s| s.0), Some(45));
        assert!(t.single.is_some_and(|s| (s.1 + 10.0).abs() <= 2.0));
        // Focused on the low E, the same A reads five semitones sharp of it (off the dial).
        t.focus = Some(0);
        t.single = None;
        t.listen(&strum(48_000, &[(45, 0.0)]), 48_000, &open);
        assert!(t.single.is_some_and(|s| s.0 == 40 && s.1 > 450.0));
    }

    #[test]
    fn the_panel_draws_without_a_sound_card() {
        let mut app = GuitarioApp::new(guitario_engine::Session::new(), None);
        app.toggle_tuner();
        app.tuner.error = Some("No audio input: test".into());
        app.tuner.listen(&strum(48_000, &[(52, 4.0)]), 48_000, &[40, 45, 50, 55, 59, 64]);
        let ctx = egui::Context::default();
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.tuner_panel(ui.ctx())).textures_delta.clear();
        }
        assert!(app.tuner.open);
        app.toggle_tuner();
        assert!(!app.tuner.open);
    }
}
