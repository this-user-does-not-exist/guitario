//! The toolbar, in Guitar Pro 8's order: home, panel toggles, zoom, display modes, undo/redo,
//! print; the transport, the LCD, loop and speed, relative tonality; then audio track,
//! instrument views, tuner, line-in and Fretlight. Buttons sit in rounded groups; a toggle that is
//! on turns blue. When the window is narrow the groups at the right end go first (their actions
//! stay in the menus), as Guitar Pro's do.

use crate::GuitarioApp;
use crate::icons::{Icon, draw};
use crate::layout::LayoutMode;
use crate::menu_catalog::UiAction as U;
use crate::theme::{tokens, track_color};
use egui::{Color32, CornerRadius, Rect, Sense, Shape, Stroke, pos2, vec2};
use guitario_score::{Score, TripletFeel, playback_order};
use serde_json::{Value, json};

/// Height of a button group and width of one of its buttons.
const H: f32 = 30.0;
const SEG: f32 = 32.0;
const RADIUS: u8 = 6;
const LCD_W: f32 = 400.0;
const LCD_H: f32 = 46.0;
/// Space between groups.
const GAP: f32 = 8.0;

/// Seconds from the start to `measure` (first pass) and the whole song, following repeats.
fn song_times(score: &Score, measure: usize, beat_ticks: u32) -> (f64, f64) {
    let secs = |mi: usize, ticks: u32| f64::from(ticks) / f64::from(guitario_theory::TICKS_PER_QUARTER) * 60.0 / f64::from(score.tempo_at(mi).max(1));
    let mut total = 0.0;
    let mut at = None;
    for mi in playback_order(score) {
        if mi == measure && at.is_none() {
            at = Some(total + secs(mi, beat_ticks));
        }
        total += secs(mi, score.headers.get(mi).map_or(0, |h| h.capacity()));
    }
    (at.unwrap_or(0.0), total)
}

/// Minutes and seconds as Guitar Pro's LCD shows them: `01:45`.
fn clock(s: f64) -> String {
    let s = if s.is_finite() { s.max(0.0).round() as u64 } else { 0 };
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// One decimal, as the LCD's bar check shows durations.
fn one_decimal(x: f64) -> String {
    format!("{:.1}", if x.is_finite() { x } else { 0.0 })
}

/// A run of buttons on one rounded background. Buttons are laid left to right; `finish` paints
/// the background behind them.
struct Group {
    bg: egui::layers::ShapeIdx,
    rect: Option<Rect>,
}

/// Where a button sits in its group, for the corners of its blue "on" fill.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    Only,
    First,
    Middle,
    Last,
}

impl Place {
    fn corners(self) -> CornerRadius {
        let r = RADIUS;
        match self {
            Place::Only => CornerRadius::same(r),
            Place::First => CornerRadius { nw: r, sw: r, ne: 0, se: 0 },
            Place::Middle => CornerRadius::ZERO,
            Place::Last => CornerRadius { nw: 0, sw: 0, ne: r, se: r },
        }
    }
}

impl Group {
    fn begin(ui: &mut egui::Ui) -> Group {
        Group { bg: ui.painter().add(Shape::Noop), rect: None }
    }

    fn add(&mut self, r: Rect) {
        self.rect = Some(self.rect.map_or(r, |g| g.union(r)));
    }

    /// An icon button. `on` fills it blue; a disabled one is dimmed and ignores clicks.
    fn icon(&mut self, ui: &mut egui::Ui, icon: Icon, place: Place, on: bool, enabled: bool, tip: &str) -> egui::Response {
        self.icon_w(ui, icon, SEG, place, on, enabled, tip)
    }

    #[allow(clippy::too_many_arguments)]
    fn icon_w(&mut self, ui: &mut egui::Ui, icon: Icon, w: f32, place: Place, on: bool, enabled: bool, tip: &str) -> egui::Response {
        let t = tokens();
        let (rect, resp) = ui.allocate_exact_size(vec2(w, H), if enabled { Sense::click() } else { Sense::hover() });
        self.add(rect);
        let p = ui.painter();
        if on {
            p.rect_filled(rect, place.corners(), t.accent);
        } else if resp.hovered() && enabled {
            p.rect_filled(rect, place.corners(), t.control_hover);
        }
        let color = if !enabled {
            t.text_dim.gamma_multiply(0.45)
        } else if on {
            Color32::WHITE
        } else {
            t.text
        };
        draw(p, Rect::from_center_size(rect.center(), vec2(18.0, 18.0)), icon, color);
        resp.on_hover_text(tip)
    }

