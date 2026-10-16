//! The edition palette down the left edge, as in Guitar Pro: note values, rhythm modifiers,
//! dynamics, articulations and techniques, then bar tools. Buttons light up to show the state of
//! the beat and note under the cursor.

use crate::GuitarioApp;
use crate::fonts::{FontSet, serif};
use crate::glyphs::{self as g, Pen};
use crate::theme::tokens;
use egui::{Color32, Painter, Rect, Stroke, pos2, vec2};
use guitario_score::{Beat, Dynamic, Harmonic, Note, NoteKind, Slide};
use serde_json::{Value, json};

const CELL: f32 = 34.0;

type DrawFn = fn(&Painter, Rect, Color32);

enum Art {
    Glyph(char, f32),
    Text(&'static str),
    Draw(DrawFn),
}

fn cell(ui: &mut egui::Ui, fonts: FontSet, art: Art, on: bool, enabled: bool, tip: &str) -> egui::Response {
    let t = tokens();
    let (rect, resp) = ui.allocate_exact_size(vec2(CELL, CELL - 4.0), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let p = ui.painter();
    if on {
        p.rect_filled(rect.shrink(1.0), 3.0, t.accent.gamma_multiply(0.55));
    } else if resp.hovered() && enabled {
        p.rect_filled(rect.shrink(1.0), 3.0, t.control_hover);
    }
    let color = if !enabled {
        t.text_dim.gamma_multiply(0.5)
    } else if on {
        Color32::WHITE
    } else {
        t.text
    };
    match art {
        Art::Glyph(c, space) => {
            let pen = Pen { painter: p, fonts, space };
            let w = pen.width(c);
            pen.glyph(c, pos2(rect.center().x - w / 2.0, rect.center().y + rect.height() * 0.22), color);
        }
        Art::Text(s) => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, s, serif(12.5), color);
        }
        Art::Draw(f) => f(p, rect.shrink(8.0), color),
    }
    resp.on_hover_text(tip)
}

