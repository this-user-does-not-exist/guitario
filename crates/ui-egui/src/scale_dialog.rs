//! Tools › Scale Tool…: Guitar Pro 8's Scales window. A scale list filtered by search and tags, the
//! key note and spelling, the scale's notes, steps and intervals on a line, a play button, and
//! "Find Scales from Selection" (`tools.findScales`). The scale chosen is drawn on the fretboard
//! and keyboard views until the window turns it off.

use crate::GuitarioApp;
use crate::theme;
use egui::{Color32, RichText, Sense, Stroke, pos2, vec2};
use guitario_theory::pitch_class_name;
use guitario_theory::scales::{SCALES, Tag};
use serde_json::json;

pub(crate) struct ScaleDialog {
    pub(crate) open: bool,
    pub(crate) search: String,
    /// Tags every listed scale has.
    pub(crate) tags: Vec<Tag>,
    /// Index in [`SCALES`].
    pub(crate) scale: usize,
    pub(crate) root: u8,
    pub(crate) flats: bool,
    /// Show the scale on the fretboard and keyboard views.
    pub(crate) show: bool,
    /// The last "Find Scales from Selection": (key note, scale, percent).
    pub(crate) found: Option<Vec<(u8, usize, f32)>>,
    /// Find only scales on the key note.
    pub(crate) on_key: bool,
}

impl Default for ScaleDialog {
    fn default() -> Self {
        ScaleDialog {
            open: false,
            search: String::new(),
            tags: vec![Tag::Usual, Tag::Scale],
            scale: 0,
            root: 0,
            flats: false,
            show: true,
            found: None,
            on_key: false,
        }
    }
}

impl ScaleDialog {
    /// The scales the search and tags let through, by index.
    pub(crate) fn listed(&self) -> Vec<usize> {
        let words: Vec<String> = self.search.split_whitespace().map(str::to_lowercase).collect();
        SCALES
            .iter()
            .enumerate()
            .filter(|(_, s)| self.tags.iter().all(|t| s.has(*t)))
            .filter(|(_, s)| words.iter().all(|w| s.name.to_lowercase().contains(w.as_str())))
            .map(|(i, _)| i)
            .collect()
    }
}

impl GuitarioApp {
    /// Tools › Scale Tool…: opens on the scale shown on the views, or the key of the cursor's bar.
    pub fn open_scales(&mut self) {
        let c = self.session.cursor();
        let key = self.session.score().headers.get(c.measure).map_or(0, |h| h.key);
        let d = &mut self.scales;
        d.open = true;
        d.flats = key < 0;
        match self.view.scale {
            Some((root, scale, _)) => {
                d.root = root;
                d.scale = scale;
            }
            // The major key the signature names: 0 is C, each sharp a fifth up.
            None => d.root = (i16::from(key) * 7).rem_euclid(12) as u8,
        }
        if d.show {
            self.view.scale = Some((d.root, d.scale, d.flats));
        }
    }

    /// Shows the library scale named `name` on `root` in the open Scales window and on the views.
    pub fn choose_scale(&mut self, root: u8, name: &str) {
        if let Some(i) = SCALES.iter().position(|s| s.name.starts_with(name)) {
            self.scales.root = root % 12;
            self.scales.scale = i;
            self.view.scale = self.scales.show.then_some((self.scales.root, i, self.scales.flats));
        }
    }

    /// Closes the Scales window; the views keep showing the scale.
    pub fn close_scales(&mut self) {
        self.scales.open = false;
    }

