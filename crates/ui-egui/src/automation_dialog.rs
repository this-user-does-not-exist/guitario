//! Sound › Automations › Tempo Automation…: the song's tempo points, each a jump or a gradual
//! change from the point before (Guitar Pro's linear automation), with the resulting curve drawn
//! across the whole score.

use crate::GuitarioApp;
use crate::theme::tokens;
use serde_json::{Value, json};

#[derive(Default)]
pub(crate) struct TempoAutomation {
    pub(crate) open: bool,
    /// The bar a new point goes on (1-based, as shown) and its tempo and kind.
    add_bar: usize,
    add_bpm: i64,
    add_linear: bool,
    /// A tempo being dragged: bar (`None` is the song tempo) and value, committed on release.
    editing: Option<(Option<usize>, i64)>,
}

impl GuitarioApp {
    /// Opens the tempo automation editor, ready to add a point at the cursor's bar.
    pub fn open_tempo_automation(&mut self) {
        let m = self.session.cursor().measure;
        let bpm = i64::from(self.session.score().tempo_at(m));
        self.tempo_automation = TempoAutomation { open: true, add_bar: m + 1, add_bpm: bpm, add_linear: true, editing: None };
    }

    pub(crate) fn tempo_automation_dialog(&mut self, ctx: &egui::Context) {
        if !self.tempo_automation.open {
            return;
        }
        let score = std::sync::Arc::clone(self.session.score());
        let bars = score.headers.len().max(1);
        let points: Vec<(usize, u16, bool)> = score.headers.iter().enumerate().filter_map(|(i, h)| Some((i, h.tempo?, h.tempo_linear))).collect();
        let curves = score.tempo_curves();
        let mut actions: Vec<(&'static str, Value)> = Vec::new();
        let mut go_to: Option<usize> = None;
        let mut open = true;
        let d = &mut self.tempo_automation;
        egui::Window::new("Tempo Automation").open(&mut open).resizable(false).collapsible(false).default_width(420.0).default_pos([140.0, 130.0]).show(
            ctx,
            |ui| {
                let as_options: Vec<Option<(f64, f64)>> = curves.iter().copied().map(Some).collect();
                curve(ui, &as_options, f64::from(score.tempo), None);
                ui.add_space(6.0);
                // A tempo field committed when the drag ends (or when typed).
                let mut tempo_field = |ui: &mut egui::Ui, key: Option<usize>, value: u16, commit: &mut dyn FnMut(i64)| {
                    let mut v = match d.editing {
                        Some((k, v)) if k == key => v,
                        _ => i64::from(value),
                    };
                    let r = ui.add(egui::DragValue::new(&mut v).range(20..=400).prefix("♩ = "));
                    if r.dragged() || r.has_focus() {
                        d.editing = Some((key, v));
                    }
                    if r.drag_stopped() || r.lost_focus() || (r.changed() && !r.dragged() && !r.has_focus()) {
                        d.editing = None;
                        if v != i64::from(value) {
                            commit(v);
                        }
                    }
                };
                egui::Grid::new("tempo-points").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Song start");
                    tempo_field(ui, None, score.tempo, &mut |v| actions.push(("score.setTempo", json!({"bpm": v}))));
                    ui.label("");
                    ui.label("");
                    ui.end_row();
                    for &(bar, bpm, linear) in &points {
                        if ui.link(format!("Bar {}", bar + 1)).on_hover_text("Go to this bar").clicked() {
                            go_to = Some(bar);
                        }
                        tempo_field(ui, Some(bar), bpm, &mut |v| actions.push(("measure.setTempo", json!({"bpm": v, "linear": linear, "measure": bar}))));
                        let mut gradual = linear;
                        if ui.checkbox(&mut gradual, "Gradual").on_hover_text("Glide from the tempo before instead of jumping here").changed() {
                            actions.push(("measure.setTempo", json!({"bpm": bpm, "linear": gradual, "measure": bar})));
                        }
                        if ui.small_button("Remove").clicked() {
                            actions.push(("measure.setTempo", json!({"bpm": null, "measure": bar})));
                        }
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("At bar");
                    ui.add(egui::DragValue::new(&mut d.add_bar).range(1..=bars));
                    ui.add(egui::DragValue::new(&mut d.add_bpm).range(20..=400).prefix("♩ = "));
                    ui.checkbox(&mut d.add_linear, "Gradual");
                    if ui.button("Add").clicked() {
                        let bar = d.add_bar.clamp(1, bars) - 1;
                        actions.push(("measure.setTempo", json!({"bpm": d.add_bpm, "linear": d.add_linear && bar > 0, "measure": bar})));
                    }
                });
            },
        );
        self.tempo_automation.open = open;
        for (id, params) in actions {
            self.run(id, params);
        }
        if let Some(bar) = go_to {
            self.run("cursor.set", json!({"measure": bar, "beat": 0}));
        }
    }
}