    /// A value with up/down arrows at its right, as Guitar Pro's zoom and speed boxes have. Returns
    /// the value's response (click it for a list) and whether an arrow was clicked (+1 up, -1 down).
    #[allow(clippy::too_many_arguments)]
    fn spinner(&mut self, ui: &mut egui::Ui, id: &str, icon: Option<Icon>, text: &str, w: f32, on: bool, tip: &str) -> (egui::Response, i32) {
        let t = tokens();
        let (rect, _) = ui.allocate_exact_size(vec2(w, H), Sense::hover());
        self.add(rect);
        let arrows = Rect::from_min_max(pos2(rect.right() - 14.0, rect.top()), rect.max);
        let body = Rect::from_min_max(rect.min, pos2(arrows.left(), rect.bottom()));
        let resp = ui.interact(body, ui.id().with((id, "body")), Sense::click()).on_hover_text(tip);
        let up = ui.interact(Rect::from_min_max(arrows.min, pos2(arrows.right(), arrows.center().y)), ui.id().with((id, "up")), Sense::click());
        let down = ui.interact(Rect::from_min_max(pos2(arrows.left(), arrows.center().y), arrows.max), ui.id().with((id, "down")), Sense::click());
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(body, 0.0, t.control_hover);
        }
        let ink = if on { t.accent } else { t.text };
        let mut x = body.left() + 6.0;
        if let Some(icon) = icon {
            draw(p, Rect::from_center_size(pos2(x + 8.0, body.center().y), vec2(16.0, 16.0)), icon, ink);
            x += 19.0;
        }
        let text_rect = Rect::from_min_max(pos2(x, body.top()), pos2(body.right() - 2.0, body.bottom()));
        p.with_clip_rect(text_rect).text(
            text_rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(13.0),
            if on { t.accent } else { t.text },
        );
        for (r, dir) in [(&up, -1.0f32), (&down, 1.0)] {
            let col = if r.hovered() { t.text } else { t.text_dim };
            let cx = arrows.center().x;
            let cy = arrows.center().y + dir * 4.5;
            p.add(Shape::line(vec![pos2(cx - 3.0, cy - dir * 1.8), pos2(cx, cy + dir * 1.8), pos2(cx + 3.0, cy - dir * 1.8)], Stroke::new(1.3, col)));
        }
        let step = if up.clicked() {
            1
        } else if down.clicked() {
            -1
        } else {
            0
        };
        (resp, step)
    }

    fn finish(self, ui: &mut egui::Ui) {
        let t = tokens();
        if let Some(r) = self.rect {
            ui.painter().set(self.bg, egui::epaint::RectShape::filled(r, CornerRadius::same(RADIUS), t.control));
        }
    }
}

/// Zoom levels the toolbar steps through (Guitar Pro offers 10% to 800%; ours stops at 25-400%).
const ZOOMS: [f32; 11] = [0.25, 0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0, 3.0, 4.0];
const SPEEDS: [f32; 9] = [0.25, 0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0];

/// The next value of `list` above (`dir` 1) or below (`dir` -1) `now`.
fn step_in(list: &[f32], now: f32, dir: i32) -> f32 {
    if dir > 0 { list.iter().copied().find(|v| *v > now + 1e-4).unwrap_or(now) } else { list.iter().rev().copied().find(|v| *v < now - 1e-4).unwrap_or(now) }
}

