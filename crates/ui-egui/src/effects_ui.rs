//! A track's effect chain and equalizer and the song's mastering, as Guitar Pro 8 shows them: rows
//! of effects in the inspector (icon, name with the library arrows, power, options), the mixer's
//! equalizer popup (an on switch, presets and five bands of knobs) and a window of knobs for each
//! effect. A knob being turned is heard at once and becomes one undo step when it is let go.

use crate::GuitarioApp;
use crate::theme::tokens;
use egui::{Color32, CornerRadius, Rect, Sense, Shape, Stroke, pos2, vec2};
use guitario_playback::TrackSound;
use guitario_score::{Effect, EffectCategory, EffectType, MASTER_SLOTS, MAX_TRACK_EFFECTS, Master, ParamSpec, Score, Unit};
use serde_json::{Value, json};

/// Which effect a control edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// A track's chain: track, slot.
    Effect(usize, usize),
    /// A track's mixer equalizer.
    Eq(usize),
    /// A mastering slot (compressor, reverb, equalizer).
    Master(usize),
}

#[derive(Default)]
pub(crate) struct SoundUi {
    /// The equalizer popup's track.
    pub(crate) eq: Option<usize>,
    /// The effect whose knobs are open.
    pub(crate) effect: Option<Target>,
    /// A knob being turned: what, which parameter, its value so far.
    pub(crate) drag: Option<(Target, &'static str, f32)>,
}

/// The effect a target points at in `score`.
pub(crate) fn effect_of(score: &Score, target: Target) -> Option<Effect> {
    match target {
        Target::Effect(ti, slot) => score.tracks.get(ti)?.chain().get(slot).cloned(),
        Target::Eq(ti) => Some(score.tracks.get(ti)?.eq.clone()),
        Target::Master(i) => score.master.slots().get(i).map(|e| (*e).clone()),
    }
}

/// The command that sets `params` (preset, on, params) on a target.
fn command(target: Target, mut params: Value) -> (&'static str, Value) {
    let (id, extra) = match target {
        Target::Effect(ti, slot) => ("track.setEffect", json!({"track": ti, "slot": slot})),
        Target::Eq(ti) => ("track.setEq", json!({"track": ti})),
        Target::Master(i) => ("score.setMastering", json!({"slot": i})),
    };
    if let (Some(p), Some(e)) = (params.as_object_mut(), extra.as_object()) {
        p.extend(e.clone());
    }
    (id, params)
}

/// The name an effect row shows: its type, with the model for an amp or a cabinet.
pub(crate) fn display_name(e: &Effect) -> String {
    match e.kind.params().iter().find(|s| s.unit == Unit::Choice) {
        Some(s) => format!("{} \u{b7} {}", e.kind.label(), format_value(s, e.value(s.name))),
        None => e.kind.label().to_string(),
    }
}

impl GuitarioApp {
    /// What the player should hear now: each track's effects and equalizer, with a knob being
    /// turned.
    pub(crate) fn live_sound(&self) -> Vec<TrackSound> {
        let score = self.session.score();
        let mut sound: Vec<TrackSound> = score.tracks.iter().map(TrackSound::of).collect();
        if let Some((target, name, v)) = self.sound_ui.drag {
            match target {
                Target::Effect(ti, slot) => {
                    if let (Some(s), Some(t)) = (sound.get_mut(ti), score.tracks.get(ti)) {
                        let mut chain = t.chain();
                        if let Some(e) = chain.get_mut(slot) {
                            let _ = e.set(name, v);
                        }
                        s.effects = Some(chain);
                    }
                }
                Target::Eq(ti) => {
                    if let Some(s) = sound.get_mut(ti) {
                        let _ = s.eq.set(name, v);
                    }
                }
                Target::Master(_) => {}
            }
        }
        sound
    }