    fn find_scales(&mut self) {
        let params = if self.scales.on_key { json!({"limit": 30, "key": self.scales.root}) } else { json!({"limit": 30}) };
        if let Some(r) = self.run("tools.findScales", params) {
            let found = r
                .get("scales")
                .and_then(serde_json::Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|v| {
                            let name = v.get("scale")?.as_str()?;
                            let scale = SCALES.iter().position(|s| s.name == name)?;
                            Some((v.get("root")?.as_u64()? as u8, scale, v.get("percent")?.as_f64()? as f32))
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.scales.found = Some(found);
        }
    }

    /// Plays the scale up an octave from the key note nearest the track's middle, in eighths.
    fn audition_scale(&mut self) {
        let c = self.session.cursor();
        let Some(track) = self.session.score().tracks.get(c.track).filter(|t| !t.is_drums()).cloned() else { return };
        let Some(scale) = SCALES.get(self.scales.scale) else { return };
        let low = track.tuning.strings.iter().copied().min().unwrap_or(40);
        let start = low + 12 + (self.scales.root + 12 - low % 12) % 12;
        let mut score = guitario_score::Score::new();
        let pitches: Vec<u8> = scale.intervals.iter().map(|i| start + i).chain(std::iter::once(start + 12)).collect();
        let eighth = guitario_theory::Duration::EIGHTH;
        let bars = pitches.len().div_ceil(8);
        score.headers = vec![guitario_score::MeasureHeader::new(guitario_theory::TimeSignature::COMMON); bars.max(1)];
        let mut t = guitario_score::Track::new("Scale", track.tuning.clone(), &score.headers);
        t.capo = track.capo;
        t.program = track.program;
        for (bar, chunk) in pitches.chunks(8).enumerate() {
            let beats: Vec<guitario_score::Beat> = chunk
                .iter()
                .filter_map(|p| crate::keyboard::place(&t, *p, c.string))
                .map(|(s, f)| guitario_score::Beat { rest: false, notes: vec![guitario_score::Note::new(s as u8, f)], ..guitario_score::Beat::rest(eighth) })
                .collect();
            if let Some(v) = t.measures.get_mut(bar).and_then(|m| m.voices.first_mut()) {
                let used: u32 = beats.iter().map(|b| b.duration.ticks()).sum();
                v.beats = beats;
                // The bar is filled with eighth rests.
                let rest = guitario_theory::TimeSignature::COMMON.measure_ticks().saturating_sub(used);
                for _ in 0..rest / eighth.ticks().max(1) {
                    v.beats.push(guitario_score::Beat::rest(eighth));
                }
            }
        }
        score.tracks = vec![t];
        self.audition(&score);
    }

    pub(crate) fn scale_dialog(&mut self, ctx: &egui::Context) {
        if !self.scales.open {
            return;
        }
        let (mut close, mut find, mut play) = (false, false, false);
        let has_selection = self.session.selection().is_some();
        let d = &mut self.scales;
        egui::Modal::new(egui::Id::new("scale-dialog")).show(ctx, |ui| {
            ui.set_width(820.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(560.0);
                    ui.label(RichText::new("SCALES").strong().size(20.0));
                    let listed = d.listed();
                    egui::Frame::new().fill(theme::tokens().chrome_dark).inner_margin(4.0).show(ui, |ui| {
                        ui.set_width(552.0);
                        egui::ScrollArea::vertical().id_salt("scale-list").max_height(230.0).show(ui, |ui| match &d.found {
                            Some(found) => {
                                if found.is_empty() {
                                    ui.label("No scale holds these notes.");
                                }
                                let mut pick = None;
                                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                                    for (root, scale, percent) in found {
                                        let name = SCALES.get(*scale).map_or("?", |s| s.name);
                                        let text = format!("{:>5.1} %   {} {name}", percent, pitch_class_name(*root, d.flats));
                                        let on = d.scale == *scale && d.root == *root;
                                        if ui.selectable_label(on, RichText::new(text).size(15.0)).clicked() {
                                            pick = Some((*root, *scale));
                                        }
                                    }
                                });
                                if let Some((root, scale)) = pick {
                                    d.root = root;
                                    d.scale = scale;
                                }
                            }
                            None => {
                                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                                    for i in listed {
                                        let name = SCALES.get(i).map_or("?", |s| s.name);
                                        if ui.selectable_label(d.scale == i, RichText::new(name).size(15.0)).clicked() {
                                            d.scale = i;
                                        }
                                    }
                                });
                            }
                        });
                    });
                    if d.found.is_some() && ui.button("Show the scale list").clicked() {
                        d.found = None;
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("INTERVALS").strong().size(16.0));
                    ui.horizontal(|ui| {
                        ui.label("Key note:");
                        let flats = d.flats;
                        egui::ComboBox::from_id_salt("scale-root").width(60.0).selected_text(pitch_class_name(d.root, flats)).show_ui(ui, |ui| {
                            for pc in 0..12u8 {
                                ui.selectable_value(&mut d.root, pc, pitch_class_name(pc, flats));
                            }
                        });
                        ui.add_space(16.0);
                        ui.label("Preferred alteration:");
                        if ui.add(egui::Button::new("b").selected(d.flats).min_size(vec2(28.0, 20.0))).clicked() {
                            d.flats = true;
                        }
                        if ui.add(egui::Button::new("#").selected(!d.flats).min_size(vec2(28.0, 20.0))).clicked() {
                            d.flats = false;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(RichText::new("\u{25B6}").size(16.0)).on_hover_text("Play the scale").clicked() {
                                play = true;
                            }
                        });
                    });
                    intervals(ui, d);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(has_selection, egui::Button::new("Find Scales from Selection"))
                            .on_disabled_hover_text("Select the bars to analyse first")
                            .clicked()
                        {
                            find = true;
                        }
                        ui.radio_value(&mut d.on_key, false, "All key notes");
                        ui.radio_value(&mut d.on_key, true, "Selected key note");
                    });
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(220.0);
                    ui.label(RichText::new("SEARCH").strong().size(16.0));
                    ui.add(egui::TextEdit::singleline(&mut d.search).hint_text("Scale name").desired_width(210.0));
                    ui.add_space(8.0);
                    ui.label(RichText::new("TAGS").strong().size(16.0));
                    egui::ScrollArea::vertical().id_salt("scale-tags").max_height(300.0).show(ui, |ui| {
                        for tag in Tag::ALL {
                            let mut on = d.tags.contains(&tag);
                            if ui.checkbox(&mut on, tag.label()).changed() {
                                if on {
                                    d.tags.push(tag);
                                } else {
                                    d.tags.retain(|t| *t != tag);
                                }
                            }
                        }
                    });
                    ui.add_space(8.0);
                    ui.checkbox(&mut d.show, "Show on the fretboard and keyboard");
                });
            });
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
            });
        });
        self.view.scale = self.scales.show.then_some((self.scales.root, self.scales.scale, self.scales.flats));
        if find {
            self.find_scales();
        }
        if play {
            self.audition_scale();
        }
        if close {
            self.scales.open = false;
        }
    }
}

