//! Guitar Pro 8's Add Track window: the instrument type, its family and instrument, then the new
//! track's information, notation, tuning and sound. Track › Add Track and the global view's +
//! open it; a new file opens it to set up its first track, with Skip to keep the default guitar.

use crate::GuitarioApp;
use crate::icons::{Icon, draw};
use crate::inspector::{Glyph, button_group};
use crate::theme::tokens;
use egui::{Color32, Rect, Sense, Stroke, pos2, vec2};
use guitario_theory::Tuning;
use serde_json::json;

/// An instrument the window offers: its name in the list, the new track's name and short name, its
/// General MIDI program and the tunings it takes (the first is the default).
pub(crate) struct Instrument {
    pub name: &'static str,
    pub track: &'static str,
    pub short: &'static str,
    pub program: u8,
    pub tunings: &'static [&'static str],
    pub drums: bool,
}

const fn inst(name: &'static str, track: &'static str, short: &'static str, program: u8, tunings: &'static [&'static str]) -> Instrument {
    Instrument { name, track, short, program, tunings, drums: false }
}

const GUITAR: &[&str] = &["Standard", "Drop D", "D Standard", "Drop C", "Drop B", "Half step down", "DADGAD", "Open G"];

/// A family of instruments; an empty one is shown dimmed (Guitario cannot write it yet).
pub(crate) struct Family {
    pub name: &'static str,
    pub instruments: &'static [Instrument],
}

pub(crate) const STRINGED: [Family; 4] = [
    Family {
        name: "Acoustic Guitar",
        instruments: &[
            inst("Steel", "Steel Guitar", "s.guit.", 25, GUITAR),
            inst("12-String", "12-String Guitar", "12-str.gt.", 25, GUITAR),
            inst("Nylon", "Classical Guitar", "cl.guit.", 24, GUITAR),
            inst("Resonator", "Resonator Guitar", "res.gt.", 25, &["Open G", "Standard", "DADGAD"]),
        ],
    },
    Family {
        name: "Electric Guitar",
        instruments: &[
            inst("Clean", "Clean Guitar", "el.guit.", 27, GUITAR),
            inst("Jazz", "Jazz Guitar", "jz.guit.", 26, GUITAR),
            inst("Overdrive", "Overdrive Guitar", "od.guit.", 29, GUITAR),
            inst("Distortion", "Distortion Guitar", "dist.guit.", 30, GUITAR),
            inst("7-String", "7-String Guitar", "7-str.gt.", 30, &["7 string", "7 String Drop A"]),
            inst("8-String", "8-String Guitar", "8-str.gt.", 30, &["8 string"]),
        ],
    },
    Family {
        name: "Bass",
        instruments: &[
            inst("4-String", "Bass", "bass", 33, &["Bass (4 string)"]),
            inst("5-String", "5-String Bass", "5-str.bs.", 33, &["Bass (5 string)"]),
            inst("6-String", "6-String Bass", "6-str.bs.", 33, &["Bass (6 string)"]),
            inst("Fretless", "Fretless Bass", "frtl.bs.", 35, &["Bass (4 string)", "Bass (5 string)"]),
            inst("Slap", "Slap Bass", "slap.bs.", 36, &["Bass (4 string)", "Bass (5 string)"]),
        ],
    },
    Family {
        name: "Other",
        instruments: &[
            inst("Banjo", "Banjo", "bjo.", 105, &["Banjo (open G)"]),
            inst("Mandolin", "Mandolin", "mand.", 25, &["Mandolin"]),
            inst("Ukulele", "Ukulele", "uke.", 24, &["Ukulele"]),
            inst("Sitar", "Sitar", "sit.", 104, &["Standard"]),
        ],
    },
];

pub(crate) const ORCHESTRA: [Family; 4] = [
    Family {
        name: "Strings",
        instruments: &[
            inst("Violin", "Violin", "vln.", 40, &["Violin"]),
            inst("Viola", "Viola", "vla.", 41, &["Viola"]),
            inst("Cello", "Cello", "vc.", 42, &["Cello"]),
            inst("Double Bass", "Double Bass", "cb.", 43, &["Double bass"]),
        ],
    },
    Family { name: "Winds", instruments: &[] },
    Family { name: "Brass", instruments: &[] },
    Family { name: "Keyboards", instruments: &[] },
];

pub(crate) const DRUMS: [Family; 1] =
    [Family { name: "Drum Kit", instruments: &[Instrument { name: "Drum Kit", track: "Drums", short: "drms", program: 0, tunings: &[], drums: true }] }];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Kind {
    #[default]
    Stringed,
    Orchestra,
    Drums,
    Midi,
}

impl Kind {
    fn families(self) -> &'static [Family] {
        match self {
            Kind::Stringed => &STRINGED,
            Kind::Orchestra => &ORCHESTRA,
            Kind::Drums => &DRUMS,
            Kind::Midi => &[],
        }
    }
}