/// Which optional groups fit: (tonality, audio track, tuner and line-in, Fretlight).
fn fits(width: f32) -> (bool, bool, bool, bool) {
    let core = 450.0 + 5.0 * SEG + LCD_W + 140.0 + 3.0 * SEG + 8.0 * GAP;
    let tonality = 84.0 + GAP;
    let audio = SEG + GAP;
    let tuner = 2.0 * SEG + GAP;
    let fretlight = SEG + GAP;
    let mut room = width - core;
    let mut take = |w: f32| {
        let ok = room >= w;
        if ok {
            room -= w;
        }
        ok
    };
    (take(tonality), take(audio), take(tuner), take(fretlight))
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let full = ui.max_rect();
    let (show_tonality, show_audio, show_tuner, show_fretlight) = fits(full.width());
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        left(app, ui);

        // Transport, LCD, loop and speed, tonality: centred in the toolbar when there is room.
        let block = 5.0 * SEG + 4.0 + GAP + LCD_W + GAP + 120.0 + if show_tonality { GAP + 84.0 } else { 0.0 };
        let right_w = 3.0 * SEG
            + if show_audio { SEG + GAP } else { 0.0 }
            + if show_tuner { 2.0 * SEG + GAP } else { 0.0 }
            + if show_fretlight { SEG + GAP } else { 0.0 };
        let centred = full.center().x - block / 2.0;
        let x = ui.cursor().min.x;
        let limit = full.right() - right_w - GAP - block;
        ui.add_space((centred.min(limit) - x).max(GAP));
        transport(app, ui);
        ui.add_space(GAP);
        lcd(app, ui);
        ui.add_space(GAP);
        loop_speed(app, ui);
        if show_tonality {
            ui.add_space(GAP);
            tonality(app, ui);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(4.0);
            // Right to left: the last group first.
            if show_fretlight {
                let mut g = Group::begin(ui);
                g.icon(ui, Icon::Fretlight, Place::Only, false, false, "Fretlight (needs a Fretlight guitar; not in Guitario yet)");
                g.finish(ui);
                ui.add_space(GAP);
            }
            if show_tuner {
                let mut g = Group::begin(ui);
                g.icon(ui, Icon::LineIn, Place::Last, false, false, "Line-in (not in Guitario yet)");
                g.icon(ui, Icon::Tuner, Place::First, false, false, "Guitar tuner (not in Guitario yet)");
                g.finish(ui);
                ui.add_space(GAP);
            }
            instrument_views(app, ui);
            if show_audio {
                ui.add_space(GAP);
                let mut g = Group::begin(ui);
                g.icon(ui, Icon::AudioTrack, Place::Only, false, false, "Audio track (not in Guitario yet)");
                g.finish(ui);
            }
        });
    });
}

fn left(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let mut g = Group::begin(ui);
    let home = g.icon(ui, Icon::Home, Place::Only, false, true, "Home");
    g.finish(ui);
    egui::Popup::menu(&home).show(|ui| {
        if ui.button("New").clicked() {
            ui.close();
            app.new_document();
        }
        if ui.button("Open…").clicked() {
            ui.close();
            app.open_dialog();
        }
        let recent = app.recent_files();
        if !recent.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Recent files").small().color(tokens().text_dim));
            for path in recent.into_iter().take(10) {
                let name = std::path::Path::new(&path).file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
                if ui.button(name).on_hover_text(&path).clicked() {
                    ui.close();
                    app.open_path(&path);
                }
            }
        }
    });

    ui.add_space(GAP);
    let mut g = Group::begin(ui);
    let pn = &mut app.panels;
    if g.icon(ui, Icon::Palette, Place::First, pn.palette, true, "Show/hide the edition palette").clicked() {
        pn.palette = !pn.palette;
    }
    if g.icon(ui, Icon::GlobalView, Place::Middle, pn.global, true, "Show/hide the global view").clicked() {
        pn.global = !pn.global;
    }
    if g.icon(ui, Icon::Inspector, Place::Last, pn.inspector, true, "Show/hide the inspector").clicked() {
        pn.inspector = !pn.inspector;
    }
    g.finish(ui);

    ui.add_space(GAP);
    let mut g = Group::begin(ui);
    let zoom = app.view.zoom;
    let (resp, step) = g.spinner(ui, "zoom", Some(Icon::Zoom), &format!("{:.0}%", zoom * 100.0), 92.0, false, "Zoom");
    g.finish(ui);
    if step != 0 {
        app.view.zoom = step_in(&ZOOMS, zoom, step);
    }
    egui::Popup::menu(&resp).show(|ui| {
        for z in ZOOMS {
            if ui.selectable_label((app.view.zoom - z).abs() < 1e-4, format!("{:.0}%", z * 100.0)).clicked() {
                app.view.zoom = z;
                ui.close();
            }
        }
    });

    ui.add_space(GAP);
    let mut g = Group::begin(ui);
    let (resp, step) = g.spinner(ui, "layout", Some(Icon::Layouts), "", 44.0, false, "Display mode");
    g.finish(ui);
    const MODES: [(LayoutMode, &str); 3] = [(LayoutMode::Page, "Page"), (LayoutMode::Screen, "Screen"), (LayoutMode::Horizontal, "Horizontal")];
    if step != 0 {
        let i = MODES.iter().position(|(m, _)| *m == app.view.layout).unwrap_or(0);
        let j = if step > 0 { (i + 1).min(MODES.len() - 1) } else { i.saturating_sub(1) };
        if let Some((m, _)) = MODES.get(j) {
            app.ui_action(U::Layout(*m));
        }
    }
    egui::Popup::menu(&resp).show(|ui| {
        for (m, label) in MODES {
            if ui.selectable_label(app.view.layout == m, label).clicked() {
                app.ui_action(U::Layout(m));
                ui.close();
            }
        }
        ui.separator();
        if ui.selectable_label(app.view.multitrack, "Multitrack").clicked() {
            app.ui_action(U::ToggleMultitrack);
            ui.close();
        }
    });

    ui.add_space(GAP);
    let mut g = Group::begin(ui);
    if g.icon(ui, Icon::Undo, Place::First, false, app.session.can_undo(), "Undo (⌘Z)").clicked() {
        app.run("edit.undo", Value::Null);
    }
    if g.icon(ui, Icon::Redo, Place::Last, false, app.session.can_redo(), "Redo (⇧⌘Z)").clicked() {
        app.run("edit.redo", Value::Null);
    }
    g.finish(ui);

    ui.add_space(GAP);
    let mut g = Group::begin(ui);
    if g.icon(ui, Icon::Print, Place::Only, false, true, "Print… (⌘P)").clicked() {
        app.ui_action(U::Print);
    }
    g.finish(ui);
}