    /// The master as the player should hear it now, with a knob or fader being turned.
    pub(crate) fn live_master(&self) -> Master {
        let mut m = self.session.score().master.clone();
        if let Some((Target::Master(i), name, v)) = self.sound_ui.drag
            && let Some(e) = m.slot_mut(i)
        {
            let _ = e.set(name, v);
        }
        if let Some((_, control, v)) = self.mixer.drag {
            match control {
                crate::mixer::Control::MasterVolume => m.volume = v.clamp(0, 127) as u8,
                crate::mixer::Control::MasterPan => m.pan = v.clamp(-63, 63) as i8,
                crate::mixer::Control::MasterFocus => m.focus = v.clamp(-100, 100) as i8,
                _ => {}
            }
        }
        m
    }

    /// Sends the live effects and master to the player, if one is playing.
    pub(crate) fn push_sound(&self) {
        let (sound, master) = (self.live_sound(), self.live_master());
        if let Some(audio) = &self.audio
            && let Some(p) = audio.slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut()
        {
            p.set_sound(&sound);
            p.set_master(&master);
        }
    }

    /// Opens the mixer's equalizer for a track (the global view's Eq. button).
    pub fn open_equalizer(&mut self, track: usize) {
        self.sound_ui.eq = Some(track);
    }

    /// Opens the knobs of the effect in a track's chain (the snapshot example uses it).
    pub fn open_track_effect(&mut self, track: usize, slot: usize) {
        self.open_effect(Target::Effect(track, slot));
    }

    /// Opens the knobs of a mastering slot: 0 compressor, 1 reverb, 2 equalizer.
    pub fn open_mastering(&mut self, slot: usize) {
        self.open_effect(Target::Master(slot));
    }

    /// Opens an effect's knobs.
    pub(crate) fn open_effect(&mut self, target: Target) {
        self.sound_ui.effect = Some(target);
    }