/// The scale on a line, as Guitar Pro draws it: the notes above, the steps between them on the line
/// (½, 1, 1½ tones), the intervals below.
fn intervals(ui: &mut egui::Ui, d: &ScaleDialog) {
    let Some(scale) = SCALES.get(d.scale) else { return };
    let (rect, _) = ui.allocate_exact_size(vec2(552.0, 110.0), Sense::hover());
    let t = theme::tokens();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, t.chrome_dark);
    let (x0, x1) = (rect.left() + 28.0, rect.right() - 28.0);
    let y = rect.center().y;
    let x_of = |semis: u8| x0 + (x1 - x0) * f32::from(semis) / 12.0;
    p.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(1.5, t.text));
    let notes = scale.notes(d.root, d.flats);
    let names = scale.interval_names();
    let positions: Vec<u8> = scale.intervals.iter().copied().chain(std::iter::once(12)).collect();
    for (k, &semis) in positions.iter().enumerate() {
        let x = x_of(semis);
        p.line_segment([pos2(x, y - 5.0), pos2(x, y + 5.0)], Stroke::new(1.5, t.text));
        let note = if semis == 12 { notes.first().cloned().unwrap_or_default() } else { notes.get(k).cloned().unwrap_or_default() };
        p.text(pos2(x, y - 22.0), egui::Align2::CENTER_CENTER, note, egui::FontId::proportional(17.0), t.text);
        p.text(pos2(x, y + 22.0), egui::Align2::CENTER_CENTER, names.get(k).copied().unwrap_or(""), egui::FontId::proportional(13.0), t.text_dim);
    }
    for (k, step) in scale.steps().iter().enumerate() {
        let (Some(a), Some(b)) = (positions.get(k), positions.get(k + 1)) else { continue };
        let label = match step {
            1 => "\u{BD}".to_string(),
            2 => "1".to_string(),
            3 => "1\u{BD}".to_string(),
            n => format!("{}", f32::from(*n) / 2.0),
        };
        let mid = pos2((x_of(*a) + x_of(*b)) / 2.0, y);
        let galley = p.layout_no_wrap(label, egui::FontId::proportional(13.0), t.accent);
        p.rect_filled(egui::Rect::from_center_size(mid, galley.size() + vec2(6.0, 0.0)), 0.0, t.chrome_dark);
        p.galley(mid - galley.size() / 2.0, galley, Color32::WHITE);
    }
}