/// First bar, previous bar, play/stop, next bar, last bar: bare icons, as in Guitar Pro.
fn transport(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    let c = app.session.cursor();
    let last = app.session.score().headers.len().saturating_sub(1);
    let playing = app.transport.playing;
    let bare = |ui: &mut egui::Ui, icon: Icon, size: f32, enabled: bool, tip: &str| {
        let (rect, resp) = ui.allocate_exact_size(vec2(SEG, H + 4.0), if enabled { Sense::click() } else { Sense::hover() });
        let color = if !enabled {
            t.text_dim.gamma_multiply(0.45)
        } else if resp.hovered() {
            Color32::WHITE
        } else {
            t.text
        };
        draw(ui.painter(), Rect::from_center_size(rect.center(), vec2(size, size)), icon, color);
        resp.on_hover_text(tip)
    };
    if bare(ui, Icon::First, 18.0, true, "First bar").clicked() {
        app.run("cursor.set", json!({"measure": 0, "beat": 0}));
    }
    if bare(ui, Icon::Prev, 18.0, c.measure > 0, "Previous bar").clicked() {
        app.run("cursor.set", json!({"measure": c.measure.saturating_sub(1), "beat": 0}));
    }
    let play = bare(ui, if playing { Icon::Stop } else { Icon::Play }, 30.0, true, if playing { "Stop (Space)" } else { "Play (Space)" });
    if play.clicked() {
        app.toggle_play();
    }
    if bare(ui, Icon::Next, 18.0, c.measure < last, "Next bar").clicked() {
        app.run("cursor.set", json!({"measure": (c.measure + 1).min(last), "beat": 0}));
    }
    if bare(ui, Icon::Last, 18.0, true, "Last bar").clicked() {
        app.run("cursor.set", json!({"measure": last, "beat": 0}));
    }
}

fn loop_speed(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let mut g = Group::begin(ui);
    let tr = &mut app.transport;
    if g.icon_w(ui, Icon::Loop, 38.0, Place::First, tr.looping, true, "Loop").clicked() {
        tr.looping = !tr.looping;
    }
    let changed = (tr.speed - 1.0).abs() > 1e-4;
    let (resp, step) = g.spinner(ui, "speed", Some(Icon::Quarter), &format!("{:.0}%", tr.speed * 100.0), 82.0, changed, "Relative speed");
    g.finish(ui);
    if step != 0 {
        tr.speed = step_in(&SPEEDS, tr.speed, step);
    }
    egui::Popup::menu(&resp).show(|ui| {
        for v in SPEEDS {
            if ui.selectable_label((app.transport.speed - v).abs() < 1e-4, format!("{:.0}%", v * 100.0)).clicked() {
                app.transport.speed = v;
                ui.close();
            }
        }
    });
}