    /// The equalizer popup and the effect window, when open.
    pub(crate) fn sound_windows(&mut self, ctx: &egui::Context) {
        let score = self.session.score().clone();
        let mut acts: Vec<(&'static str, Value)> = Vec::new();
        let mut drag = self.sound_ui.drag;
        if let Some(ti) = self.sound_ui.eq {
            let mut open = true;
            match (score.tracks.get(ti), effect_of(&score, Target::Eq(ti))) {
                (Some(tr), Some(e)) => {
                    egui::Window::new(format!("Equalizer \u{2013} {}", tr.name))
                        .id(egui::Id::new("track-equalizer"))
                        .open(&mut open)
                        .resizable(false)
                        .collapsible(false)
                        .default_pos([420.0, 420.0])
                        .show(ctx, |ui| panel(ui, &e, Target::Eq(ti), &mut drag, &mut acts));
                }
                _ => open = false,
            }
            if !open {
                self.sound_ui.eq = None;
            }
        }
        if let Some(target) = self.sound_ui.effect {
            let mut open = true;
            let owner = match target {
                Target::Effect(ti, _) | Target::Eq(ti) => score.tracks.get(ti).map_or_else(String::new, |t| t.name.clone()),
                Target::Master(_) => "Mastering".to_string(),
            };
            match effect_of(&score, target) {
                Some(e) => {
                    egui::Window::new(format!("{} \u{2013} {owner}", e.kind.label()))
                        .id(egui::Id::new("effect-settings"))
                        .open(&mut open)
                        .resizable(false)
                        .collapsible(false)
                        .default_pos([520.0, 200.0])
                        .show(ctx, |ui| panel(ui, &e, target, &mut drag, &mut acts));
                }
                None => open = false,
            }
            if !open {
                self.sound_ui.effect = None;
            }
        }
        let dragging = drag.is_some() || self.sound_ui.drag.is_some();
        self.sound_ui.drag = drag;
        for (id, p) in acts {
            self.run(id, p);
        }
        if dragging {
            self.push_sound();
        }
    }
}

/// A knob's position, 0 to 1, for a value: logarithmic for frequencies.
fn knob_pos(spec: &ParamSpec, v: f32) -> f32 {
    let t = if spec.unit == Unit::Hertz && spec.min > 0.0 {
        (v / spec.min).ln() / (spec.max / spec.min).ln()
    } else {
        (v - spec.min) / (spec.max - spec.min).max(1e-6)
    };
    if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 }
}

fn knob_value(spec: &ParamSpec, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let v = if spec.unit == Unit::Hertz && spec.min > 0.0 { spec.min * (spec.max / spec.min).powf(t) } else { spec.min + (spec.max - spec.min) * t };
    // Round to what the label shows.
    let step = match spec.unit {
        Unit::Hertz if v >= 1000.0 => 10.0,
        Unit::Hertz | Unit::Milliseconds if v >= 100.0 => 1.0,
        Unit::Percent | Unit::Choice => 1.0,
        Unit::Q | Unit::Ratio => 0.1,
        _ => 0.1,
    };
    ((v / step).round() * step).clamp(spec.min, spec.max)
}

/// "-3.5 dB", "250 Hz", "1.2k Hz", "4:1".
pub(crate) fn format_value(spec: &ParamSpec, v: f32) -> String {
    match spec.unit {
        Unit::Knob => format!("{v:.1}"),
        Unit::Decibels => format!("{v:+.1} dB"),
        Unit::Hertz if v >= 1000.0 => format!("{:.1}k Hz", v / 1000.0),
        Unit::Hertz if v < 10.0 => format!("{v:.1} Hz"),
        Unit::Hertz => format!("{v:.0} Hz"),
        Unit::Milliseconds if v < 10.0 => format!("{v:.1} ms"),
        Unit::Milliseconds => format!("{v:.0} ms"),
        Unit::Percent => format!("{v:.0}%"),
        Unit::Ratio => format!("{v:.1}:1"),
        Unit::Q => format!("Q {v:.1}"),
        Unit::Choice => spec.choices.get(v.round() as usize).map_or_else(String::new, |s| (*s).to_string()),
    }
}

/// Guitar Pro's knob: a dark disc, a blue arc from the start (from the centre for a ±dB knob) to
/// the value, and a white pointer. Drag up or right to turn it; double-click for the default.
/// Returns the value while it turns and whether it was let go.
fn knob(ui: &mut egui::Ui, spec: &ParamSpec, value: f32, id: egui::Id) -> (Option<f32>, bool) {
    let t = tokens();
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click_and_drag());
    let resp = resp.on_hover_text(format!("{}: {}", spec.label, format_value(spec, value)));
    let pos = knob_pos(spec, value);
    let p = ui.painter();
    let c = rect.center();
    let r = 13.0;
    let (a0, a1) = (0.75 * std::f32::consts::PI, 2.25 * std::f32::consts::PI);
    let angle = |x: f32| a0 + (a1 - a0) * x;
    let arc = |from: f32, to: f32| -> Vec<egui::Pos2> {
        let n = 24;
        (0..=n).map(|i| angle(from + (to - from) * i as f32 / n as f32)).map(|a| c + vec2(r * a.cos(), r * a.sin())).collect()
    };
    p.add(Shape::line(arc(0.0, 1.0), Stroke::new(2.5, Color32::from_gray(0x55))));
    let from = if spec.unit == Unit::Decibels && spec.min < 0.0 { knob_pos(spec, 0.0) } else { 0.0 };
    if (pos - from).abs() > 0.005 {
        p.add(Shape::line(arc(from.min(pos), from.max(pos)), Stroke::new(2.5, t.accent)));
    }
    p.circle_filled(c, r - 3.0, Color32::from_rgb(0x10, 0x11, 0x14));
    let a = angle(pos);
    p.line_segment([c + vec2(3.0 * a.cos(), 3.0 * a.sin()), c + vec2((r - 4.0) * a.cos(), (r - 4.0) * a.sin())], Stroke::new(2.0, Color32::WHITE));
    if resp.double_clicked() {
        return (Some(spec.default), true);
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        let held: f32 = ui.data(|m| m.get_temp(id)).unwrap_or(pos);
        let next = (held + (d.x - d.y) / 160.0).clamp(0.0, 1.0);
        ui.data_mut(|m| m.insert_temp(id, next));
        return (Some(knob_value(spec, next)), false);
    }
    if resp.drag_stopped() {
        ui.data_mut(|m| m.remove::<f32>(id));
        return (Some(value), true);
    }
    (None, false)
}