/// An automation across the score: a line through each bar's start and end value, dim where the
/// automation has no point yet (`fallback` applies there), with the range written at the left.
/// `range` fixes the scale (volume and pan); without it the curve's own extremes are used.
fn curve(ui: &mut egui::Ui, curves: &[Option<(f64, f64)>], fallback: f64, range: Option<(f64, f64)>) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(400.0, 90.0), egui::Sense::hover());
    let t = tokens();
    ui.painter().rect_filled(rect, 3.0, t.lcd_bg);
    let values = || curves.iter().map(|c| c.unwrap_or((fallback, fallback))).flat_map(|c| [c.0, c.1]);
    let (lo, hi) = range.unwrap_or_else(|| (values().fold(f64::INFINITY, f64::min), values().fold(f64::NEG_INFINITY, f64::max)));
    if !lo.is_finite() || !hi.is_finite() {
        return;
    }
    let (lo, hi) = if hi - lo < 1.0 { (lo - 10.0, hi + 10.0) } else { (lo, hi) };
    let inner = rect.shrink2(egui::vec2(34.0, 10.0));
    let x = |bar: f64| inner.left() + (bar / curves.len().max(1) as f64) as f32 * inner.width();
    let y = |v: f64| inner.bottom() - ((v - lo) / (hi - lo)) as f32 * inner.height();
    for (i, c) in curves.iter().enumerate() {
        let ((a, b), color) = match c {
            Some(c) => (*c, t.accent),
            None => ((fallback, fallback), t.lcd_dim),
        };
        ui.painter().line_segment([egui::pos2(x(i as f64), y(a)), egui::pos2(x(i as f64 + 1.0), y(b))], egui::Stroke::new(1.5, color));
        // Join to the next bar's start.
        if let Some(next) = curves.get(i + 1) {
            let n = next.map_or(fallback, |n| n.0);
            if (n - b).abs() > 1e-9 {
                ui.painter().line_segment([egui::pos2(x(i as f64 + 1.0), y(b)), egui::pos2(x(i as f64 + 1.0), y(n))], egui::Stroke::new(1.5, t.accent));
            }
        }
    }
    let label = |v: f64, at: f32| {
        ui.painter().text(egui::pos2(rect.left() + 4.0, at), egui::Align2::LEFT_CENTER, format!("{v:.0}"), egui::FontId::monospace(10.0), t.lcd_dim)
    };
    label(hi, inner.top());
    label(lo, inner.bottom());
}

/// Which mixer value an automation window edits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MixKind {
    #[default]
    Volume,
    Pan,
}

impl MixKind {
    fn key(self) -> &'static str {
        match self {
            MixKind::Volume => "volume",
            MixKind::Pan => "pan",
        }
    }

    fn range(self) -> (i64, i64) {
        match self {
            MixKind::Volume => (0, 127),
            MixKind::Pan => (-63, 63),
        }
    }
}

/// Sound › Automations › Volume Automation… and Pan Automation…, for one track at a time or for
/// the master.
#[derive(Default)]
pub(crate) struct MixAutomation {
    pub(crate) open: bool,
    pub(crate) kind: MixKind,
    /// A track's index, or [`MASTER`].
    pub(crate) track: usize,
    add_bar: usize,
    add_value: i64,
    add_linear: bool,
}

/// The automation window's target when it edits the master rather than a track.
pub(crate) const MASTER: usize = usize::MAX;

impl GuitarioApp {
    /// Opens the volume or pan automation of the cursor's track, ready to add a point at its bar.
    /// Sound › Automations › Volume Automation… (the snapshot example opens it too).
    pub fn open_volume_automation(&mut self) {
        self.open_mix_automation(MixKind::Volume);
    }