/// Relative tonality: a tuning fork, the semitones, and + / − buttons.
fn tonality(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    let mut g = Group::begin(ui);
    let semis = app.transport.transpose;
    let tip = "Relative tonality: playback sounds this many semitones higher or lower; the score is unchanged. Click to reset.";
    let (rect, resp) = ui.allocate_exact_size(vec2(62.0, H), Sense::click());
    g.add(rect);
    let resp = resp.on_hover_text(tip);
    let on = semis != 0;
    let ink = if on { t.accent } else { t.text };
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, CornerRadius { nw: RADIUS, sw: RADIUS, ne: 0, se: 0 }, t.control_hover);
    }
    draw(p, Rect::from_center_size(pos2(rect.left() + 15.0, rect.center().y), vec2(17.0, 17.0)), Icon::TuningFork, ink);
    let label = if semis > 0 { format!("+{semis}") } else { semis.to_string() };
    p.text(
        pos2(rect.left() + 44.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        if on { t.accent } else { t.text_dim },
    );
    if resp.clicked() {
        app.transport.transpose = 0;
    }
    let (r, _) = ui.allocate_exact_size(vec2(22.0, H), Sense::hover());
    g.add(r);
    let up =
        ui.interact(Rect::from_min_max(r.min, pos2(r.right(), r.center().y)), ui.id().with("tonality-up"), Sense::click()).on_hover_text("A semitone higher");
    let down =
        ui.interact(Rect::from_min_max(pos2(r.left(), r.center().y), r.max), ui.id().with("tonality-down"), Sense::click()).on_hover_text("A semitone lower");
    let p = ui.painter();
    p.line_segment([pos2(r.left() + 3.0, r.center().y), pos2(r.right() - 3.0, r.center().y)], Stroke::new(1.0, t.border));
    for (resp, text, y) in [(&up, "+", r.top() + H * 0.27), (&down, "−", r.bottom() - H * 0.27)] {
        p.text(
            pos2(r.center().x, y),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(13.0),
            if resp.hovered() { Color32::WHITE } else { t.text },
        );
    }
    if up.clicked() {
        app.transport.transpose = (semis + 1).min(12);
    }
    if down.clicked() {
        app.transport.transpose = (semis - 1).max(-12);
    }
    g.finish(ui);
}

/// Fretboard, keyboard and drum views. Fretboard and drums share the bottom panel, which shows
/// the drum pad on a drum track (as View › Instrument View does).
fn instrument_views(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let drums = app.session.score().tracks.get(app.session.cursor().track).is_some_and(guitario_score::Track::is_drums);
    let mut g = Group::begin(ui);
    // Right to left.
    let pn = &mut app.panels;
    if g.icon(ui, Icon::Drums, Place::Last, pn.fretboard && drums, drums, "Drum view").clicked() {
        pn.fretboard = !pn.fretboard;
    }
    if g.icon(ui, Icon::Keyboard, Place::Middle, pn.keyboard, true, "Keyboard view").clicked() {
        pn.keyboard = !pn.keyboard;
    }
    if g.icon(ui, Icon::Fretboard, Place::First, pn.fretboard && !drums, !drums, "Fretboard view").clicked() {
        pn.fretboard = !pn.fretboard;
    }
    g.finish(ui);
}

/// What the LCD reads from the score at the cursor (or the playing beat).
pub(crate) struct LcdInfo {
    pub track: usize,
    pub name: String,
    pub icon: Icon,
    pub bar: usize,
    pub bars: usize,
    /// The voice's written duration and the bar's, in beats of the time signature.
    pub filled: f64,
    pub capacity: f64,
    pub now: f64,
    pub total: f64,
    pub feel: TripletFeel,
    pub tempo: u16,
    pub signature: (u8, u8),
    /// The note under the cursor: its letter and octave.
    pub note: Option<(String, i32)>,
}

impl LcdInfo {
    pub(crate) fn read(score: &Score, cursor: guitario_engine::Cursor, play: Option<(f64, usize, usize)>) -> LcdInfo {
        let mut c = cursor;
        if let Some((_, m, b)) = play {
            c.measure = m;
            c.beat = b;
        }
        let track = score.tracks.get(c.track);
        let header = score.headers.get(c.measure);
        let voice = track.and_then(|tr| tr.measures.get(c.measure)).and_then(|m| m.voices.get(c.voice));
        let beat_ticks: u32 = voice.map_or(0, |v| v.beats.iter().take(c.beat).map(|b| b.duration.ticks()).sum());
        let (now, total) = song_times(score, c.measure, beat_ticks);
        let signature = header.map_or((4, 4), |h| (h.time_signature.numerator, h.time_signature.denominator));
        let unit = f64::from(4 * guitario_theory::TICKS_PER_QUARTER) / f64::from(signature.1.max(1));
        let filled = f64::from(voice.map_or(0, guitario_score::Voice::ticks)) / unit;
        let capacity = f64::from(header.map_or(0, guitario_score::MeasureHeader::capacity)) / unit;
        let icon = match track {
            Some(t) if t.is_drums() => Icon::Drums,
            Some(t) if (32..=39).contains(&t.program) => Icon::Bass,
            _ => Icon::Guitar,
        };
        let note = track.filter(|t| !t.is_drums()).and_then(|t| {
            let beat = voice?.beats.get(c.beat)?;
            let n = beat.notes.iter().find(|n| usize::from(n.string) == c.string)?;
            let pitch = t.tuning.pitch(usize::from(n.string), n.fret, t.capo)?;
            Some((guitario_theory::note_name(pitch.0).to_string(), i32::from(pitch.0 / 12) - 1))
        });
        LcdInfo {
            track: c.track,
            name: track.map_or_else(String::new, |t| format!("{}. {}", c.track + 1, t.name)),
            icon,
            bar: c.measure + 1,
            bars: score.headers.len(),
            filled,
            capacity,
            now: play.map_or(now, |p| p.0.max(0.0)),
            total,
            feel: header.map_or(TripletFeel::None, |h| h.triplet_feel),
            tempo: score.tempo_at(c.measure),
            signature,
            note,
        }
    }