/// What the views draw of a shown scale: its pitch classes, its key note, and each class spelled
/// as the scale spells it.
pub(crate) struct Overlay {
    pub mask: u16,
    pub root: u8,
    pub names: [String; 12],
}

pub(crate) fn overlay(view: Option<(u8, usize, bool)>) -> Option<Overlay> {
    let (root, index, flats) = view?;
    let scale = SCALES.get(index)?;
    let mut names: [String; 12] = Default::default();
    for (i, n) in scale.intervals.iter().zip(scale.notes(root, flats)) {
        if let Some(slot) = names.get_mut(usize::from((root + i) % 12)) {
            *slot = n;
        }
    }
    Some(Overlay { mask: scale.mask(root), root: root % 12, names })
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn the_list_follows_search_and_tags_and_the_views_show_the_scale() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.open_scales();
        assert!(app.scales.open);
        assert_eq!(app.view.scale, Some((0, 0, false)), "C major for a bar without sharps or flats");
        let names = |d: &ScaleDialog| d.listed().iter().map(|i| SCALES[*i].name).collect::<Vec<_>>();
        assert_eq!(names(&app.scales)[..2], ["Major", "Natural Minor"]);
        app.scales.search = "penta".into();
        app.scales.tags = vec![Tag::Minor];
        assert!(names(&app.scales).iter().all(|n| n.contains("Pentatonic")));
        let o = overlay(Some((9, 10, false))).unwrap();
        assert_eq!(o.root, 9);
        assert_eq!(o.mask, [9u8, 0, 2, 4, 7].iter().fold(0u16, |m, pc| m | 1 << pc), "A minor pentatonic");
        let dorian = overlay(Some((5, 3, false))).unwrap();
        assert_eq!((dorian.names[8].as_str(), dorian.names[3].as_str()), ("Ab", "Eb"), "spelled as the scale spells them");
        let ctx = egui::Context::default();
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.scale_dialog(ui.ctx())).textures_delta.clear();
        }
        assert!(app.scales.open);
    }

    #[test]
    fn find_from_selection_lists_scales_with_their_share() {
        let mut s = Session::new();
        for (string, fret) in [(0, 5), (0, 8), (1, 5), (1, 8), (2, 5), (2, 7)] {
            s.run("cursor.set", &json!({"string": string})).unwrap();
            s.run("note.setFret", &json!({"fret": fret})).unwrap();
            s.run("cursor.right", &json!({})).unwrap();
        }
        s.run("select.all", &json!({})).unwrap();
        let mut app = GuitarioApp::new(s, None);
        app.open_scales();
        app.scales.root = 9;
        app.scales.on_key = true;
        app.find_scales();
        let found = app.scales.found.clone().unwrap();
        assert_eq!((found[0].0, SCALES[found[0].1].name, found[0].2), (9, "Minor Pentatonic", 100.0));
    }
}