    pub(crate) fn open_mix_automation(&mut self, kind: MixKind) {
        let c = self.session.cursor();
        let track = self.session.score().tracks.get(c.track);
        let value = track.map_or(0, |t| if kind == MixKind::Volume { i64::from(t.volume) } else { i64::from(t.pan) });
        self.mix_automation = MixAutomation { open: true, kind, track: c.track, add_bar: c.measure + 1, add_value: value, add_linear: true };
    }

    /// Opens the master's volume automation (the snapshot example uses it).
    pub fn open_master_volume_automation(&mut self) {
        self.open_master_automation(MixKind::Volume);
    }

    /// Opens the master's volume or pan automation (the global view's Master row).
    pub(crate) fn open_master_automation(&mut self, kind: MixKind) {
        let m = &self.session.score().master;
        let value = if kind == MixKind::Volume { i64::from(m.volume) } else { i64::from(m.pan) };
        self.mix_automation = MixAutomation { open: true, kind, track: MASTER, add_bar: self.session.cursor().measure + 1, add_value: value, add_linear: true };
    }

    pub(crate) fn mix_automation_dialog(&mut self, ctx: &egui::Context) {
        if !self.mix_automation.open {
            return;
        }
        let score = std::sync::Arc::clone(self.session.score());
        let d = &mut self.mix_automation;
        let master = d.track == MASTER;
        if !master {
            d.track = d.track.min(score.tracks.len().saturating_sub(1));
        }
        let bars = score.headers.len().max(1);
        let kind = d.kind;
        let (lo, hi) = kind.range();
        // The curve, the value before the first point, and the points: the master's or a track's.
        let ((vol, pan), (own_volume, own_pan), points): (_, _, Vec<(usize, i64, bool)>) = if master {
            let points = score
                .headers
                .iter()
                .enumerate()
                .filter_map(|(i, h)| match kind {
                    MixKind::Volume => Some((i, i64::from(h.master.volume?), h.master.volume_linear)),
                    MixKind::Pan => Some((i, i64::from(h.master.pan?), h.master.pan_linear)),
                })
                .collect();
            (score.master_curves(), (f64::from(score.master.volume), f64::from(score.master.pan)), points)
        } else {
            let Some(track) = score.tracks.get(d.track) else { return };
            let points = track
                .measures
                .iter()
                .enumerate()
                .filter_map(|(i, m)| match kind {
                    MixKind::Volume => Some((i, i64::from(m.volume?), m.volume_linear)),
                    MixKind::Pan => Some((i, i64::from(m.pan?), m.pan_linear)),
                })
                .collect();
            (score.mix_curves(d.track), (f64::from(track.volume), f64::from(track.pan)), points)
        };
        let (curves, fallback) = match kind {
            MixKind::Volume => (vol, own_volume),
            MixKind::Pan => (pan, own_pan),
        };
        let names: Vec<String> = score.tracks.iter().map(|t| t.name.clone()).collect();
        let target = |track: usize| if track == MASTER { "Master".to_string() } else { names.get(track).cloned().unwrap_or_default() };
        let mut actions: Vec<Value> = Vec::new();
        let mut open = true;
        let title = if kind == MixKind::Volume { "Volume Automation" } else { "Pan Automation" };
        egui::Window::new(title).id(egui::Id::new("mix-automation")).open(&mut open).resizable(false).collapsible(false).default_pos([140.0, 130.0]).show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut d.kind, MixKind::Volume, "Volume");
                    ui.selectable_value(&mut d.kind, MixKind::Pan, "Pan");
                    egui::ComboBox::from_id_salt("mix-automation-track").selected_text(target(d.track)).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            ui.selectable_value(&mut d.track, i, n);
                        }
                        ui.selectable_value(&mut d.track, MASTER, "Master");
                    });
                });
                curve(ui, &curves, fallback, Some((lo as f64, hi as f64)));
                let whose = if master { "master's" } else { "mixer's" };
                ui.label(egui::RichText::new(format!("Before the first point the {whose} {} applies ({}).", kind.key(), fallback)).small().weak());
                // A point's parameters: the master's command takes no track.
                let at = |bar: usize, value: Value, linear: bool| {
                    let mut p = json!({"kind": kind.key(), "value": value, "linear": linear, "measure": bar});
                    if let Some(o) = p.as_object_mut().filter(|_| !master) {
                        o.insert("track".into(), json!(d.track));
                    }
                    p
                };
                egui::Grid::new("mix-points").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                    for &(bar, value, linear) in &points {
                        ui.label(format!("Bar {}", bar + 1));
                        let mut v = value;
                        let r = ui.add(egui::DragValue::new(&mut v).range(lo..=hi));
                        if (r.drag_stopped() || r.lost_focus() || (r.changed() && !r.dragged() && !r.has_focus())) && v != value {
                            actions.push(at(bar, json!(v), linear));
                        }
                        let mut gradual = linear;
                        if ui.checkbox(&mut gradual, "Gradual").changed() {
                            actions.push(at(bar, json!(value), gradual));
                        }
                        if ui.small_button("Remove").clicked() {
                            actions.push(at(bar, Value::Null, false));
                        }
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("At bar");
                    ui.add(egui::DragValue::new(&mut d.add_bar).range(1..=bars));
                    d.add_value = d.add_value.clamp(lo, hi);
                    ui.add(egui::DragValue::new(&mut d.add_value).range(lo..=hi));
                    ui.checkbox(&mut d.add_linear, "Gradual");
                    if ui.button("Add").clicked() {
                        let bar = d.add_bar.clamp(1, bars) - 1;
                        actions.push(at(bar, json!(d.add_value), d.add_linear));
                    }
                });
            },
        );
        self.mix_automation.open = open;
        let id = if master { "score.setMasterAutomation" } else { "track.setAutomation" };
        for params in actions {
            self.run(id, params);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn volume_and_pan_windows_edit_the_chosen_track() {
        let mut s = Session::new();
        s.run("measure.append", &Value::Null).unwrap();
        s.run("track.add", &json!({"name": "Bass", "tuning": "bass"})).unwrap();
        s.run("cursor.set", &json!({"track": 1, "measure": 1})).unwrap();
        let mut app = GuitarioApp::new(s, None);
        app.open_mix_automation(MixKind::Pan);
        assert_eq!((app.mix_automation.track, app.mix_automation.add_bar, app.mix_automation.kind), (1, 2, MixKind::Pan));
        app.run("track.setAutomation", json!({"kind": "pan", "value": -63, "measure": 1, "track": 1}));
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions().0);
        for kind in [MixKind::Pan, MixKind::Volume] {
            app.mix_automation.kind = kind;
            ctx.run_ui(egui::RawInput::default(), |ui| app.mix_automation_dialog(ui.ctx())).textures_delta.clear();
        }
        assert!(app.mix_automation.open);
        // A track removed under the window: it shows the last one rather than failing.
        app.mix_automation.track = 9;
        ctx.run_ui(egui::RawInput::default(), |ui| app.mix_automation_dialog(ui.ctx())).textures_delta.clear();
        assert_eq!(app.mix_automation.track, 1);
        // The master's own window edits the master's points.
        app.open_master_automation(MixKind::Volume);
        assert_eq!(app.mix_automation.track, MASTER);
        app.run("score.setMasterAutomation", json!({"kind": "volume", "value": 64, "measure": 1}));
        ctx.run_ui(egui::RawInput::default(), |ui| app.mix_automation_dialog(ui.ctx())).textures_delta.clear();
        assert_eq!((app.mix_automation.track, app.session.score().headers[1].master.volume), (MASTER, Some(64)));
    }

    #[test]
    fn the_editor_shows_points_and_adds_a_gradual_one() {
        let mut s = Session::new();
        for _ in 0..3 {
            s.run("measure.append", &Value::Null).unwrap();
        }
        s.run("cursor.set", &json!({"measure": 3})).unwrap();
        let mut app = GuitarioApp::new(s, None);
        app.open_tempo_automation();
        assert_eq!((app.tempo_automation.add_bar, app.tempo_automation.add_linear), (4, true));
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions().0);
        ctx.run_ui(egui::RawInput::default(), |ui| app.tempo_automation_dialog(ui.ctx())).textures_delta.clear();
        assert!(app.tempo_automation.open);
        // What Add sends, run directly.
        app.run("measure.setTempo", json!({"bpm": 180, "linear": true, "measure": 3}));
        ctx.run_ui(egui::RawInput::default(), |ui| app.tempo_automation_dialog(ui.ctx())).textures_delta.clear();
        let c = app.session.score().tempo_curves();
        assert!((c[2].1 - 180.0).abs() < 1e-9 && c[0].0 < 180.0);
    }
}