/// A knob for one parameter, with its unit (equalizer) or name (effects) above it. Turning it
/// records the drag; letting go queues the command.
fn param_knob(
    ui: &mut egui::Ui,
    e: &Effect,
    spec: &'static ParamSpec,
    target: Target,
    caption: &str,
    drag: &mut Option<(Target, &'static str, f32)>,
    acts: &mut Vec<(&'static str, Value)>,
) {
    let value = match *drag {
        Some((t, name, v)) if t == target && name == spec.name => v,
        _ => e.value(spec.name),
    };
    ui.allocate_ui_with_layout(vec2(52.0, 74.0), egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(egui::RichText::new(caption).small());
        let (turned, released) = knob(ui, spec, value, egui::Id::new(("knob", format!("{target:?}"), spec.name)));
        if let Some(v) = turned {
            if released {
                *drag = None;
                acts.push(command(target, json!({"params": {spec.name: v}})));
            } else {
                *drag = Some((target, spec.name, v));
            }
        }
        ui.label(egui::RichText::new(format_value(spec, value)).small().color(tokens().text_dim));
    });
}

/// The settings of one effect: on switch, presets, then a knob per parameter (a list for a
/// choice). The mixer's equalizer groups its knobs by band, as Guitar Pro's popup does.
fn panel(ui: &mut egui::Ui, e: &Effect, target: Target, drag: &mut Option<(Target, &'static str, f32)>, acts: &mut Vec<(&'static str, Value)>) {
    ui.horizontal(|ui| {
        if crate::inspector::switch(ui, e.on) {
            acts.push(command(target, json!({"on": !e.on})));
        }
        ui.add_space(24.0);
        let shown = e.preset_name().unwrap_or("Presets");
        egui::ComboBox::from_id_salt(("presets", format!("{target:?}"))).width(150.0).selected_text(shown).show_ui(ui, |ui| {
            for (name, _) in e.kind.presets() {
                if ui.selectable_label(e.preset_name() == Some(*name), *name).clicked() {
                    acts.push(command(target, json!({"preset": name})));
                }
            }
        });
    });
    ui.add_space(6.0);
    let spec = |name: &str| e.kind.param(name);
    if e.kind == EffectType::MixerEq {
        let groups: [(&str, &[&str]); 5] = [
            ("Low", &["low"]),
            ("Low Medium", &["lowMid", "lowMidFreq"]),
            ("Medium", &["midQ", "mid", "midFreq"]),
            ("High Medium", &["highMid", "highMidFreq"]),
            ("High", &["high"]),
        ];
        ui.horizontal_top(|ui| {
            for (label, names) in groups {
                ui.vertical(|ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for s in names.iter().filter_map(|n| spec(n)) {
                            let unit = match s.unit {
                                Unit::Hertz => "Hz",
                                Unit::Q => "Q",
                                _ => "dB",
                            };
                            param_knob(ui, e, s, target, unit, drag, acts);
                        }
                    });
                    let w = 52.0 * names.len() as f32;
                    ui.allocate_ui_with_layout(vec2(w, 18.0), egui::Layout::top_down(egui::Align::Center), |ui| ui.label(label));
                });
                ui.add_space(8.0);
            }
        });
        return;
    }
    // Choices first, as lists.
    for s in e.kind.params().iter().filter(|s| s.unit == Unit::Choice) {
        ui.horizontal(|ui| {
            ui.label(format!("{}:", s.label));
            let now = e.value(s.name).round() as usize;
            egui::ComboBox::from_id_salt(("choice", format!("{target:?}"), s.name))
                .width(140.0)
                .selected_text(s.choices.get(now).copied().unwrap_or_default())
                .show_ui(ui, |ui| {
                    for (i, c) in s.choices.iter().enumerate() {
                        if ui.selectable_label(i == now, *c).clicked() {
                            acts.push(command(target, json!({"params": {s.name: i}})));
                        }
                    }
                });
        });
    }
    let knobs: Vec<&'static ParamSpec> = e.kind.params().iter().filter(|s| s.unit != Unit::Choice).collect();
    for row in knobs.chunks(if e.kind == EffectType::Equalizer { 11 } else { 6 }) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for s in row {
                param_knob(ui, e, s, target, s.label, drag, acts);
            }
        });
    }
}