fn section(ui: &mut egui::Ui) {
    ui.add_space(3.0);
    let r = ui.available_rect_before_wrap();
    ui.painter().line_segment([pos2(r.left() + 2.0, r.top()), pos2(r.right() - 2.0, r.top())], Stroke::new(1.0, tokens().border));
    ui.add_space(4.0);
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let fonts = app.fonts;
    let score = app.session.score().clone();
    let c = app.session.cursor();
    let beat: Option<&Beat> =
        score.tracks.get(c.track).and_then(|t| t.measures.get(c.measure)).and_then(|m| m.voices.get(c.voice)).and_then(|v| v.beats.get(c.beat));
    let note: Option<&Note> = beat.and_then(|b| b.note_on(c.string as u8));
    // With a selection, note tools apply to every note in it. Guitar techniques don't apply to drums.
    let drums = score.tracks.get(c.track).is_some_and(guitario_score::Track::is_drums);
    let has_note = (note.is_some() || app.session.selection().is_some()) && !drums;
    let div = beat.map_or(4, |b| b.duration.value.divisor());
    let dots = beat.map_or(0, |b| b.duration.dots);
    let mut actions: Vec<(&'static str, Value)> = Vec::new();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let grid = |ui: &mut egui::Ui, id: &str, f: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Grid::new(id).spacing(vec2(2.0, 2.0)).min_col_width(CELL).show(ui, |ui| f(ui));
        };
        // Note values.
        grid(ui, "values", &mut |ui| {
            let values =
                [(1, "Whole note"), (2, "Half note"), (4, "Quarter note"), (8, "Eighth note"), (16, "16th note"), (32, "32nd note"), (64, "64th note")];
            for (i, (d, tip)) in values.iter().enumerate() {
                if cell(ui, fonts, Art::Glyph(g::metronome_for(*d), 6.5), div == *d, beat.is_some(), &format!("{tip} (Alt+{})", i + 1)).clicked() {
                    actions.push(("beat.setDuration", json!({"value": d, "dots": dots})));
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
            if cell(ui, fonts, Art::Draw(draw_rest), beat.is_some_and(|b| b.rest), beat.is_some(), "Rest (R)").clicked() {
                actions.push(("beat.toggleRest", Value::Null));
            }
            ui.end_row();
            if cell(ui, fonts, Art::Draw(draw_dot), dots == 1, beat.is_some(), "Dotted (.)").clicked() {
                actions.push(("beat.toggleDot", Value::Null));
            }
            if cell(ui, fonts, Art::Draw(draw_double_dot), dots == 2, beat.is_some(), "Double dotted").clicked() {
                actions.push(("beat.setDuration", json!({"value": div, "dots": if dots == 2 { 0 } else { 2 }})));
            }
            ui.end_row();
            if cell(ui, fonts, Art::Text("3"), beat.is_some_and(|b| b.duration.tuplet != (1, 1)), beat.is_some(), "Triplet (/)").clicked() {
                actions.push(("beat.toggleTriplet", Value::Null));
            }
            if cell(ui, fonts, Art::Draw(draw_tie), note.is_some_and(|n| n.kind == NoteKind::Tie), has_note, "Tied note (L)").clicked() {
                actions.push(("note.toggleTie", Value::Null));
            }
            ui.end_row();
        });
        section(ui);
        // Dynamics.
        grid(ui, "dynamics", &mut |ui| {
            let dyns = [
                (Dynamic::Ppp, '\u{E52A}', "ppp"),
                (Dynamic::Pp, '\u{E52B}', "pp"),
                (Dynamic::P, '\u{E520}', "p"),
                (Dynamic::Mp, '\u{E52C}', "mp"),
                (Dynamic::Mf, '\u{E52D}', "mf"),
                (Dynamic::F, '\u{E522}', "f"),
                (Dynamic::Ff, '\u{E52F}', "ff"),
                (Dynamic::Fff, '\u{E530}', "fff"),
            ];
            for (i, (d, glyph, name)) in dyns.iter().enumerate() {
                let art = if fonts.smufl { Art::Glyph(*glyph, 4.2) } else { Art::Text(name) };
                if cell(ui, fonts, art, beat.is_some_and(|b| b.dynamic == *d), beat.is_some(), name).clicked() {
                    actions.push(("beat.setDynamic", json!({"dynamic": name})));
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        section(ui);
        // Articulations and techniques on the note under the cursor.
        grid(ui, "effects", &mut |ui| {
            let e = note.map(|n| n.effects).unwrap_or_default();
            let toggles: [(Art, bool, &str, &str); 8] = [
                (Art::Glyph(g::ACCENT_ABOVE, 6.0), e.accent, "accent", "Accent (A)"),
                (Art::Glyph(g::STACCATO_ABOVE, 6.0), e.staccato, "staccato", "Staccato"),
                (Art::Text("( )"), e.ghost, "ghost", "Ghost note (G)"),
                (Art::Text("P.M."), e.palm_mute, "palmMute", "Palm mute (P)"),
                (Art::Text("L.R."), e.let_ring, "letRing", "Let ring (I)"),
                (Art::Draw(draw_vibrato), e.vibrato, "vibrato", "Vibrato (V)"),
                (Art::Text("H/P"), e.hammer, "hammer", "Hammer-on / pull-off (H)"),
                (Art::Text("X"), note.is_some_and(|n| n.kind == NoteKind::Dead), "", "Dead note (X)"),
            ];
            for (i, (art, on, id, tip)) in toggles.into_iter().enumerate() {
                if cell(ui, fonts, art, on, has_note, tip).clicked() {
                    if id.is_empty() {
                        actions.push(("note.toggleDead", Value::Null));
                    } else {
                        actions.push(("note.toggleEffect", json!({"effect": id})));
                    }
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
            let slides: [(DrawFn, Slide, &str, &str); 4] = [
                (draw_slide_shift, Slide::Shift, "shift", "Shift slide (S)"),
                (draw_slide_legato, Slide::Legato, "legato", "Legato slide"),
                (draw_slide_in, Slide::IntoFromBelow, "intoFromBelow", "Slide in from below"),
                (draw_slide_out, Slide::OutDown, "outDown", "Slide out downwards"),
            ];
            for (i, (f, kind, id, tip)) in slides.into_iter().enumerate() {
                let on = e.slide == Some(kind);
                if cell(ui, fonts, Art::Draw(f), on, has_note, tip).clicked() {
                    actions.push(("note.setSlide", json!({"slide": if on { "none" } else { id }})));
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
            if cell(ui, fonts, Art::Draw(draw_bend), e.bend.is_some(), has_note, "Bend, full step (B)").clicked() {
                actions.push(("note.setBend", json!({"quarterTones": if e.bend.is_some() { 0 } else { 4 }})));
            }
            if cell(ui, fonts, Art::Text("N.H."), e.harmonic == Some(Harmonic::Natural), has_note, "Natural harmonic").clicked() {
                actions.push(("note.setHarmonic", json!({"harmonic": if e.harmonic.is_some() { "none" } else { "natural" }})));
            }
            ui.end_row();
        });
        section(ui);
        // Bar tools.
        let header = score.headers.get(c.measure).cloned();
        grid(ui, "bar", &mut |ui| {
            let open = header.as_ref().is_some_and(|h| h.repeat_open);
            let close = header.as_ref().map_or(0, |h| h.repeat_close);
            if cell(ui, fonts, Art::Draw(draw_repeat_open), open, true, "Repeat open").clicked() {
                actions.push(("measure.setRepeat", json!({"open": !open})));
            }
            if cell(ui, fonts, Art::Draw(draw_repeat_close), close > 0, true, "Repeat close").clicked() {
                actions.push(("measure.setRepeat", json!({"close": if close > 0 { 0 } else { 1 }})));
            }
            ui.end_row();
            let r = cell(ui, fonts, Art::Draw(draw_time_sig), false, true, "Time signature");
            egui::Popup::menu(&r).show(|ui| {
                for (n, d) in [(2, 4), (3, 4), (4, 4), (5, 4), (6, 4), (3, 8), (6, 8), (7, 8), (9, 8), (12, 8)] {
                    if ui.button(format!("{n}/{d}")).clicked() {
                        actions.push(("measure.setTimeSignature", json!({"numerator": n, "denominator": d})));
                        ui.close();
                    }
                }
            });
            let r = cell(ui, fonts, Art::Glyph(g::SHARP, 6.0), false, true, "Key signature");
            egui::Popup::menu(&r).show(|ui| {
                let names = ["C♭", "G♭", "D♭", "A♭", "E♭", "B♭", "F", "C", "G", "D", "A", "E", "B", "F♯", "C♯"];
                for (i, n) in names.iter().enumerate() {
                    let k = i as i64 - 7;
                    if ui
                        .button(format!(
                            "{n} major ({})",
                            if k == 0 {
                                "no accidentals".to_string()
                            } else if k > 0 {
                                format!("{k}♯")
                            } else {
                                format!("{}♭", -k)
                            }
                        ))
                        .clicked()
                    {
                        actions.push(("measure.setKey", json!({"key": k})));
                        ui.close();
                    }
                }
            });
            ui.end_row();
            let r = cell(ui, fonts, Art::Glyph(g::MET_QUARTER_UP, 5.0), header.as_ref().is_some_and(|h| h.tempo.is_some()), true, "Tempo change at this bar");
            egui::Popup::menu(&r).show(|ui| {
                let mut bpm = i64::from(header.as_ref().and_then(|h| h.tempo).unwrap_or(score.tempo_at(c.measure)));
                ui.horizontal(|ui| {
                    ui.label("♩ =");
                    if ui.add(egui::DragValue::new(&mut bpm).range(20..=400)).changed() {
                        actions.push(("measure.setTempo", json!({"bpm": bpm})));
                    }
                });
                if ui.button("Remove tempo change").clicked() {
                    actions.push(("measure.setTempo", json!({"bpm": null})));
                    ui.close();
                }
            });
            let r = cell(ui, fonts, Art::Text("A"), header.as_ref().is_some_and(|h| h.marker.is_some()), true, "Section marker");
            egui::Popup::menu(&r).show(|ui| {
                for name in ["Intro", "Verse", "Pre-Chorus", "Chorus", "Bridge", "Solo", "Outro"] {
                    if ui.button(name).clicked() {
                        actions.push(("measure.setMarker", json!({"text": name})));
                        ui.close();
                    }
                }
                if ui.button("Remove marker").clicked() {
                    actions.push(("measure.setMarker", json!({"text": ""})));
                    ui.close();
                }
            });
            ui.end_row();
            if cell(ui, fonts, Art::Text("+ |"), false, true, "Insert bar before").clicked() {
                actions.push(("measure.insert", Value::Null));
            }
            if cell(ui, fonts, Art::Text("− |"), false, score.headers.len() > 1, "Delete bar").clicked() {
                actions.push(("measure.delete", Value::Null));
            }
            ui.end_row();
        });
    });
    for (id, params) in actions {
        app.run(id, params);
    }
}

fn st(c: Color32) -> Stroke {
    Stroke::new(1.5, c)
}

fn draw_rest(p: &Painter, r: Rect, c: Color32) {
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    p.add(egui::Shape::line(vec![at(0.4, 0.0), at(0.65, 0.3), at(0.4, 0.55), at(0.65, 0.8), at(0.45, 0.78), at(0.55, 1.0)], Stroke::new(2.0, c)));
}

fn draw_dot(p: &Painter, r: Rect, c: Color32) {
    p.circle_filled(r.center(), 2.6, c);
}

fn draw_double_dot(p: &Painter, r: Rect, c: Color32) {
    p.circle_filled(r.center() - vec2(4.0, 0.0), 2.4, c);
    p.circle_filled(r.center() + vec2(4.0, 0.0), 2.4, c);
}

fn draw_tie(p: &Painter, r: Rect, c: Color32) {
    g::arc(p, pos2(r.left(), r.center().y), pos2(r.right(), r.center().y), 5.0, Stroke::new(2.0, c));
}

fn draw_vibrato(p: &Painter, r: Rect, c: Color32) {
    g::wavy(p, r.left(), r.right(), r.center().y, 2.5, st(c));
}

fn draw_slide_shift(p: &Painter, r: Rect, c: Color32) {
    p.line_segment([r.left_bottom(), r.right_top()], st(c));
}

fn draw_slide_legato(p: &Painter, r: Rect, c: Color32) {
    p.line_segment([r.left_bottom(), pos2(r.right(), r.center().y)], st(c));
    g::arc(p, pos2(r.left(), r.top() + 3.0), pos2(r.right(), r.top() + 3.0), -3.0, st(c));
}

fn draw_slide_in(p: &Painter, r: Rect, c: Color32) {
    p.line_segment([r.left_bottom(), r.center()], st(c));
    p.text(r.right_center(), egui::Align2::RIGHT_CENTER, "5", egui::FontId::proportional(11.0), c);
}

fn draw_slide_out(p: &Painter, r: Rect, c: Color32) {
    p.text(r.left_center(), egui::Align2::LEFT_CENTER, "5", egui::FontId::proportional(11.0), c);
    p.line_segment([r.center(), r.right_bottom()], st(c));
}

fn draw_bend(p: &Painter, r: Rect, c: Color32) {
    let a = r.left_bottom();
    let b = pos2(r.right() - 3.0, r.top() + 2.0);
    p.add(egui::epaint::QuadraticBezierShape::from_points_stroke([a, pos2(b.x, a.y), b], false, Color32::TRANSPARENT, st(c)));
    p.add(egui::Shape::convex_polygon(vec![b - vec2(0.0, 3.0), b + vec2(-3.0, 2.0), b + vec2(3.0, 2.0)], c, Stroke::NONE));
}

fn draw_repeat_open(p: &Painter, r: Rect, c: Color32) {
    p.line_segment([r.left_top(), r.left_bottom()], Stroke::new(3.0, c));
    p.line_segment([pos2(r.left() + 5.0, r.top()), pos2(r.left() + 5.0, r.bottom())], st(c));
    p.circle_filled(pos2(r.left() + 10.0, r.center().y - 3.5), 1.8, c);
    p.circle_filled(pos2(r.left() + 10.0, r.center().y + 3.5), 1.8, c);
}

fn draw_repeat_close(p: &Painter, r: Rect, c: Color32) {
    p.line_segment([r.right_top(), r.right_bottom()], Stroke::new(3.0, c));
    p.line_segment([pos2(r.right() - 5.0, r.top()), pos2(r.right() - 5.0, r.bottom())], st(c));
    p.circle_filled(pos2(r.right() - 10.0, r.center().y - 3.5), 1.8, c);
    p.circle_filled(pos2(r.right() - 10.0, r.center().y + 3.5), 1.8, c);
}

fn draw_time_sig(p: &Painter, r: Rect, c: Color32) {
    p.text(pos2(r.center().x, r.center().y - 5.0), egui::Align2::CENTER_CENTER, "4", egui::FontId::proportional(12.0), c);
    p.text(pos2(r.center().x, r.center().y + 6.0), egui::Align2::CENTER_CENTER, "4", egui::FontId::proportional(12.0), c);
}