    /// The bar check: complete, short of its duration, or over it.
    pub(crate) fn check(&self) -> Check {
        if (self.filled - self.capacity).abs() < 1e-6 {
            Check::Complete
        } else if self.filled < self.capacity {
            Check::Short
        } else {
            Check::Over
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Check {
    Complete,
    Short,
    Over,
}

fn feel_label(f: TripletFeel) -> (&'static str, &'static str) {
    match f {
        TripletFeel::None => ("♫=♫", "No triplet feel"),
        TripletFeel::Triplet8th => ("♫=³♫", "Triplet feel: eighths"),
        TripletFeel::Triplet16th => ("♬=³♬", "Triplet feel: sixteenths"),
        TripletFeel::Dotted8th => ("♫=♪.♬", "Dotted feel: eighths"),
        TripletFeel::Dotted16th => ("♬=♬.", "Dotted feel: sixteenths"),
        TripletFeel::Scottish8th => ("♫=♬♪.", "Scottish feel: eighths"),
        TripletFeel::Scottish16th => ("♬=.♬", "Scottish feel: sixteenths"),
    }
}

/// Guitar Pro's LCD: the track (colour, icon, number and name), count-in, metronome and their
/// settings, the note under the cursor; below, bar, bar check, time, triplet feel, tempo and
/// time signature, over a playback progress line.
fn lcd(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    let play = app.transport.beat.filter(|_| app.transport.playing).map(|(m, bt)| (app.transport.position, m, bt));
    let info = LcdInfo::read(app.session.score(), app.session.cursor(), play);
    let (rect, _) = ui.allocate_exact_size(vec2(LCD_W, LCD_H), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(rect, 4.0, t.lcd_bg);
    let line = Stroke::new(1.0, Color32::from_gray(48));
    let row = rect.top() + 26.0;
    p.line_segment([pos2(rect.left() + 6.0, row), pos2(rect.right(), row)], line);
    // Track colour strip.
    p.rect_filled(Rect::from_min_max(rect.min, pos2(rect.left() + 5.0, rect.bottom())), CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 }, track_color(info.track));

    // Row 1: the note box at the right, the three toggles before it, the track name in the rest.
    let note_w = 54.0;
    let note_box = Rect::from_min_max(pos2(rect.right() - note_w, rect.top()), pos2(rect.right(), row));
    let toggles = Rect::from_min_max(pos2(note_box.left() - 84.0, rect.top()), pos2(note_box.left(), row));
    for x in [note_box.left(), toggles.left()] {
        p.line_segment([pos2(x, rect.top()), pos2(x, row)], line);
    }
    let name_rect = Rect::from_min_max(pos2(rect.left() + 8.0, rect.top()), pos2(toggles.left() - 4.0, row));
    let name_resp = ui.interact(name_rect, ui.id().with("lcd-track"), Sense::click()).on_hover_text("Choose the track");
    draw(&p, Rect::from_center_size(pos2(name_rect.left() + 11.0, name_rect.center().y), vec2(17.0, 17.0)), info.icon, t.lcd_text);
    let name_clip = Rect::from_min_max(pos2(name_rect.left() + 24.0, name_rect.top()), name_rect.max);
    p.with_clip_rect(name_clip).text(
        pos2(name_clip.left(), name_clip.center().y + 1.0),
        egui::Align2::LEFT_CENTER,
        &info.name,
        crate::fonts::sans_bold(16.0),
        if name_resp.hovered() { Color32::WHITE } else { t.lcd_text },
    );
    egui::Popup::menu(&name_resp).show(|ui| {
        let names: Vec<String> = app.session.score().tracks.iter().enumerate().map(|(i, tr)| format!("{}. {}", i + 1, tr.name)).collect();
        for (i, n) in names.into_iter().enumerate() {
            if ui.selectable_label(i == info.track, n).clicked() {
                app.run("cursor.set", json!({"track": i}));
                ui.close();
            }
        }
    });

    let tr = &mut app.transport;
    let slot = |i: f32| Rect::from_center_size(pos2(toggles.left() + 14.0 + i * 28.0, toggles.center().y), vec2(26.0, 24.0));
    let toggle = |ui: &mut egui::Ui, r: Rect, icon: Icon, on: bool, id: &str, tip: &str| {
        let resp = ui.interact(r, ui.id().with(id), Sense::click()).on_hover_text(tip);
        let color = if on {
            t.accent
        } else if resp.hovered() {
            Color32::WHITE
        } else {
            t.lcd_text
        };
        draw(&p, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), icon, color);
        resp
    };
    if toggle(ui, slot(0.0), Icon::CountIn, tr.count_in, "lcd-countin", "Count-in").clicked() {
        tr.count_in = !tr.count_in;
    }
    if toggle(ui, slot(1.0), Icon::Metronome, tr.metronome, "lcd-metronome", "Metronome").clicked() {
        tr.metronome = !tr.metronome;
    }
    let more = toggle(ui, slot(2.0), Icon::More, false, "lcd-more", "Metronome and count-in settings");
    egui::Popup::menu(&more).show(|ui| {
        ui.checkbox(&mut app.transport.metronome, "Metronome");
        ui.checkbox(&mut app.transport.count_in, "Count-in");
    });

    if let Some((letter, octave)) = &info.note {
        let dim = Color32::from_rgb(0x5c, 0x64, 0x72);
        let g = p.layout_no_wrap(letter.clone(), egui::FontId::proportional(22.0), dim);
        let o = p.layout_no_wrap(octave.to_string(), egui::FontId::proportional(12.0), dim);
        let w = g.size().x + 1.0 + o.size().x;
        let x = note_box.center().x - w / 2.0;
        let base = note_box.center().y + 1.0;
        p.galley(pos2(x, base - g.size().y / 2.0), g.clone(), dim);
        p.galley(pos2(x + g.size().x + 1.0, base + g.size().y / 2.0 - o.size().y - 1.0), o, dim);
    }

    // Row 2.
    let y = (row + rect.bottom() - 2.0) / 2.0;
    let small = egui::FontId::proportional(12.5);
    p.text(pos2(rect.left() + 30.0, y), egui::Align2::CENTER_CENTER, format!("{}/{}", info.bar, info.bars), small.clone(), t.lcd_text);
    let check = info.check();
    let (mark, mark_col, tip) = match check {
        Check::Complete => ("✔", Color32::from_rgb(0x3c, 0xb9, 0x4a), "The bar is complete"),
        Check::Short => ("!", Color32::from_rgb(0xf0, 0xa0, 0x30), "The bar is incomplete"),
        Check::Over => ("!", Color32::from_rgb(0xe0, 0x40, 0x40), "The bar is too long"),
    };
    let check_rect = Rect::from_center_size(pos2(rect.left() + 104.0, y), vec2(80.0, 16.0));
    let c0 = pos2(check_rect.left() + 7.0, y);
    p.circle_filled(c0, 6.0, mark_col);
    p.text(c0, egui::Align2::CENTER_CENTER, mark, egui::FontId::proportional(if check == Check::Complete { 8.5 } else { 10.0 }), t.lcd_bg);
    p.text(pos2(c0.x + 9.0, y), egui::Align2::LEFT_CENTER, format!("{}:{}", one_decimal(info.filled), one_decimal(info.capacity)), small.clone(), t.lcd_text);
    let _ = ui
        .interact(check_rect, ui.id().with("lcd-check"), Sense::hover())
        .on_hover_text(format!("{tip} (voice {}): written and expected duration in beats", app.session.cursor().voice + 1));
    p.text(pos2(rect.left() + 214.0, y), egui::Align2::CENTER_CENTER, format!("{} / {}", clock(info.now), clock(info.total)), small.clone(), t.lcd_text);
    let (feel, feel_tip) = feel_label(info.feel);
    let feel_rect = Rect::from_center_size(pos2(rect.right() - 118.0, y), vec2(44.0, 16.0));
    p.text(feel_rect.center(), egui::Align2::CENTER_CENTER, feel, egui::FontId::proportional(13.0), t.lcd_text);
    let _ = ui.interact(feel_rect, ui.id().with("lcd-feel"), Sense::hover()).on_hover_text(feel_tip);
    let tempo_rect = Rect::from_center_size(pos2(rect.right() - 62.0, y), vec2(56.0, 16.0));
    let tempo = ui.interact(tempo_rect, ui.id().with("lcd-tempo"), Sense::click()).on_hover_text("Tempo (click to edit the tempo automation)");
    p.text(tempo_rect.center(), egui::Align2::CENTER_CENTER, format!("♩= {}", info.tempo), small, if tempo.hovered() { Color32::WHITE } else { t.lcd_text });
    if tempo.clicked() {
        app.ui_action(U::TempoAutomation);
    }
    let sig_x = rect.right() - 16.0;
    let sig = egui::FontId::proportional(8.5);
    p.text(pos2(sig_x, y - 4.0), egui::Align2::CENTER_CENTER, info.signature.0.to_string(), sig.clone(), t.lcd_text);
    p.text(pos2(sig_x, y + 4.0), egui::Align2::CENTER_CENTER, info.signature.1.to_string(), sig, t.lcd_text);

    // Playback progress.
    let frac = if info.total > 0.0 { (info.now / info.total).clamp(0.0, 1.0) as f32 } else { 0.0 };
    if frac > 0.0 {
        let bar = Rect::from_min_max(pos2(rect.left() + 6.0, rect.bottom() - 3.0), pos2(rect.left() + 6.0 + (rect.width() - 12.0) * frac, rect.bottom() - 1.0));
        p.rect_filled(bar, 1.0, t.accent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn clock_pads_and_survives_bad_values() {
        assert_eq!(clock(105.0), "01:45");
        assert_eq!(clock(-3.0), "00:00");
        assert_eq!(clock(f64::NAN), "00:00");
        assert_eq!(one_decimal(f64::INFINITY), "0.0");
    }

    #[test]
    fn steps_walk_the_lists_and_stop_at_the_ends() {
        assert_eq!(step_in(&ZOOMS, 1.0, 1), 1.1);
        assert_eq!(step_in(&ZOOMS, 1.0, -1), 0.9);
        assert_eq!(step_in(&ZOOMS, 4.0, 1), 4.0);
        assert_eq!(step_in(&SPEEDS, 0.25, -1), 0.25);
        // A value between steps goes to the neighbour on that side.
        assert_eq!(step_in(&ZOOMS, 1.17, 1), 1.25);
    }

    #[test]
    fn right_groups_drop_out_as_the_window_narrows() {
        assert_eq!(fits(2400.0), (true, true, true, true));
        assert_eq!(fits(900.0), (false, false, false, false));
        let (a, b, c, d) = fits(1700.0);
        // Groups go from the right end first.
        assert!(a >= b && b >= c && c >= d);
    }

    #[test]
    fn lcd_reads_the_bar_check_and_note() {
        let mut s = Session::new();
        let lcd = LcdInfo::read(s.score(), s.cursor(), None);
        assert_eq!((lcd.bar, lcd.capacity, lcd.filled), (1, 4.0, 4.0), "a new bar holds one whole rest");
        assert_eq!(lcd.check(), Check::Complete);
        assert!(lcd.note.is_none());
        s.run("beat.setDuration", &json!({"value": 4})).unwrap();
        s.run("note.setFret", &json!({"fret": 0})).unwrap();
        let lcd = LcdInfo::read(s.score(), s.cursor(), None);
        assert_eq!(lcd.filled, 1.0);
        assert_eq!(lcd.check(), Check::Short);
        assert_eq!(lcd.note, Some(("E".to_string(), 4)), "open high E");
        assert_eq!(lcd.icon, Icon::Guitar);
        assert_eq!(lcd.name, format!("1. {}", s.score().tracks[0].name));
        // A tuplet-free overfull voice reads as too long.
        let mut score: Score = (**s.score()).clone();
        let beat = score.tracks[0].measures[0].voices[0].beats[0].clone();
        let mut long = beat.clone();
        long.duration = guitario_theory::Duration::WHOLE;
        score.tracks[0].measures[0].voices[0].beats = vec![long, beat];
        let lcd = LcdInfo::read(&score, s.cursor(), None);
        assert_eq!(lcd.check(), Check::Over);
        assert_eq!(lcd.filled, 5.0);
    }

    #[test]
    fn lcd_follows_playback_and_hostile_cursors() {
        let s = Session::new();
        let mut c = s.cursor();
        c.track = 99;
        c.measure = 99;
        let lcd = LcdInfo::read(s.score(), c, None);
        assert_eq!(lcd.name, "");
        assert!(lcd.note.is_none());
        let lcd = LcdInfo::read(s.score(), s.cursor(), Some((1.5, 0, 0)));
        assert_eq!(lcd.now, 1.5);
    }
}