/// Guitar Pro's icon for an effect's family: a pedal, an amp, or a rack unit with a meter.
pub(crate) fn effect_icon(p: &egui::Painter, cell: Rect, kind: Option<EffectType>, ink: Color32) {
    let stroke = Stroke::new(1.3, ink);
    let c = cell.center();
    match kind.map(EffectType::category) {
        Some(EffectCategory::Pedal) => {
            let r = Rect::from_center_size(c, vec2(11.0, 16.0));
            p.rect_stroke(r, 1.5, stroke, egui::StrokeKind::Inside);
            p.line_segment([pos2(r.left(), r.top() + 6.0), pos2(r.right(), r.top() + 6.0)], stroke);
            for dx in [-3.0, 0.0, 3.0] {
                p.circle_filled(pos2(c.x + dx, r.top() + 3.0), 0.9, ink);
            }
        }
        Some(EffectCategory::Amp) => {
            let r = Rect::from_center_size(c, vec2(18.0, 13.0));
            p.rect_stroke(r, 1.5, stroke, egui::StrokeKind::Inside);
            p.line_segment([pos2(r.left(), r.top() + 4.0), pos2(r.right(), r.top() + 4.0)], stroke);
            for i in 0..5 {
                p.circle_filled(pos2(r.left() + 3.5 + 2.75 * i as f32, r.top() + 2.0), 0.7, ink);
            }
        }
        Some(EffectCategory::Rack) => {
            let r = Rect::from_center_size(c, vec2(18.0, 11.0));
            p.rect_filled(r, 2.0, ink);
            let dark = Color32::from_rgb(0x30, 0x33, 0x3c);
            let m = pos2(r.left() + 6.0, r.bottom() - 2.5);
            let pts: Vec<_> = (0..=8).map(|i| std::f32::consts::PI * (1.0 + i as f32 / 8.0)).map(|a| m + vec2(4.0 * a.cos(), 4.0 * a.sin())).collect();
            p.add(Shape::line(pts, Stroke::new(1.0, dark)));
            for (dx, dy) in [(12.0, 3.0), (15.0, 6.0)] {
                p.line_segment([pos2(r.left() + dx, r.top() + 2.0), pos2(r.left() + dx, r.bottom() - 2.0)], Stroke::new(1.0, dark));
                p.line_segment([pos2(r.left() + dx - 1.5, r.top() + dy), pos2(r.left() + dx + 1.5, r.top() + dy)], Stroke::new(1.2, dark));
            }
        }
        None => {}
    }
}

/// Up and down chevrons: an effect library.
fn chevrons(p: &egui::Painter, at: egui::Pos2, ink: Color32) {
    for (dy, tip) in [(-4.0, -1.0), (4.0, 1.0)] {
        p.add(Shape::line(vec![at + vec2(-4.0, dy - 2.0 * tip), at + vec2(0.0, dy + 2.0 * tip), at + vec2(4.0, dy - 2.0 * tip)], Stroke::new(1.4, ink)));
    }
}