#[derive(Default)]
pub struct AddTrack {
    pub open: bool,
    /// Setting up a new file's first track: Create replaces it, Skip keeps it.
    replace: Option<usize>,
    /// The index the new track will have, for its default colour.
    next: usize,
    kind: Kind,
    family: usize,
    instrument: usize,
    name: String,
    short: String,
    color: Option<[u8; 3]>,
    standard: bool,
    tab: bool,
    tuning: usize,
    program: u8,
}

impl AddTrack {
    fn current(&self) -> Option<&'static Instrument> {
        self.kind.families().get(self.family)?.instruments.get(self.instrument)
    }

    /// Takes the chosen instrument's defaults.
    fn choose(&mut self, kind: Kind, family: usize, instrument: usize) {
        (self.kind, self.family, self.instrument) = (kind, family, instrument);
        if let Some(i) = self.current() {
            self.name = i.track.to_string();
            self.short = i.short.to_string();
            self.program = i.program;
            self.tuning = 0;
            self.standard = true;
            self.tab = !i.drums;
        }
    }

    /// The `track.create` parameters for what the window shows.
    fn params(&self) -> Option<serde_json::Value> {
        let i = self.current()?;
        let mut p = serde_json::Map::new();
        p.insert("name".into(), json!(self.name));
        p.insert("shortName".into(), json!(self.short));
        if i.drums {
            p.insert("kind".into(), json!("drums"));
        } else {
            p.insert("tuning".into(), json!(i.tunings.get(self.tuning).copied().unwrap_or("Standard")));
            p.insert("program".into(), json!(self.program));
            p.insert("standard".into(), json!(self.standard));
            p.insert("tab".into(), json!(self.tab));
        }
        if let Some([r, g, b]) = self.color {
            p.insert("color".into(), json!(format!("#{r:02x}{g:02x}{b:02x}")));
        }
        if let Some(index) = self.replace {
            p.insert("replace".into(), json!(index));
        }
        Some(serde_json::Value::Object(p))
    }
}

impl GuitarioApp {
    /// Opens the Add Track window; `replace` sets up that track instead of adding one (a new file's
    /// first track).
    pub fn open_add_track(&mut self, replace: Option<usize>) {
        let keep_kind = self.add_track.kind;
        let next = replace.unwrap_or(self.session.score().tracks.len());
        self.add_track = AddTrack { open: true, replace, next, ..AddTrack::default() };
        let kind = if keep_kind == Kind::Midi { Kind::Stringed } else { keep_kind };
        self.add_track.choose(kind, 0, 0);
    }

