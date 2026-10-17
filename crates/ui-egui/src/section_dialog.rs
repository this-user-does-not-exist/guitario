//! Section › Edit Section… (and the palette's Section button): Guitar Pro 8's small Section
//! window with a letter, a name (typed or picked from the usual names) and Remove, Cancel, OK.
//! A name in brackets, `[Chorus]`, is drawn framed in the score.

use crate::GuitarioApp;
use guitario_score::Score;
use serde_json::json;

/// The names Guitar Pro offers in the Name list.
pub(crate) const NAMES: [&str; 12] =
    ["Intro", "Verse", "Pre-Chorus", "Chorus", "Post-Chorus", "Bridge", "Interlude", "Breakdown", "Solo", "Riff", "Outro", "Ending"];

#[derive(Default)]
pub(crate) struct SectionDialog {
    pub(crate) open: bool,
    /// The bar the section starts on.
    measure: usize,
    letter: String,
    name: String,
    /// Whether the bar already starts a section (Remove is offered).
    existing: bool,
}

/// The letter a new section at `measure` takes: the one after the closest lettered section before
/// it, `A` when there is none (Guitar Pro counts on from the previous section).
pub(crate) fn next_letter(score: &Score, measure: usize) -> String {
    let prev = score.headers.iter().take(measure).rev().find_map(|h| h.marker.as_ref().and(h.section_letter.as_deref()));
    match prev.and_then(|l| l.chars().next()) {
        Some(c @ 'A'..='Y') | Some(c @ 'a'..='y') => char::from(c as u8 + 1).to_string(),
        _ => "A".to_string(),
    }
}

/// The name as drawn and whether it is framed: `[Chorus]` is `Chorus`, framed.
pub(crate) fn display_name(name: &str) -> (&str, bool) {
    let t = name.trim();
    match t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        Some(inner) => (inner.trim(), true),
        None => (t, false),
    }
}

impl GuitarioApp {
    /// Opens the window on the cursor's bar, filled in with its section when it has one.
    pub fn open_section(&mut self) {
        let measure = self.session.cursor().measure;
        let score = self.session.score();
        let header = score.headers.get(measure);
        let existing = header.is_some_and(|h| h.marker.is_some());
        let letter = header.and_then(|h| h.marker.as_ref().and(h.section_letter.clone())).unwrap_or_else(|| next_letter(score, measure));
        let name = header.and_then(|h| h.marker.clone()).unwrap_or_default();
        self.section = SectionDialog { open: true, measure, letter, name, existing };
    }

    pub(crate) fn section_dialog(&mut self, ctx: &egui::Context) {
        if !self.section.open {
            return;
        }
        enum Out {
            Ok,
            Remove,
            Cancel,
        }
        let mut out = None;
        let d = &mut self.section;
        let t = crate::theme::tokens();
        egui::Modal::new(egui::Id::new("section-dialog")).show(ctx, |ui| {
            ui.set_width(300.0);
            ui.label(egui::RichText::new("SECTION").font(crate::fonts::sans_bold(18.0)).color(t.text));
            ui.add_space(10.0);
            egui::Grid::new("section-grid").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
                ui.label("Letter:");
                egui::ComboBox::from_id_salt("section-letter").width(64.0).selected_text(d.letter.as_str()).show_ui(ui, |ui| {
                    for c in 'A'..='Z' {
                        let l = c.to_string();
                        if ui.selectable_label(d.letter == l, &l).clicked() {
                            d.letter = l;
                        }
                    }
                });
                ui.end_row();
                ui.label("Name:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(150.0).hint_text("Intro").frame(crate::theme::field_frame()));
                    egui::ComboBox::from_id_salt("section-name").width(24.0).selected_text("").show_ui(ui, |ui| {
                        for n in NAMES {
                            for name in [n.to_string(), format!("[{n}]")] {
                                if ui.selectable_label(d.name == name, &name).clicked() {
                                    d.name = name;
                                }
                            }
                        }
                    });
                });
                ui.end_row();
            });
            ui.label(egui::RichText::new("A name in [brackets] is framed in the score.").small().color(t.text_dim));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(d.existing, egui::Button::new("Remove")).clicked() {
                    out = Some(Out::Remove);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ok = egui::Button::new(egui::RichText::new("OK").color(egui::Color32::WHITE)).fill(t.accent);
                    if ui.add(ok).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        out = Some(Out::Ok);
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        out = Some(Out::Cancel);
                    }
                });
            });
        });
        let (measure, letter, name) = (d.measure, d.letter.trim().to_string(), d.name.trim().to_string());
        let params = match out {
            None => return,
            Some(Out::Cancel) => None,
            Some(Out::Remove) => Some(json!({"text": ""})),
            Some(Out::Ok) => Some(json!({"text": name, "letter": letter})),
        };
        self.section.open = false;
        if let Some(p) = params {
            // The bar the window was opened on, even if playback moved the cursor meanwhile.
            let c = self.session.cursor();
            if c.measure != measure {
                self.run("cursor.set", json!({"measure": measure, "beat": 0}));
            }
            self.run("measure.setMarker", p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::MeasureHeader;
    use guitario_theory::TimeSignature;

    #[test]
    fn a_new_section_takes_the_letter_after_the_previous_one() {
        let mut s = Score::new();
        s.headers = vec![MeasureHeader::new(TimeSignature::COMMON); 6];
        assert_eq!(next_letter(&s, 3), "A");
        s.headers[0].marker = Some("Intro".into());
        s.headers[0].section_letter = Some("A".into());
        s.headers[2].marker = Some("Verse".into());
        s.headers[2].section_letter = Some("C".into());
        assert_eq!(next_letter(&s, 1), "B");
        assert_eq!(next_letter(&s, 5), "D");
        s.headers[2].section_letter = Some("Z".into());
        assert_eq!(next_letter(&s, 5), "A", "after Z it starts again");
    }

    #[test]
    fn a_bracketed_name_is_framed_without_its_brackets() {
        assert_eq!(display_name("[Pre-Chorus]"), ("Pre-Chorus", true));
        assert_eq!(display_name(" Solo "), ("Solo", false));
        assert_eq!(display_name("[open"), ("[open", false));
    }
}