/// Power: an open ring with a bar through its gap.
fn power(p: &egui::Painter, c: egui::Pos2, ink: Color32) {
    let ring: Vec<egui::Pos2> = (0..=20)
        .map(|i| -std::f32::consts::FRAC_PI_2 + 0.6 + i as f32 * (2.0 * std::f32::consts::PI - 1.2) / 20.0)
        .map(|a| c + 6.5 * vec2(a.cos(), a.sin()))
        .collect();
    p.add(Shape::line(ring, Stroke::new(1.6, ink)));
    p.line_segment([c + vec2(0.0, -8.0), c + vec2(0.0, -1.5)], Stroke::new(1.6, ink));
}

/// The four parts of an effect row a click can land on.
pub(crate) struct Row {
    pub(crate) icon: egui::Response,
    pub(crate) name: egui::Response,
    pub(crate) power: egui::Response,
    pub(crate) more: egui::Response,
}

/// One effect row: icon (opens the knobs), name with the library arrows, power, options. `None`
/// draws the empty slot that adds an effect (only `name` matters then).
pub(crate) fn effect_row(ui: &mut egui::Ui, id: egui::Id, e: Option<&Effect>) -> Row {
    let t = tokens();
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    let (icon, rest) = r.split_left_right_at_x(r.left() + 30.0);
    let (name, rest) = rest.split_left_right_at_x(rest.right() - 62.0);
    let (pow, more) = rest.split_left_right_at_x(rest.left() + 34.0);
    let hit = |cell: Rect, part: &str| ui.interact(cell, id.with(part), Sense::click());
    let row = Row { icon: hit(icon, "icon"), name: hit(name, "name"), power: hit(pow, "power"), more: hit(more, "more") };
    let p = ui.painter();
    let Some(e) = e else {
        let whole = hit(r, "add");
        p.rect_filled(r, 5.0, if whole.hovered() { t.control_hover } else { t.control.gamma_multiply(0.55) });
        if whole.hovered() {
            p.text(r.center(), egui::Align2::CENTER_CENTER, "Add an effect", egui::FontId::proportional(13.0), t.text_dim);
        }
        return Row { name: whole, ..row };
    };
    let ink = if e.on { t.text } else { t.text_dim };
    for (cell, resp, corners) in [
        (icon, &row.icon, CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 }),
        (name, &row.name, CornerRadius::ZERO),
        (pow, &row.power, CornerRadius::ZERO),
        (more, &row.more, CornerRadius { nw: 0, sw: 0, ne: 5, se: 5 }),
    ] {
        p.rect_filled(cell.shrink2(vec2(0.5, 0.0)), corners, if resp.hovered() { t.control_hover } else { t.control });
    }
    effect_icon(p, icon, Some(e.kind), ink);
    p.with_clip_rect(name.shrink2(vec2(4.0, 0.0))).text(
        pos2(name.left() + 10.0, name.center().y),
        egui::Align2::LEFT_CENTER,
        display_name(e),
        egui::FontId::proportional(14.0),
        ink,
    );
    chevrons(p, pos2(name.right() - 14.0, name.center().y), ink);
    power(p, pow.center(), if e.on { t.text } else { t.text_dim.gamma_multiply(0.5) });
    crate::icons::draw(p, more.shrink(7.0), crate::icons::Icon::More, t.text);
    row
}