    pub(crate) fn add_track_window(&mut self, ctx: &egui::Context) {
        if !self.add_track.open {
            return;
        }
        let t = tokens();
        let mut open = true;
        let mut action: Option<bool> = None; // Some(true) create, Some(false) close
        egui::Window::new("Add Track")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(560.0, 0.0))
            .frame(egui::Frame::NONE.fill(t.panel).corner_radius(8).stroke(Stroke::new(1.0, t.border)).inner_margin(egui::Margin::same(14)))
            .open(&mut open)
            .show(ctx, |ui| {
                let w = &mut self.add_track;
                ui.label(egui::RichText::new("ADD TRACK").font(crate::fonts::sans_bold(20.0)));
                ui.add_space(8.0);
                // The four instrument types.
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::hover());
                let kw = row.width() / 4.0;
                for (k, (kind, name)) in
                    [(Kind::Stringed, "Stringed"), (Kind::Orchestra, "Orchestra"), (Kind::Drums, "Drums"), (Kind::Midi, "MIDI")].into_iter().enumerate()
                {
                    let r = Rect::from_min_size(row.min + vec2(k as f32 * kw, 0.0), vec2(kw, row.height()));
                    let enabled = kind != Kind::Midi;
                    let resp = ui.interact(r, ui.id().with(("kind", k)), if enabled { Sense::click() } else { Sense::hover() });
                    let corners = egui::CornerRadius {
                        nw: if k == 0 { 6 } else { 0 },
                        sw: if k == 0 { 6 } else { 0 },
                        ne: if k == 3 { 6 } else { 0 },
                        se: if k == 3 { 6 } else { 0 },
                    };
                    let on = w.kind == kind;
                    let p = ui.painter();
                    p.rect_filled(
                        r,
                        corners,
                        if on {
                            t.accent
                        } else if resp.hovered() && enabled {
                            t.control_hover
                        } else {
                            t.control
                        },
                    );
                    if k > 0 {
                        p.line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0, t.panel));
                    }
                    let ink = if !enabled {
                        t.text_dim.gamma_multiply(0.6)
                    } else if on {
                        Color32::WHITE
                    } else {
                        t.text
                    };
                    let icon = Rect::from_center_size(pos2(r.center().x, r.top() + 20.0), vec2(22.0, 22.0));
                    match kind {
                        Kind::Stringed => draw(p, icon, Icon::Guitar, ink),
                        Kind::Orchestra => violin(p, icon, ink),
                        Kind::Drums => draw(p, icon, Icon::Drums, ink),
                        Kind::Midi => {
                            p.text(icon.center(), egui::Align2::CENTER_CENTER, "MIDI", crate::fonts::sans_bold(12.0), ink);
                        }
                    }
                    p.text(pos2(r.center().x, r.bottom() - 14.0), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(14.0), ink);
                    let resp = if enabled { resp } else { resp.on_hover_text("Tracks for any General MIDI instrument (not in Guitario yet)") };
                    if resp.clicked() && !on {
                        w.choose(kind, 0, 0);
                    }
                }
                ui.add_space(8.0);
                // Families at the left, their instruments at the right.
                let (lists, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), Sense::hover());
                let half = (lists.width() - 10.0) / 2.0;
                let left = Rect::from_min_size(lists.min, vec2(half, lists.height()));
                let right = Rect::from_min_size(pos2(lists.right() - half, lists.top()), vec2(half, lists.height()));
                ui.painter().rect_filled(left, 4.0, t.chrome_dark);
                ui.painter().rect_filled(right, 4.0, t.chrome_dark);
                let families = w.kind.families();
                let mut pick: Option<(usize, usize)> = None;
                let row_h = 24.0;
                for (fi, f) in families.iter().enumerate() {
                    let r = Rect::from_min_size(left.min + vec2(2.0, 2.0 + fi as f32 * row_h), vec2(left.width() - 4.0, row_h - 2.0));
                    let enabled = !f.instruments.is_empty();
                    let resp = ui.interact(r, ui.id().with(("family", fi)), if enabled { Sense::click() } else { Sense::hover() });
                    list_row(ui, r, f.name, fi == w.family, enabled, resp.hovered());
                    let resp = if enabled { resp } else { resp.on_hover_text(format!("{} (not in Guitario yet)", f.name)) };
                    if resp.clicked() {
                        pick = Some((fi, 0));
                    }
                }
                if let Some(f) = families.get(w.family) {
                    for (ii, i) in f.instruments.iter().enumerate() {
                        let r = Rect::from_min_size(right.min + vec2(2.0, 2.0 + ii as f32 * row_h), vec2(right.width() - 4.0, row_h - 2.0));
                        let resp = ui.interact(r, ui.id().with(("instrument", w.family, ii)), Sense::click());
                        list_row(ui, r, i.name, ii == w.instrument, true, resp.hovered());
                        if resp.double_clicked() {
                            action = Some(true);
                        }
                        if resp.clicked() {
                            pick = Some((w.family, ii));
                        }
                    }
                }
                if let Some((f, i)) = pick
                    && (f, i) != (w.family, w.instrument)
                {
                    let kind = w.kind;
                    w.choose(kind, f, i);
                }
                ui.add_space(12.0);
                let Some(inst) = w.current() else { return };
                ui.label(egui::RichText::new(format!("{} SETTINGS", inst.name.to_uppercase())).font(crate::fonts::sans_bold(17.0)));
                ui.add_space(8.0);
                let label = |ui: &mut egui::Ui, text: &str, enabled: bool| {
                    ui.allocate_ui_with_layout(vec2(96.0, 26.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(if enabled { egui::RichText::new(text) } else { egui::RichText::new(text).color(t.text_dim.gamma_multiply(0.6)) })
                    });
                };
                ui.horizontal(|ui| {
                    label(ui, "Information:", true);
                    ui.spacing_mut().item_spacing.x = 3.0;
                    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                    ui.painter().rect_filled(r, egui::CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 }, t.control);
                    let rgb = w.color.unwrap_or_else(|| {
                        guitario_score::TRACK_COLORS.get(w.next % guitario_score::TRACK_COLORS.len()).copied().unwrap_or([0xe0, 0x4f, 0x4f])
                    });
                    ui.painter().rect_filled(r.shrink(6.0), 2.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                    egui::Popup::menu(&resp.on_hover_text("Track colour")).show(|ui| {
                        egui::Grid::new("add-track-swatches").spacing([3.0, 3.0]).show(ui, |ui| {
                            for (k, c) in guitario_score::TRACK_COLORS.iter().enumerate() {
                                let (r, sr) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                                ui.painter().rect_filled(r, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
                                if sr.clicked() {
                                    w.color = Some(*c);
                                    ui.close();
                                }
                                if k % 4 == 3 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                    let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                    ui.painter().rect_filled(r, egui::CornerRadius { nw: 0, sw: 0, ne: 5, se: 5 }, t.control);
                    draw(
                        ui.painter(),
                        r.shrink(5.0),
                        if inst.drums {
                            Icon::Drums
                        } else if inst.program >= 32 && inst.program <= 39 {
                            Icon::Bass
                        } else {
                            Icon::Guitar
                        },
                        t.text,
                    );
                    let name_w = (ui.available_width() - 90.0).max(80.0);
                    ui.add(egui::TextEdit::singleline(&mut w.name).desired_width(name_w).frame(crate::theme::field_frame()));
                    ui.add(egui::TextEdit::singleline(&mut w.short).desired_width(80.0).frame(crate::theme::field_frame()));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    label(ui, "Notation:", true);
                    let later = " (not in Guitario yet)";
                    let (slash, numbered) = (format!("Slash notation{later}"), format!("Numbered notation (jianpu){later}"));
                    let buttons = [
                        (Glyph::Tab, w.tab && !inst.drums, !inst.drums, "Tablature"),
                        (Glyph::Standard, w.standard || inst.drums, !inst.drums, "Standard notation"),
                        (Glyph::Slash, false, false, slash.as_str()),
                        (Glyph::Numbered, false, false, numbered.as_str()),
                    ];
                    match button_group(ui, "add-track-notation", &buttons) {
                        // One of the two always stays on.
                        Some(0) if w.standard || !w.tab => w.tab = !w.tab,
                        Some(1) if w.tab || !w.standard => w.standard = !w.standard,
                        _ => {}
                    }
                    ui.add_space(10.0);
                    let grand = format!("Two staves (grand staff){later}");
                    button_group(ui, "add-track-staves", &[(Glyph::SingleStaff, true, true, "One staff"), (Glyph::GrandStaff, false, false, grand.as_str())]);
                });
                ui.add_space(4.0);
                if !inst.drums {
                    ui.horizontal(|ui| {
                        label(ui, "Upper staff:", true);
                        let tuning = inst.tunings.get(w.tuning).and_then(|n| Tuning::preset(n));
                        let strings = tuning.as_ref().map_or(6, Tuning::string_count);
                        ui.add_enabled(false, egui::Button::new(format!("{strings} strings")).min_size(vec2(90.0, 26.0)));
                        ui.add_enabled(false, egui::Button::new(inst.name).min_size(vec2(110.0, 26.0)));
                        let shown = tuning.as_ref().map_or_else(
                            || "Standard".to_string(),
                            |t| {
                                let notes: Vec<&str> = t.strings.iter().rev().map(|p| guitario_theory::note_name(*p)).collect();
                                format!("{}  {}", t.name, notes.join(" "))
                            },
                        );
                        egui::ComboBox::from_id_salt("add-track-tuning").width(ui.available_width() - 4.0).selected_text(shown).show_ui(ui, |ui| {
                            for (k, name) in inst.tunings.iter().enumerate() {
                                if ui.selectable_label(k == w.tuning, *name).clicked() {
                                    w.tuning = k;
                                }
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        label(ui, "Lower staff:", false);
                        ui.add_enabled(false, egui::Button::new("Two staves (not in Guitario yet)").min_size(vec2(ui.available_width() - 4.0, 26.0)));
                    });
                    ui.horizontal(|ui| {
                        label(ui, "Sound:", true);
                        egui::ComboBox::from_id_salt("add-track-sound").width(220.0).selected_text(crate::inspector::program_name(w.program)).show_ui(
                            ui,
                            |ui| {
                                for (program, name) in crate::inspector::PROGRAMS {
                                    if ui.selectable_label(w.program == program, name).clicked() {
                                        w.program = program;
                                    }
                                }
                            },
                        );
                        ui.add_enabled(false, egui::Button::new("▶")).on_disabled_hover_text("Listen to the sound (not in Guitario yet)");
                    });
                }
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let create = egui::Button::new(egui::RichText::new("Create").color(Color32::WHITE)).fill(t.accent).min_size(vec2(84.0, 28.0));
                        if ui.add_enabled(!w.name.trim().is_empty(), create).clicked() {
                            action = Some(true);
                        }
                        if ui.add(egui::Button::new("Cancel").min_size(vec2(84.0, 28.0))).clicked() {
                            action = Some(false);
                        }
                        if w.replace.is_some() {
                            ui.add_space(20.0);
                            if ui.add(egui::Button::new("Skip").min_size(vec2(70.0, 28.0))).on_hover_text("Keep the guitar track").clicked() {
                                action = Some(false);
                            }
                        }
                    });
                });
            });
        if !open {
            action = Some(false);
        }
        match action {
            Some(true) => {
                if let Some(params) = self.add_track.params()
                    && self.run("track.create", params).is_some()
                {
                    self.add_track.open = false;
                }
            }
            Some(false) => self.add_track.open = false,
            None => {}
        }
    }
}

fn list_row(ui: &egui::Ui, r: Rect, text: &str, on: bool, enabled: bool, hovered: bool) {
    let t = tokens();
    let p = ui.painter();
    if on && enabled {
        p.rect_filled(r, 4.0, t.accent);
    } else if hovered && enabled {
        p.rect_filled(r, 4.0, t.control);
    }
    let ink = if !enabled {
        t.text_dim.gamma_multiply(0.5)
    } else if on {
        Color32::WHITE
    } else {
        t.text
    };
    p.text(pos2(r.left() + 12.0, r.center().y), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(15.0), ink);
}

/// A violin: body, waist, neck and scroll.
fn violin(p: &egui::Painter, r: Rect, c: Color32) {
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    p.add(egui::Shape::ellipse_filled(at(0.5, 0.8), vec2(r.width() * 0.28, r.height() * 0.18), c));
    p.add(egui::Shape::ellipse_filled(at(0.5, 0.52), vec2(r.width() * 0.22, r.height() * 0.14), c));
    p.line_segment([at(0.5, 0.4), at(0.5, 0.05)], Stroke::new(2.2, c));
    p.circle_stroke(at(0.5, 0.06), r.width() * 0.07, Stroke::new(1.4, c));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_instrument_has_tunings_guitario_knows_and_a_valid_track() {
        for families in [&STRINGED[..], &ORCHESTRA[..], &DRUMS[..]] {
            for f in families {
                for i in f.instruments {
                    if !i.drums {
                        assert!(!i.tunings.is_empty(), "{}", i.name);
                    }
                    for t in i.tunings {
                        assert!(Tuning::preset(t).is_some_and(|p| p.name == *t), "{}: {t}", i.name);
                    }
                }
            }
        }
    }

    #[test]
    fn the_window_creates_what_it_shows_in_one_command() {
        let mut s = guitario_engine::Session::new();
        let mut w = AddTrack::default();
        for (kind, families) in [(Kind::Stringed, &STRINGED[..]), (Kind::Orchestra, &ORCHESTRA[..]), (Kind::Drums, &DRUMS[..])] {
            for (fi, f) in families.iter().enumerate() {
                for ii in 0..f.instruments.len() {
                    w.choose(kind, fi, ii);
                    let params = w.params().unwrap_or_default();
                    let before = s.score().tracks.len();
                    s.run("track.create", &params).unwrap_or_else(|e| panic!("{params}: {e}"));
                    assert_eq!(s.score().tracks.len(), before + 1);
                    // Keep under the track limit.
                    s.run("track.remove", &json!(null)).ok();
                }
            }
        }
        // A new file's first track is replaced, not added.
        w.replace = Some(0);
        w.choose(Kind::Drums, 0, 0);
        let mut fresh = guitario_engine::Session::new();
        fresh.run("track.create", &w.params().unwrap_or_default()).unwrap();
        assert!(fresh.score().tracks.len() == 1 && fresh.score().tracks[0].is_drums());
    }
}