/// The track inspector's effect chain: a row per effect and an empty slot to add one.
pub(crate) fn chain_section(app: &mut GuitarioApp, ui: &mut egui::Ui, tr: &guitario_score::Track, ti: usize, actions: &mut Vec<(&'static str, Value)>) {
    let chain = tr.chain();
    ui.horizontal(|ui| {
        ui.label("Effect chain");
        let (r, more) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
        crate::icons::draw(ui.painter(), r.shrink(2.0), crate::icons::Icon::More, if more.hovered() { Color32::WHITE } else { tokens().text });
        let more = more.on_hover_text("Chain options");
        egui::Popup::menu(&more).show(|ui| {
            if ui.add_enabled(tr.effects.is_some(), egui::Button::new("Reset to the Instrument's Chain")).clicked() {
                actions.push(("track.resetEffects", json!({"track": ti})));
                ui.close();
            }
        });
        if tr.effects.is_none() {
            ui.label(egui::RichText::new("the instrument's").small().color(tokens().text_dim))
                .on_hover_text("The built-in sound plays this chain; a sampled instrument plays none. Editing it gives the track its own.");
        }
    });
    let n = chain.len();
    for (slot, e) in chain.iter().enumerate() {
        let row = effect_row(ui, egui::Id::new(("chain", ti, slot)), Some(e));
        let target = Target::Effect(ti, slot);
        if row.icon.on_hover_text("Effect settings").clicked() {
            app.open_effect(target);
        }
        if row.power.on_hover_text(if e.on { "Turn off" } else { "Turn on" }).clicked() {
            actions.push(command(target, json!({"on": !e.on})));
        }
        egui::Popup::menu(&row.name).show(|ui| library_menu(ui, Some(e), target, actions));
        egui::Popup::menu(&row.more).show(|ui| {
            if ui.add_enabled(slot > 0, egui::Button::new("Move Up")).clicked() {
                actions.push(("track.moveEffect", json!({"track": ti, "slot": slot, "to": slot - 1})));
                ui.close();
            }
            if ui.add_enabled(slot + 1 < n, egui::Button::new("Move Down")).clicked() {
                actions.push(("track.moveEffect", json!({"track": ti, "slot": slot, "to": slot + 1})));
                ui.close();
            }
            if ui.button("Delete").clicked() {
                actions.push(("track.removeEffect", json!({"track": ti, "slot": slot})));
                ui.close();
            }
        });
        ui.add_space(3.0);
    }
    if n < MAX_TRACK_EFFECTS {
        let row = effect_row(ui, egui::Id::new(("chain-add", ti)), None);
        egui::Popup::menu(&row.name).show(|ui| library_menu(ui, None, Target::Effect(ti, n), actions));
    }
}

/// The effect library: the slot's presets, then every type to put in the slot instead.
fn library_menu(ui: &mut egui::Ui, e: Option<&Effect>, target: Target, actions: &mut Vec<(&'static str, Value)>) {
    if let Some(e) = e {
        for (name, _) in e.kind.presets() {
            if ui.selectable_label(e.preset_name() == Some(*name), *name).clicked() {
                actions.push(command(target, json!({"preset": name})));
                ui.close();
            }
        }
        if matches!(target, Target::Master(_)) {
            return;
        }
        ui.separator();
    }
    for kind in EffectType::CHAIN {
        if e.is_some_and(|e| e.kind == kind) {
            continue;
        }
        if ui.button(kind.label()).clicked() {
            actions.push(command(target, json!({"type": kind.id()})));
            ui.close();
        }
    }
}

/// The song inspector's Sound Mastering: compressor, reverb and equalizer, each its own type.
pub(crate) fn mastering_section(app: &mut GuitarioApp, ui: &mut egui::Ui, master: &Master, actions: &mut Vec<(&'static str, Value)>) {
    for (i, (e, (name, _))) in master.slots().into_iter().zip(MASTER_SLOTS).enumerate() {
        let row = effect_row(ui, egui::Id::new(("mastering", i)), Some(e));
        let target = Target::Master(i);
        if row.icon.on_hover_text(format!("Mastering {name} settings")).clicked() {
            app.open_effect(target);
        }
        if row.power.on_hover_text(if e.on { "Turn off" } else { "Turn on" }).clicked() {
            actions.push(command(target, json!({"on": !e.on})));
        }
        egui::Popup::menu(&row.name).show(|ui| library_menu(ui, Some(e), target, actions));
        let default = Master::default();
        egui::Popup::menu(&row.more).show(|ui| {
            if ui.button("Default Settings").clicked() {
                let d = default.slots().get(i).map(|d| d.params.clone()).unwrap_or_default();
                actions.push(command(target, json!({"params": d})));
                ui.close();
            }
        });
        ui.add_space(3.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn knobs_map_values_both_ways_and_label_them() {
        let eq = EffectType::MixerEq;
        let hz = eq.param("midFreq").unwrap();
        assert!((knob_pos(hz, 300.0)).abs() < 1e-6 && (knob_pos(hz, 5000.0) - 1.0).abs() < 1e-6);
        assert_eq!(knob_value(hz, knob_pos(hz, 1000.0)), 1000.0);
        let db = eq.param("mid").unwrap();
        assert_eq!(knob_value(db, 0.5), 0.0);
        assert_eq!(knob_pos(db, f32::NAN), 0.0);
        assert_eq!(format_value(db, -3.5), "-3.5 dB");
        assert_eq!(format_value(hz, 1200.0), "1.2k Hz");
        let amp = EffectType::Amp.param("model").unwrap();
        assert_eq!(format_value(amp, 2.0), "Lead");
        assert_eq!(command(Target::Master(1), json!({"on": true})), ("score.setMastering", json!({"on": true, "slot": 1})));
    }

    #[test]
    fn a_turning_knob_is_heard_live_and_saved_once_on_release() {
        let mut s = Session::new();
        s.run("track.setProgram", &json!({"program": 30})).unwrap();
        let mut app = GuitarioApp::new(s, None);
        app.sound_ui.drag = Some((Target::Effect(0, 1), "gain", 2.0));
        let live = app.live_sound();
        assert_eq!(live[0].effects.as_ref().map(|c| c[1].value("gain")), Some(2.0));
        assert!(app.session.score().tracks[0].effects.is_none(), "nothing saved while turning");
        app.sound_ui.drag = Some((Target::Eq(0), "high", 6.0));
        assert_eq!(app.live_sound()[0].eq.value("high"), 6.0);
        app.sound_ui.drag = Some((Target::Master(1), "mix", 50.0));
        assert_eq!(app.live_master().reverb.value("mix"), 50.0);
        // A stale target is ignored.
        app.sound_ui.drag = Some((Target::Effect(7, 9), "gain", 2.0));
        assert_eq!(app.live_sound().len(), 1);
        let (id, p) = command(Target::Effect(0, 1), json!({"params": {"gain": 2.0}}));
        app.run(id, p).unwrap();
        assert_eq!(app.session.score().tracks[0].effects.as_ref().map(|c| c[1].value("gain")), Some(2.0));
    }

    #[test]
    fn the_windows_and_sections_draw() {
        let mut s = Session::new();
        s.run("track.setProgram", &json!({"program": 30})).unwrap();
        let mut app = GuitarioApp::new(s, None);
        app.open_equalizer(0);
        app.open_effect(Target::Effect(0, 1));
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions().0);
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                app.sound_windows(ui.ctx());
                let score = app.session.score().clone();
                let mut actions = Vec::new();
                chain_section(&mut app, ui, &score.tracks[0], 0, &mut actions);
                mastering_section(&mut app, ui, &score.master, &mut actions);
                assert!(actions.is_empty());
            })
            .textures_delta
            .clear();
        }
        assert!(app.sound_ui.eq.is_some() && app.sound_ui.effect.is_some());
        // A window whose effect has gone closes.
        app.open_effect(Target::Effect(0, 5));
        ctx.run_ui(egui::RawInput::default(), |ui| app.sound_windows(ui.ctx())).textures_delta.clear();
        assert!(app.sound_ui.effect.is_none());
    }
}
