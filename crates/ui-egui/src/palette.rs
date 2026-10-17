//! The edition palette down the left edge, laid out as Guitar Pro 8's: the voices, multivoice and
//! design mode, Lyrics and Chords, then bar symbols, note symbols, effect symbols, notation symbols
//! and automations, eight buttons to a row. Buttons light up for the beat and note under the
//! cursor; symbols that do not apply there, or that Guitario cannot write yet, are dimmed.
//!
//! The palette is data first (`sections`), so its layout and wiring are tested without drawing.

use crate::GuitarioApp;
use crate::fonts::{FontSet, serif, serif_italic};
use crate::glyphs::{self as g, Pen};
use crate::menu_catalog::UiAction;
use crate::theme::tokens;
use egui::{Color32, Painter, Rect, Shape, Stroke, pos2, vec2};
use guitario_score::{Beat, Dynamic, Harmonic, MeasureHeader, Note, NoteKind, Slide, StrokeDirection, TripletFeel};
use serde_json::{Value, json};

/// Guitar Pro's palette is eight buttons wide.
pub const CELL: f32 = 26.0;
pub const COLUMNS: f32 = 8.0;
/// What a dimmed button not built yet says.
const LATER: &str = " (not in Guitario yet)";
/// The active voice is orange, as in Guitar Pro.
const VOICE_ON: Color32 = Color32::from_rgb(0xf5, 0xa5, 0x3c);

type DrawFn = fn(&Painter, Rect, Color32);

#[derive(Clone, Copy)]
enum Art {
    /// A SMuFL glyph filling this fraction of the button; without the music font the pen draws
    /// its own stand-in.
    Glyph(char, f32),
    /// A SMuFL glyph filling this fraction of the button, or this text without the music font.
    Smufl(char, f32, &'static str),
    /// Glyphs in a row, filling this fraction of the button.
    Glyphs(&'static str, f32),
    /// A SMuFL glyph at the size the second one fills this fraction of the button at, so a set
    /// (note values) shares one size as in Guitar Pro.
    GlyphAs(char, char, f32),
    /// A dynamic: SMuFL glyphs all at one size on one baseline, or this text without the font.
    Dynamic(char, &'static str),
    Text(&'static str),
    Italic(&'static str),
    Draw(DrawFn),
    /// An empty slot, keeping the grid.
    Blank,
}

/// A popup a button opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pop {
    Key,
    Time,
    TripletFeel,
    Endings,
    RepeatClose,
    Directions,
    Tuplet,
    Fermata,
    Harmonic,
    Slur,
    Text,
}

/// A window a button opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Window {
    Lyrics,
    Chords,
    Trill,
    Grace,
    Fingering,
    Tremolo,
}

#[derive(Clone, Debug)]
enum Act {
    Run(&'static str, Value),
    Pop(Pop),
    Ui(UiAction),
    Window(Window),
    /// Dimmed: nothing to do.
    Off,
}

struct Button {
    art: Art,
    tip: String,
    on: bool,
    enabled: bool,
    act: Act,
    /// Cells wide.
    span: usize,
    /// Guitar Pro's corner triangle: the symbol has options.
    options: bool,
    /// Drawn on a rounded button, as the multivoice and design-mode buttons are.
    boxed: bool,
}

fn b(art: Art, tip: impl Into<String>, on: bool, enabled: bool, act: Act) -> Button {
    Button { art, tip: tip.into(), on, enabled, act, span: 1, options: false, boxed: false }
}

impl Button {
    /// Two cells wide, as Guitar Pro's hairpins are.
    fn wide(mut self) -> Self {
        self.span = 2;
        self
    }

    fn options(mut self) -> Self {
        self.options = true;
        self
    }

    fn boxed(mut self) -> Self {
        self.boxed = true;
        self
    }
}

/// Buttons in rows of `COLUMNS` cells; a wide button takes two.
fn rows(buttons: &[Button]) -> Vec<&[Button]> {
    let mut out = Vec::new();
    let (mut start, mut used) = (0, 0);
    for (i, btn) in buttons.iter().enumerate() {
        if used + btn.span > COLUMNS as usize && used > 0 {
            out.extend(buttons.get(start..i));
            (start, used) = (i, 0);
        }
        used += btn.span;
    }
    if start < buttons.len() {
        out.extend(buttons.get(start..));
    }
    out
}

/// A symbol Guitario cannot write yet: dimmed, saying so.
fn later(art: Art, name: &str) -> Button {
    b(art, format!("{name}{LATER}"), false, false, Act::Off)
}

fn blank() -> Button {
    b(Art::Blank, "", false, false, Act::Off)
}

/// What the palette reads at the cursor.
struct State<'a> {
    beat: Option<&'a Beat>,
    note: Option<&'a Note>,
    header: Option<&'a MeasureHeader>,
    /// A note (or a selection) on a string track: note symbols apply.
    has_note: bool,
    /// A beat to work on.
    has_beat: bool,
    fermata: bool,
}

fn run(id: &'static str, params: Value) -> Act {
    Act::Run(id, params)
}

/// The bar, note, effect, notation and automation sections, in Guitar Pro 8's order.
fn sections(s: &State) -> Vec<(&'static str, Vec<Button>)> {
    let e = s.note.map(|n| n.effects).unwrap_or_default();
    let hd = s.header;
    let (hn, hb) = (s.has_note, s.has_beat);
    let div = s.beat.map_or(4, |b| b.duration.value.divisor());
    let dots = s.beat.map_or(0, |b| b.duration.dots);
    let beat = s.beat;

    let open = hd.is_some_and(|h| h.repeat_open);
    let close = hd.map_or(0, |h| h.repeat_close);
    let free = hd.is_some_and(|h| h.free_time);
    let double = hd.is_some_and(|h| h.double_bar);
    let bar = vec![
        later(Art::Glyph(g::G_CLEF, 0.9), "Clef"),
        b(Art::Glyphs("\u{E262}\u{E262}\u{E262}", 0.75), "Key signature", hd.is_some_and(|h| h.key != 0), true, Act::Pop(Pop::Key)),
        b(Art::Draw(draw_time_sig), "Time signature", false, true, Act::Pop(Pop::Time)),
        b(Art::Draw(draw_triplet_feel), "Triplet feel", hd.is_some_and(|h| h.triplet_feel != TripletFeel::None), true, Act::Pop(Pop::TripletFeel)),
        b(Art::Draw(draw_free_time), "Free time", free, true, run("measure.toggleFreeTime", Value::Null)),
        b(Art::Draw(draw_double_bar), "Double bar line", double, true, run("measure.toggleDoubleBar", Value::Null)),
        later(Art::Smufl('\u{E500}', 0.55, "%"), "Single repeat sign"),
        later(Art::Smufl('\u{E501}', 0.75, "%%"), "Double repeat sign"),
        b(Art::Draw(draw_repeat_open), "Repeat start", open, true, run("measure.setRepeat", json!({"open": !open}))),
        b(Art::Draw(draw_endings), "Alternate endings", hd.is_some_and(|h| h.endings != 0), true, Act::Pop(Pop::Endings)),
        b(Art::Draw(draw_repeat_close), "Repeat close", close > 0, true, Act::Pop(Pop::RepeatClose)),
        b(Art::Glyph(g::CODA, 0.7), "Directions", hd.is_some_and(|h| !h.targets.is_empty() || h.jump.is_some()), true, Act::Pop(Pop::Directions)),
        later(Art::Smufl('\u{E511}', 0.85, "8va"), "8va octaviation"),
        later(Art::Smufl('\u{E51C}', 0.85, "8vb"), "8vb octaviation"),
        later(Art::Smufl('\u{E515}', 0.85, "15ma"), "15ma octaviation"),
        later(Art::Smufl('\u{E51D}', 0.85, "15mb"), "15mb octaviation"),
    ];

    let values = [
        (1, "Whole note"),
        (2, "Half note"),
        (4, "Quarter note"),
        (8, "Eighth note"),
        (16, "Sixteenth note"),
        (32, "Thirty-second note"),
        (64, "Sixty-fourth note"),
    ];
    let mut note: Vec<Button> = values
        .iter()
        .enumerate()
        .map(|(i, (d, tip))| {
            b(
                Art::GlyphAs(g::metronome_for(*d), g::metronome_for(4), 0.85),
                format!("{tip} (Alt+{})", i + 1),
                div == *d,
                hb,
                run("beat.setDuration", json!({"value": d, "dots": dots})),
            )
        })
        .collect();
    let tuplet = beat.is_some_and(|b| b.duration.tuplet != (1, 1));
    let triplet = beat.is_some_and(|b| b.duration.tuplet == (3, 2));
    note.extend([
        b(Art::Draw(draw_rest), "Rest (R)", beat.is_some_and(|b| b.rest), hb, run("beat.toggleRest", Value::Null)),
        b(Art::Draw(draw_dot), "Dotted note (.)", dots == 1, hb, run("beat.toggleDot", Value::Null)),
        b(
            Art::Draw(draw_double_dot),
            "Double-dotted note",
            dots == 2,
            hb,
            run("beat.setDuration", json!({"value": div, "dots": if dots == 2 { 0 } else { 2 }})),
        ),
        b(Art::Draw(draw_triplet), "Triplet (/)", triplet, hb, run("beat.toggleTriplet", Value::Null)),
        b(Art::Draw(draw_tuplet), "Custom n-tuplet", tuplet && !triplet, hb, Act::Pop(Pop::Tuplet)).options(),
        later(Art::Draw(draw_nested_tuplet), "Nested n-tuplet").options(),
        b(Art::Draw(draw_tie), "Tie (L)", s.note.is_some_and(|n| n.kind == NoteKind::Tie), hn, run("note.toggleTie", Value::Null)),
        later(Art::Draw(draw_ties), "Multiple ties"),
        b(Art::Glyph(g::FERMATA, 0.7), "Fermata (every track)", s.fermata, hb, Act::Pop(Pop::Fermata)),
        later(Art::Smufl('\u{E264}', 0.6, "bb"), "Double flat"),
        later(Art::Glyph(g::FLAT, 0.6), "Flat"),
        later(Art::Glyph(g::NATURAL, 0.65), "Natural"),
        later(Art::Glyph(g::SHARP, 0.65), "Sharp"),
        later(Art::Smufl('\u{E263}', 0.35, "x"), "Double sharp"),
        later(Art::Text("b♯"), "Change accidental"),
        b(Art::Draw(draw_semitone_down), "Lower by a semitone", false, hn, run("edit.transposeDown", Value::Null)),
        b(Art::Draw(draw_semitone_up), "Raise by a semitone", false, hn, run("edit.transposeUp", Value::Null)),
    ]);
    let dyns = [
        (Dynamic::Ppp, '\u{E52A}', "ppp", "Pianississimo"),
        (Dynamic::Pp, '\u{E52B}', "pp", "Pianissimo"),
        (Dynamic::P, '\u{E520}', "p", "Piano"),
        (Dynamic::Mp, '\u{E52C}', "mp", "Mezzo piano"),
        (Dynamic::Mf, '\u{E52D}', "mf", "Mezzo forte"),
        (Dynamic::F, '\u{E522}', "f", "Forte"),
        (Dynamic::Ff, '\u{E52F}', "ff", "Fortissimo"),
        (Dynamic::Fff, '\u{E530}', "fff", "Fortississimo"),
    ];
    note.extend(dyns.iter().map(|(d, glyph, name, tip)| {
        b(Art::Dynamic(*glyph, name), format!("{tip} ({name})"), beat.is_some_and(|b| b.dynamic == *d), hb, run("beat.setDynamic", json!({"dynamic": name})))
    }));
    note.extend([later(Art::Draw(draw_crescendo), "Crescendo").wide(), later(Art::Draw(draw_decrescendo), "Decrescendo").wide()]);

    let toggle = |effect: &'static str| run("note.toggleEffect", json!({"effect": effect}));
    let slide = |kind: Slide, id: &'static str| run("note.setSlide", json!({"slide": if e.slide == Some(kind) { "none" } else { id }}));
    let harmonic = e.harmonic;
    let stroke = beat.and_then(|b| b.stroke);
    let brush = |kind: &'static str, dir: &'static str| run("beat.setBrush", json!({"kind": kind, "direction": dir}));
    let technique = beat.and_then(|b| b.technique);
    let fade = beat.and_then(|b| b.fade);
    let fade_act = |f: &'static str, on: bool| run("beat.setFade", json!({"fade": if on { "none" } else { f }}));
    let has_fade = |f: guitario_score::Fade| fade == Some(f);
    let tech =
        |t: guitario_score::BeatTechnique, id: &'static str| run("beat.setTechnique", json!({"technique": if technique == Some(t) { "none" } else { id }}));
    let effects = vec![
        // Dynamics and articulation.
        b(Art::Draw(draw_ghost), "Ghost note (G)", e.ghost, hn, toggle("ghost")),
        b(Art::Draw(draw_accent), "Accented note (A)", e.accent, hn, toggle("accent")),
        b(Art::Draw(draw_heavy_accent), "Heavily accented note", e.heavy_accent, hn, toggle("heavyAccent")),
        b(Art::Draw(draw_staccato), "Staccato", e.staccato, hn, toggle("staccato")),
        b(Art::Draw(draw_slur), "Legato (slur)", beat.is_some_and(|b| b.slur.is_some()), hb, Act::Pop(Pop::Slur)),
        b(Art::Italic("let\nring"), "Let ring (I)", e.let_ring, hn, toggle("letRing")),
        later(Art::Smufl('\u{E650}', 0.85, "Ped."), "Sustain pedal"),
        b(Art::Text("P.M."), "Palm mute (P)", e.palm_mute, hn, toggle("palmMute")),
        // Dead notes, harmonics, bends, vibrato.
        b(Art::Draw(draw_dead), "Dead note (X)", s.note.is_some_and(|n| n.kind == NoteKind::Dead), hn, run("note.toggleDead", Value::Null)),
        later(Art::Draw(draw_dead_slap), "Dead slap"),
        b(
            Art::Draw(draw_harmonic_natural),
            "Natural harmonic",
            harmonic == Some(Harmonic::Natural),
            hn,
            run("note.setHarmonic", json!({"harmonic": if harmonic == Some(Harmonic::Natural) { "none" } else { "natural" }})),
        ),
        b(Art::Draw(draw_harmonic_artificial), "Artificial harmonic", harmonic.is_some_and(|h| h != Harmonic::Natural), hn, Act::Pop(Pop::Harmonic)),
        b(
            Art::Draw(draw_bend),
            "Bend, full step (B)",
            e.bend.is_some(),
            hn,
            run("note.setBend", json!({"quarterTones": if e.bend.is_some() { 0 } else { 4 }})),
        ),
        later(Art::Draw(draw_whammy), "Whammy bar"),
        b(Art::Draw(draw_vibrato), "Slight vibrato (V)", e.vibrato, hn, toggle("vibrato")).options(),
        b(Art::Draw(draw_wide_vibrato), "Wide vibrato (W)", e.wide_vibrato, hn, toggle("wideVibrato")).options(),
        // Bridge vibrato and slides.
        later(Art::Draw(draw_bridge_vibrato), "Slight bridge vibrato"),
        later(Art::Draw(draw_bridge_vibrato_wide), "Wide bridge vibrato"),
        b(Art::Draw(draw_slide_legato), "Legato slide", e.slide == Some(Slide::Legato), hn, slide(Slide::Legato, "legato")),
        b(Art::Draw(draw_slide_shift), "Shift slide (S)", e.slide == Some(Slide::Shift), hn, slide(Slide::Shift, "shift")),
        b(Art::Draw(draw_slide_in_below), "Slide in from below", e.slide == Some(Slide::IntoFromBelow), hn, slide(Slide::IntoFromBelow, "intoFromBelow")),
        b(Art::Draw(draw_slide_in_above), "Slide in from above", e.slide == Some(Slide::IntoFromAbove), hn, slide(Slide::IntoFromAbove, "intoFromAbove")),
        b(Art::Draw(draw_slide_out_down), "Slide out downwards", e.slide == Some(Slide::OutDown), hn, slide(Slide::OutDown, "outDown")),
        b(Art::Draw(draw_slide_out_up), "Slide out upwards", e.slide == Some(Slide::OutUp), hn, slide(Slide::OutUp, "outUp")),
        // Left and right hand.
        b(Art::Draw(draw_hopo), "Hammer-on / pull-off (H)", e.hammer, hn, toggle("hammer")),
        b(Art::Draw(draw_lh_tapping), "Left-hand tapping", e.left_hand_tapped, hn, toggle("leftHandTapping")),
        b(Art::Italic("tap"), "Tapping", e.tapped, hn, toggle("tapping")),
        b(Art::Italic("slap"), "Slap", technique == Some(guitario_score::BeatTechnique::Slap), hb, tech(guitario_score::BeatTechnique::Slap, "slap")),
        b(Art::Italic("pop"), "Pop", technique == Some(guitario_score::BeatTechnique::Pop), hb, tech(guitario_score::BeatTechnique::Pop, "pop")),
        b(Art::Draw(draw_hand_left), "Left-hand fingering", s.note.is_some_and(|n| n.effects.left_finger.is_some()), hn, Act::Window(Window::Fingering)),
        b(Art::Draw(draw_hand_right), "Right-hand fingering", s.note.is_some_and(|n| n.effects.right_finger.is_some()), hn, Act::Window(Window::Fingering)),
        later(Art::Draw(draw_string_number), "String number"),
        // Strums and pick strokes.
        b(Art::Draw(draw_brush_up), "Upward brush", false, hb, brush("brush", "up")),
        b(Art::Draw(draw_brush_down), "Downward brush", false, hb, brush("brush", "down")),
        b(Art::Draw(draw_arpeggio_up), "Upward arpeggio", false, hb, brush("arpeggio", "up")),
        b(Art::Draw(draw_arpeggio_down), "Downward arpeggio", false, hb, brush("arpeggio", "down")),
        b(Art::Italic("rasg."), "Rasgueado", false, hb, brush("rasgueado", "down")),
        b(
            Art::Glyph(g::STROKE_DOWN, 0.45),
            "Downward pickstroke",
            stroke == Some(StrokeDirection::Down),
            hb,
            run("beat.setStroke", json!({"stroke": if stroke == Some(StrokeDirection::Down) { "none" } else { "down" }})),
        ),
        b(
            Art::Glyph(g::STROKE_UP, 0.5),
            "Upward pickstroke",
            stroke == Some(StrokeDirection::Up),
            hb,
            run("beat.setStroke", json!({"stroke": if stroke == Some(StrokeDirection::Up) { "none" } else { "up" }})),
        ),
        blank(),
        // Ornaments.
        b(Art::Draw(draw_grace_before), "Grace note before the beat", s.note.is_some_and(|n| n.effects.grace.is_some()), hn, Act::Window(Window::Grace)),
        b(Art::Draw(draw_grace_on), "Grace note on the beat", false, hn, Act::Window(Window::Grace)),
        b(Art::Smufl('\u{E566}', 0.6, "tr"), "Trill", e.trill.is_some(), hn, Act::Window(Window::Trill)),
        b(Art::Draw(draw_tremolo), "Tremolo picking", beat.is_some_and(|b| b.tremolo.is_some()), hb, Act::Window(Window::Tremolo)).options(),
        later(Art::Smufl('\u{E56C}', 0.75, "~"), "Upper mordent"),
        later(Art::Smufl('\u{E56D}', 0.75, "~"), "Lower mordent"),
        later(Art::Smufl('\u{E567}', 0.75, "∞"), "Turn"),
        later(Art::Smufl('\u{E568}', 0.75, "∞"), "Inverted turn"),
        // Volume and wah.
        b(Art::Draw(draw_crescendo), "Fade-in", has_fade(guitario_score::Fade::In), hb, fade_act("in", has_fade(guitario_score::Fade::In))),
        b(Art::Draw(draw_decrescendo), "Fade-out", has_fade(guitario_score::Fade::Out), hb, fade_act("out", has_fade(guitario_score::Fade::Out))),
        b(Art::Draw(draw_swell), "Volume swell", has_fade(guitario_score::Fade::Swell), hb, fade_act("swell", has_fade(guitario_score::Fade::Swell))),
        later(Art::Draw(draw_wah_open), "Open wah-wah"),
        later(Art::Draw(draw_wah_closed), "Closed wah-wah"),
        later(Art::Draw(draw_golpe_finger), "Finger golpe"),
        later(Art::Draw(draw_golpe_thumb), "Thumb golpe"),
    ];

    let beam = beat.and_then(|b| b.beam);
    let notation = vec![
        b(Art::Draw(draw_chord), "Chord", beat.is_some_and(|b| b.chord.is_some()), hb, Act::Window(Window::Chords)),
        later(Art::Draw(draw_scale), "Scale diagram"),
        later(Art::Draw(draw_slash), "Slash notation"),
        later(Art::Text("BV"), "Extended barre"),
        later(Art::Text("2:51"), "Timer"),
        b(Art::Text("TXT"), "Text", beat.is_some_and(|b| b.text.is_some()), hb, Act::Pop(Pop::Text)),
        b(Art::Draw(draw_section), "Section", hd.is_some_and(|h| h.marker.is_some()), true, Act::Ui(UiAction::Section)),
        blank(),
        b(Art::Draw(draw_beam_auto), "Automatic beaming", hb && beam.is_none(), hb, run("beat.setBeam", json!({"mode": "auto"}))),
        b(Art::Draw(draw_beam), "Force beaming", beam == Some(guitario_score::BeamOverride::Join), hb, run("beat.setBeam", json!({"mode": "join"}))),
        b(Art::Draw(draw_beam_break), "Break beaming", beam == Some(guitario_score::BeamOverride::Break), hb, run("beat.setBeam", json!({"mode": "break"}))),
        later(Art::Draw(draw_beam_secondary), "Break secondary beaming"),
        later(Art::Draw(draw_stem_auto), "Automatic stem direction"),
        later(Art::Draw(draw_stem_up), "Stem up"),
        later(Art::Draw(draw_stem_down), "Stem down"),
        later(Art::Draw(draw_tuplet_groups), "Force groups"),
    ];

    let tempo_set = hd.is_some_and(|h| h.tempo.is_some());
    let automations = vec![
        b(Art::Draw(draw_tempo), "Tempo automation", tempo_set, true, Act::Ui(UiAction::TempoAutomation)),
        later(Art::Draw(draw_master_volume), "Master track volume automation"),
        later(Art::Draw(draw_master_pan), "Master track pan automation"),
        b(Art::Draw(draw_track_volume), "Track volume automation", false, true, Act::Ui(UiAction::VolumeAutomation)),
        b(Art::Draw(draw_track_pan), "Track pan automation", false, true, Act::Ui(UiAction::PanAutomation)),
    ];
    vec![("bar", bar), ("note", note), ("effects", effects), ("notation", notation), ("automations", automations)]
}

fn cell(ui: &mut egui::Ui, fonts: FontSet, btn: &Button) -> egui::Response {
    let t = tokens();
    let gap = ui.spacing().item_spacing.x;
    let width = CELL * btn.span as f32 + gap * (btn.span.saturating_sub(1)) as f32;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, CELL), if btn.enabled { egui::Sense::click() } else { egui::Sense::hover() });
    if matches!(btn.art, Art::Blank) {
        return resp;
    }
    let p = ui.painter();
    if btn.on {
        p.rect_filled(rect.shrink(1.0), 4.0, t.accent);
    } else if resp.hovered() && btn.enabled {
        p.rect_filled(rect.shrink(1.0), 4.0, t.control_hover);
    } else if btn.boxed {
        p.rect_filled(rect, 5.0, t.control);
    }
    let color = if !btn.enabled {
        t.text_dim.gamma_multiply(0.45)
    } else if btn.on {
        Color32::WHITE
    } else {
        t.text
    };
    // Without the music font the pen draws its own stand-in at a staff space for the fill.
    let pen = |fill: f32| Pen { painter: p, fonts, space: CELL * fill / 5.0 };
    match btn.art {
        Art::Glyph(c, fill) if fonts.smufl => fit_glyph(p, &c.to_string(), rect, fill, color),
        Art::Glyph(c, fill) => {
            let pen = pen(fill);
            let w = pen.width(c);
            pen.glyph(c, pos2(rect.center().x - w / 2.0, rect.center().y + rect.height() * 0.26), color);
        }
        Art::Smufl(c, fill, _) if fonts.smufl => fit_glyph(p, &c.to_string(), rect, fill, color),
        Art::Smufl(_, _, text) => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, text, serif_italic(12.0), color);
        }
        Art::Glyphs(text, fill) if fonts.smufl => fit_glyph(p, text, rect, fill, color),
        Art::GlyphAs(c, like, fill) if fonts.smufl => fit_glyph_as(p, &c.to_string(), &like.to_string(), rect, fill, color),
        Art::GlyphAs(c, _, fill) => {
            let pen = pen(fill);
            let w = pen.width(c);
            pen.glyph(c, pos2(rect.center().x - w / 2.0, rect.center().y + rect.height() * 0.26), color);
        }
        // Every dynamic at the size "fff" fills the button at, on its baseline.
        Art::Dynamic(c, _) if fonts.smufl => fit_glyph_as(p, &c.to_string(), "\u{E530}", rect, 0.85, color),
        Art::Dynamic(_, text) => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, text, serif_italic(12.0), color);
        }
        Art::Glyphs(text, _) => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, text.chars().map(|_| '#').collect::<String>(), serif(11.0), color);
        }
        Art::Text(s) => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, s, serif(11.5), color);
        }
        Art::Italic(s) => {
            let size = if s.contains('\n') {
                10.0
            } else if s.len() > 4 {
                9.5
            } else {
                12.0
            };
            p.text(rect.center(), egui::Align2::CENTER_CENTER, s, serif_italic(size), color);
        }
        Art::Draw(f) => f(p, rect.shrink(5.0), color),
        Art::Blank => {}
    }
    if btn.options {
        let r = rect.shrink(2.0);
        p.add(Shape::convex_polygon(vec![r.right_bottom(), pos2(r.right(), r.bottom() - 5.0), pos2(r.right() - 5.0, r.bottom())], color, Stroke::NONE));
    }
    resp.on_hover_text(&btn.tip)
}

/// Draws SMuFL `text` centred in `r`, scaled so its inked extent fills `fill` of the button.
fn fit_glyph(p: &Painter, text: &str, r: Rect, fill: f32, color: Color32) {
    const PROBE: f32 = 40.0;
    let font = |size: f32| egui::FontId::new(size, egui::FontFamily::Name(crate::fonts::SMUFL.into()));
    let ink = |galley: &std::sync::Arc<egui::Galley>| galley.rows.first().map(|row| row.row.visuals.mesh_bounds.translate(row.pos.to_vec2()));
    let probe = p.layout_no_wrap(text.to_string(), font(PROBE), color);
    let Some(b) = ink(&probe).filter(|b| b.width() > 0.0 && b.height() > 0.0) else { return };
    let target = r.width().min(r.height()) * fill.clamp(0.05, 1.0);
    let size = (PROBE * target / b.width().max(b.height())).clamp(4.0, 200.0);
    let galley = p.layout_no_wrap(text.to_string(), font(size), color);
    let Some(b) = ink(&galley) else { return };
    p.galley(r.center() - b.center().to_vec2(), galley, color);
}

/// Draws SMuFL `text` at the size that makes `like` fill `fill` of the button, centred across
/// on its own ink and up and down on `like`'s, so a set of glyphs shares a size and a baseline.
fn fit_glyph_as(p: &Painter, text: &str, like: &str, r: Rect, fill: f32, color: Color32) {
    const PROBE: f32 = 40.0;
    let font = |size: f32| egui::FontId::new(size, egui::FontFamily::Name(crate::fonts::SMUFL.into()));
    let ink = |galley: &std::sync::Arc<egui::Galley>| galley.rows.first().map(|row| row.row.visuals.mesh_bounds.translate(row.pos.to_vec2()));
    let probe = p.layout_no_wrap(like.to_string(), font(PROBE), color);
    let Some(lb) = ink(&probe).filter(|b| b.width() > 0.0 && b.height() > 0.0) else { return };
    let target = r.width().min(r.height()) * fill.clamp(0.05, 1.0);
    let size = (PROBE * target / lb.width().max(lb.height())).clamp(4.0, 200.0);
    let reference = p.layout_no_wrap(like.to_string(), font(size), color);
    let galley = p.layout_no_wrap(text.to_string(), font(size), color);
    let (Some(rb), Some(b)) = (ink(&reference), ink(&galley)) else { return };
    p.galley(pos2(r.center().x - b.center().x, r.center().y - rb.center().y), galley, color);
}

/// A button two cells wide with an icon and a word, as Guitar Pro's Lyrics and Chords are.
fn wide_button(ui: &mut egui::Ui, text: &str, icon: DrawFn, tip: &str) -> egui::Response {
    let t = tokens();
    let (rect, resp) = ui.allocate_exact_size(vec2(3.5 * CELL, CELL * 0.85), egui::Sense::click());
    let fill = if resp.hovered() { t.control_hover } else { t.control };
    let p = ui.painter();
    p.rect_filled(rect, 5.0, fill);
    let galley = p.layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), t.text);
    let w = 14.0 + 4.0 + galley.size().x;
    let x = rect.center().x - w / 2.0;
    icon(p, Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(12.0, 14.0)), t.text);
    p.galley(pos2(x + 18.0, rect.center().y - galley.size().y / 2.0), galley, t.text);
    resp.on_hover_text(tip)
}

fn divider(ui: &mut egui::Ui) {
    ui.add_space(5.0);
    let r = ui.available_rect_before_wrap();
    ui.painter().line_segment([pos2(r.left() + 2.0, r.top()), pos2(r.right() - 2.0, r.top())], Stroke::new(1.0, tokens().text_dim.gamma_multiply(0.8)));
    ui.add_space(6.0);
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let fonts = app.fonts;
    let score = app.session.score().clone();
    let c = app.session.cursor();
    let voice = score.tracks.get(c.track).and_then(|t| t.measures.get(c.measure)).and_then(|m| m.voices.get(c.voice));
    let beat: Option<&Beat> = voice.and_then(|v| v.beats.get(c.beat));
    let note: Option<&Note> = beat.and_then(|b| b.note_on(c.string as u8));
    // With a selection, note tools apply to every note in it. Guitar techniques don't apply to drums.
    let drums = score.tracks.get(c.track).is_some_and(guitario_score::Track::is_drums);
    let header = score.headers.get(c.measure);
    let at: u32 = voice.map_or(0, |v| v.beats.iter().take(c.beat).map(|b| b.duration.ticks()).sum());
    let state = State {
        beat,
        note,
        header,
        has_note: (note.is_some() || app.session.selection().is_some()) && !drums,
        has_beat: beat.is_some(),
        fermata: header.is_some_and(|h| h.fermatas.iter().any(|f| f.tick == at)),
    };
    let mut acts: Vec<Act> = Vec::new();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        // The voices, multivoice, design mode.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.add_space(4.0);
            let t = tokens();
            for v in 0..4usize {
                let (rect, resp) = ui.allocate_exact_size(vec2(CELL, CELL), egui::Sense::click());
                let corners = match v {
                    0 => egui::CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 },
                    3 => egui::CornerRadius { nw: 0, sw: 0, ne: 5, se: 5 },
                    _ => egui::CornerRadius::ZERO,
                };
                let on = c.voice == v;
                let fill = if on {
                    VOICE_ON
                } else if resp.hovered() {
                    t.control_hover
                } else {
                    t.control
                };
                ui.painter().rect_filled(rect, corners, fill);
                if v > 0 {
                    ui.painter().line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(1.0, t.panel));
                }
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    (v + 1).to_string(),
                    crate::fonts::sans_bold(14.0),
                    if on { Color32::WHITE } else { t.text },
                );
                if resp.on_hover_text(format!("Voice {}", v + 1)).clicked() {
                    acts.push(run("edit.setVoice", json!({"voice": v + 1})));
                }
            }
            ui.add_space(8.0);
            let mv = b(Art::Draw(draw_multivoice), "Multivoice editing", app.view.multivoice, true, Act::Ui(UiAction::ToggleMultivoice)).boxed();
            if cell(ui, fonts, &mv).clicked() {
                acts.push(mv.act.clone());
            }
            ui.add_space(4.0);
            cell(ui, fonts, &later(Art::Draw(draw_design_mode), "Design mode").boxed());
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
            ui.add_space(12.0);
            if wide_button(ui, "Lyrics", draw_mic, "Lyrics of the track").clicked() {
                acts.push(Act::Window(Window::Lyrics));
            }
            if wide_button(ui, "Chords", draw_chord, "Chord diagram of the beat").clicked() {
                acts.push(Act::Window(Window::Chords));
            }
        });
        for (id, buttons) in sections(&state) {
            divider(ui);
            ui.push_id(id, |ui| {
                ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
                for row in rows(&buttons) {
                    ui.horizontal(|ui| {
                        for btn in row {
                            let r = cell(ui, fonts, btn);
                            match &btn.act {
                                Act::Pop(kind) => popup(&r, *kind, &state, &mut acts),
                                act if r.clicked() => acts.push(act.clone()),
                                _ => {}
                            }
                        }
                    });
                }
            });
        }
    });
    for act in acts {
        match act {
            Act::Run(id, params) => {
                app.run(id, params);
            }
            Act::Ui(a) => app.ui_action(a),
            Act::Window(w) => match w {
                Window::Lyrics => app.open_lyrics(),
                Window::Chords => app.open_chord(),
                Window::Trill => app.open_trill(),
                Window::Grace => app.open_grace(),
                Window::Fingering => app.open_fingering(),
                Window::Tremolo => app.open_tremolo(),
            },
            Act::Pop(_) | Act::Off => {}
        }
    }
}

/// The popup a button opens on click.
fn popup(r: &egui::Response, kind: Pop, s: &State, acts: &mut Vec<Act>) {
    let hd = s.header;
    egui::Popup::menu(r).show(|ui| match kind {
        Pop::Key => {
            let names = ["C♭", "G♭", "D♭", "A♭", "E♭", "B♭", "F", "C", "G", "D", "A", "E", "B", "F♯", "C♯"];
            for (i, n) in names.iter().enumerate() {
                let k = i as i64 - 7;
                let count = match k {
                    0 => "no accidentals".to_string(),
                    k if k > 0 => format!("{k}♯"),
                    k => format!("{}♭", -k),
                };
                if ui.selectable_label(hd.is_some_and(|h| i64::from(h.key) == k), format!("{n} major ({count})")).clicked() {
                    acts.push(run("measure.setKey", json!({"key": k})));
                    ui.close();
                }
            }
        }
        Pop::Time => {
            for (n, d) in [(2, 4), (3, 4), (4, 4), (5, 4), (6, 4), (7, 4), (3, 8), (5, 8), (6, 8), (7, 8), (9, 8), (12, 8)] {
                let on = hd.is_some_and(|h| h.time_signature.numerator == n && h.time_signature.denominator == d);
                if ui.selectable_label(on, format!("{n}/{d}")).clicked() {
                    acts.push(run("measure.setTimeSignature", json!({"numerator": n, "denominator": d})));
                    ui.close();
                }
            }
        }
        Pop::TripletFeel => {
            let feels = [
                (TripletFeel::None, "none", "No triplet feel"),
                (TripletFeel::Triplet8th, "triplet8th", "Triplet feel, eighths"),
                (TripletFeel::Triplet16th, "triplet16th", "Triplet feel, sixteenths"),
                (TripletFeel::Dotted8th, "dotted8th", "Dotted feel, eighths"),
                (TripletFeel::Dotted16th, "dotted16th", "Dotted feel, sixteenths"),
                (TripletFeel::Scottish8th, "scottish8th", "Scottish feel, eighths"),
                (TripletFeel::Scottish16th, "scottish16th", "Scottish feel, sixteenths"),
            ];
            for (f, id, label) in feels {
                if ui.selectable_label(hd.is_some_and(|h| h.triplet_feel == f), label).clicked() {
                    acts.push(run("measure.setTripletFeel", json!({"feel": id})));
                    ui.close();
                }
            }
        }
        Pop::Endings => {
            let have = hd.map_or(0, |h| h.endings);
            ui.label("Played on pass:");
            for n in 1..=8u8 {
                let bit = 1u8 << (n - 1);
                let mut on = have & bit != 0;
                if ui.checkbox(&mut on, format!("{n}.")).changed() {
                    let next = if on { have | bit } else { have & !bit };
                    let list: Vec<u8> = (1..=8u8).filter(|k| next & (1 << (k - 1)) != 0).collect();
                    acts.push(run("measure.setEndings", json!({"endings": list})));
                }
            }
        }
        Pop::RepeatClose => {
            let close = hd.map_or(0, |h| h.repeat_close);
            for (n, label) in [(1, "Repeat once (play twice)"), (2, "Play three times"), (3, "Play four times"), (5, "Play six times")] {
                if ui.selectable_label(close == n, label).clicked() {
                    acts.push(run("measure.setRepeat", json!({"close": n})));
                    ui.close();
                }
            }
            if ui.add_enabled(close > 0, egui::Button::new("Remove repeat close")).clicked() {
                acts.push(run("measure.setRepeat", json!({"close": 0})));
                ui.close();
            }
        }
        Pop::Directions => {
            ui.label("Targets");
            for (id, label) in [("segno", "Segno"), ("segnoSegno", "Segno segno"), ("coda", "Coda"), ("doubleCoda", "Double coda"), ("fine", "Fine")] {
                if ui.button(label).clicked() {
                    acts.push(run("measure.setDirections", json!({"targets": [id]})));
                    ui.close();
                }
            }
            ui.separator();
            ui.label("Jumps");
            for (id, label) in [
                ("daCapo", "Da Capo"),
                ("daCapoAlFine", "D.C. al Fine"),
                ("daCapoAlCoda", "D.C. al Coda"),
                ("dalSegno", "Dal Segno"),
                ("dalSegnoAlFine", "D.S. al Fine"),
                ("dalSegnoAlCoda", "D.S. al Coda"),
            ] {
                if ui.button(label).clicked() {
                    acts.push(run("measure.setDirections", json!({"jump": id})));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Remove directions").clicked() {
                acts.push(run("measure.setDirections", json!({"targets": [], "jump": null})));
                ui.close();
            }
        }
        Pop::Tuplet => {
            for n in [3u8, 5, 6, 7, 9, 10, 11, 12, 13] {
                if ui.button(format!("{n}-tuplet")).clicked() {
                    acts.push(run("beat.setTuplet", json!({"enters": n})));
                    ui.close();
                }
            }
            if ui.button("No tuplet").clicked() {
                acts.push(run("beat.setTuplet", json!({"enters": 1})));
                ui.close();
            }
        }
        Pop::Fermata => {
            for (kind, label) in [("short", "Short fermata"), ("medium", "Fermata"), ("long", "Long fermata")] {
                if ui.button(label).clicked() {
                    acts.push(run("beat.setFermata", json!({"kind": kind})));
                    ui.close();
                }
            }
            if ui.add_enabled(s.fermata, egui::Button::new("Remove fermata")).clicked() {
                acts.push(run("beat.setFermata", json!({"kind": "none"})));
                ui.close();
            }
        }
        Pop::Harmonic => {
            let now = s.note.and_then(|n| n.effects.harmonic);
            for (h, id, label) in [
                (Harmonic::Artificial, "artificial", "Artificial"),
                (Harmonic::Pinch, "pinch", "Pinch"),
                (Harmonic::Tapped, "tapped", "Tapped"),
                (Harmonic::Semi, "semi", "Semi"),
            ] {
                if ui.selectable_label(now == Some(h), label).clicked() {
                    acts.push(run("note.setHarmonic", json!({"harmonic": id})));
                    ui.close();
                }
            }
            if ui.add_enabled(now.is_some(), egui::Button::new("No harmonic")).clicked() {
                acts.push(run("note.setHarmonic", json!({"harmonic": "none"})));
                ui.close();
            }
        }
        Pop::Slur => {
            for (beats, label) in [(1, "Slur to the next beat"), (2, "Slur over two beats"), (3, "Slur over three beats"), (4, "Slur over four beats")] {
                if ui.button(label).clicked() {
                    acts.push(run("beat.setSlur", json!({"beats": beats})));
                    ui.close();
                }
            }
            if ui.add_enabled(s.beat.is_some_and(|b| b.slur.is_some()), egui::Button::new("Remove slur")).clicked() {
                acts.push(run("beat.setSlur", json!({"beats": null})));
                ui.close();
            }
        }
        Pop::Text => {
            let id = ui.make_persistent_id("palette-text");
            let mut text: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or_else(|| s.beat.and_then(|b| b.text.clone()).unwrap_or_default());
            let edit = ui.text_edit_singleline(&mut text);
            ui.data_mut(|d| d.insert_temp(id, text.clone()));
            if ui.button("Set text").clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                acts.push(run("beat.setText", json!({"text": text})));
                ui.data_mut(|d| d.remove::<String>(id));
                ui.close();
            }
        }
    });
}

fn st(c: Color32) -> Stroke {
    Stroke::new(1.5, c)
}

/// Points in `r` from fractions of its size.
fn at(r: Rect) -> impl Fn(f32, f32) -> egui::Pos2 {
    move |x, y| pos2(r.left() + x * r.width(), r.top() + y * r.height())
}

/// A quarter note, stem up, with a mark under its head, as Guitar Pro's articulation buttons.
fn note_mark(p: &Painter, r: Rect, c: Color32, mark: fn(&Painter, egui::Pos2, Color32)) {
    let a = at(r);
    p.add(Shape::ellipse_filled(a(0.42, 0.52), vec2(3.6, 2.6), c));
    p.line_segment([a(0.57, 0.5), a(0.57, -0.05)], st(c));
    mark(p, a(0.45, 0.92), c);
}

fn draw_accent(p: &Painter, r: Rect, c: Color32) {
    note_mark(p, r, c, |p, m, c| {
        p.add(Shape::line(vec![m + vec2(-4.0, -2.5), m + vec2(4.0, 0.0), m + vec2(-4.0, 2.5)], Stroke::new(1.3, c)));
    });
}

fn draw_heavy_accent(p: &Painter, r: Rect, c: Color32) {
    note_mark(p, r, c, |p, m, c| {
        p.add(Shape::line(vec![m + vec2(-3.5, 2.5), m + vec2(0.0, -3.0), m + vec2(3.5, 2.5)], Stroke::new(1.6, c)));
    });
}

fn draw_staccato(p: &Painter, r: Rect, c: Color32) {
    note_mark(p, r, c, |p, m, c| {
        p.circle_filled(m, 1.8, c);
    });
}

fn draw_golpe_finger(p: &Painter, r: Rect, c: Color32) {
    note_mark(p, Rect::from_min_max(pos2(r.left(), r.top() + 6.0), r.max), c, |_, _, _| {});
    p.text(pos2(r.center().x, r.top() + 1.0), egui::Align2::CENTER_CENTER, "*", egui::FontId::proportional(11.0), c);
}

fn draw_golpe_thumb(p: &Painter, r: Rect, c: Color32) {
    note_mark(p, Rect::from_min_max(r.min, pos2(r.right(), r.bottom() - 6.0)), c, |_, _, _| {});
    p.text(pos2(r.center().x - 1.0, r.bottom() + 1.0), egui::Align2::CENTER_CENTER, "*", egui::FontId::proportional(11.0), c);
}

fn draw_ghost(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.add(Shape::ellipse_filled(a(0.45, 0.62), vec2(3.4, 2.5), c));
    p.line_segment([a(0.6, 0.6), a(0.6, 0.08)], st(c));
    // Round brackets either side, as Guitar Pro marks a ghost note.
    for (cx, from) in [(0.32, 2.2f32), (0.68, -0.95f32)] {
        let pts = (0..=8).map(|i| from + 1.9 * i as f32 / 8.0).map(|t| a(cx, 0.5) + vec2(t.cos() * r.width() * 0.32, t.sin() * r.height() * 0.48)).collect();
        p.add(Shape::line(pts, Stroke::new(1.2, c)));
    }
}

/// One thick slanted stroke, as Guitar Pro's tremolo picking button.
fn draw_tremolo(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.add(Shape::convex_polygon(vec![a(0.1, 0.62), a(0.9, 0.32), a(0.9, 0.5), a(0.1, 0.8)], c, Stroke::NONE));
}

/// Guitar Pro's wah marks: a pedal under an open circle or a plus.
fn wah(p: &Painter, r: Rect, c: Color32, open: bool) {
    let a = at(r);
    p.add(Shape::convex_polygon(vec![a(0.1, 0.95), a(0.9, 0.95), a(0.9, 0.75), a(0.1, 0.62)], c, Stroke::NONE));
    if open {
        p.circle_stroke(a(0.5, 0.25), 3.5, Stroke::new(1.3, c));
    } else {
        p.line_segment([a(0.2, 0.25), a(0.8, 0.25)], Stroke::new(1.5, c));
        p.line_segment([a(0.5, 0.0), a(0.5, 0.5)], Stroke::new(1.5, c));
    }
}

fn draw_wah_open(p: &Painter, r: Rect, c: Color32) {
    wah(p, r, c, true);
}

fn draw_wah_closed(p: &Painter, r: Rect, c: Color32) {
    wah(p, r, c, false);
}

fn draw_rest(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.add(Shape::line(vec![a(0.4, 0.0), a(0.65, 0.3), a(0.4, 0.55), a(0.65, 0.8), a(0.45, 0.78), a(0.55, 1.0)], Stroke::new(2.0, c)));
}

fn draw_dot(p: &Painter, r: Rect, c: Color32) {
    p.circle_filled(r.center(), 2.6, c);
}

fn draw_double_dot(p: &Painter, r: Rect, c: Color32) {
    p.circle_filled(r.center() - vec2(4.0, 0.0), 2.4, c);
    p.circle_filled(r.center() + vec2(4.0, 0.0), 2.4, c);
}

/// A bracket with a label in its middle, as tuplets are marked.
fn bracket(p: &Painter, r: Rect, c: Color32, label: &str) {
    let y = r.center().y;
    let galley = p.layout_no_wrap(label.to_string(), egui::FontId::proportional(9.0), c);
    let half = galley.size().x / 2.0 + 2.0;
    let s = Stroke::new(1.2, c);
    p.line_segment([pos2(r.left(), y + 3.0), pos2(r.left(), y)], s);
    p.line_segment([pos2(r.left(), y), pos2(r.center().x - half, y)], s);
    p.line_segment([pos2(r.center().x + half, y), pos2(r.right(), y)], s);
    p.line_segment([pos2(r.right(), y), pos2(r.right(), y + 3.0)], s);
    p.galley(pos2(r.center().x - galley.size().x / 2.0, y - galley.size().y / 2.0), galley, c);
}

fn draw_triplet(p: &Painter, r: Rect, c: Color32) {
    bracket(p, r, c, "3");
}

fn draw_tuplet(p: &Painter, r: Rect, c: Color32) {
    bracket(p, r.expand2(vec2(2.0, 0.0)), c, "n:m");
}

fn draw_nested_tuplet(p: &Painter, r: Rect, c: Color32) {
    bracket(p, r.expand2(vec2(2.0, 0.0)), c, "n:m");
    p.line_segment([pos2(r.left() - 2.0, r.center().y - 4.0), pos2(r.right() + 2.0, r.center().y - 4.0)], Stroke::new(1.0, c));
}

/// Two quarter notes tied under their heads.
fn draw_tie(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    for x in [0.2, 0.75] {
        p.add(Shape::ellipse_filled(a(x, 0.62), vec2(3.0, 2.2), c));
        p.line_segment([a(x + 0.13, 0.6), a(x + 0.13, 0.0)], Stroke::new(1.2, c));
    }
    g::arc(p, a(0.22, 0.85), a(0.75, 0.85), 2.5, Stroke::new(1.4, c));
}

fn draw_ties(p: &Painter, r: Rect, c: Color32) {
    for dy in [-4.0, 0.0, 4.0] {
        g::arc(p, pos2(r.left(), r.center().y + dy), pos2(r.right(), r.center().y + dy), 3.0, Stroke::new(1.2, c));
    }
}

fn arrow(p: &Painter, from: egui::Pos2, to: egui::Pos2, c: Color32) {
    p.line_segment([from, to], st(c));
    let d = (to - from).normalized();
    let n = vec2(-d.y, d.x);
    p.add(Shape::convex_polygon(vec![to + d * 2.0, to - d * 3.5 + n * 3.0, to - d * 3.5 - n * 3.0], c, Stroke::NONE));
}

/// A notehead with a stem, and an arrow beside it.
fn note_with_arrow(p: &Painter, r: Rect, c: Color32, up: bool) {
    let a = at(r);
    p.add(Shape::ellipse_filled(a(0.35, 0.78), vec2(3.6, 2.6), c));
    p.line_segment([a(0.5, 0.75), a(0.5, 0.05)], st(c));
    if up {
        arrow(p, a(0.85, 0.85), a(0.85, 0.15), c);
    } else {
        arrow(p, a(0.85, 0.15), a(0.85, 0.85), c);
    }
}

fn draw_semitone_down(p: &Painter, r: Rect, c: Color32) {
    note_with_arrow(p, r, c, false);
}

fn draw_semitone_up(p: &Painter, r: Rect, c: Color32) {
    note_with_arrow(p, r, c, true);
}

fn draw_crescendo(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.add(Shape::line(vec![a(1.0, 0.25), a(0.0, 0.5), a(1.0, 0.75)], st(c)));
}

fn draw_decrescendo(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.add(Shape::line(vec![a(0.0, 0.25), a(1.0, 0.5), a(0.0, 0.75)], st(c)));
}

fn draw_swell(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.add(Shape::closed_line(vec![a(0.0, 0.5), a(0.5, 0.25), a(1.0, 0.5), a(0.5, 0.75)], st(c)));
}

fn draw_dead(p: &Painter, r: Rect, c: Color32) {
    let q = Rect::from_center_size(r.center(), vec2(9.0, 9.0));
    let s = Stroke::new(2.6, c);
    p.line_segment([q.left_top(), q.right_bottom()], s);
    p.line_segment([q.right_top(), q.left_bottom()], s);
}

fn draw_dead_slap(p: &Painter, r: Rect, c: Color32) {
    let q = r.shrink(1.0);
    let s = Stroke::new(1.8, c);
    p.line_segment([q.left_top(), q.right_bottom()], s);
    p.line_segment([q.right_top(), q.left_bottom()], s);
}

fn diamond(p: &Painter, r: Rect, c: Color32, filled: bool) {
    let cc = r.center();
    let pts = vec![cc + vec2(0.0, -5.0), cc + vec2(6.0, 0.0), cc + vec2(0.0, 5.0), cc + vec2(-6.0, 0.0)];
    if filled {
        p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
    } else {
        p.add(Shape::closed_line(pts, st(c)));
    }
}

fn draw_harmonic_natural(p: &Painter, r: Rect, c: Color32) {
    diamond(p, r, c, false);
}

fn draw_harmonic_artificial(p: &Painter, r: Rect, c: Color32) {
    diamond(p, r, c, true);
}

fn draw_bend(p: &Painter, r: Rect, c: Color32) {
    let a = r.left_bottom();
    let tip = pos2(r.right() - 3.0, r.top() + 2.0);
    p.add(egui::epaint::QuadraticBezierShape::from_points_stroke([a, pos2(tip.x, a.y), tip], false, Color32::TRANSPARENT, st(c)));
    p.add(Shape::convex_polygon(vec![tip - vec2(0.0, 3.0), tip + vec2(-3.0, 2.0), tip + vec2(3.0, 2.0)], c, Stroke::NONE));
}

fn draw_whammy(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.add(Shape::line(vec![a(0.0, 0.35), a(0.5, 0.9), a(1.0, 0.35)], st(c)));
    p.text(a(0.5, 0.25), egui::Align2::CENTER_CENTER, "x", egui::FontId::proportional(10.0), c);
}

fn draw_vibrato(p: &Painter, r: Rect, c: Color32) {
    g::wavy(p, r.left(), r.right(), r.center().y, 2.5, st(c));
}

fn draw_wide_vibrato(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    let pts: Vec<_> = (0..=6).map(|i| a(i as f32 / 6.0, if i % 2 == 0 { 0.3 } else { 0.7 })).collect();
    p.add(Shape::line(pts, st(c)));
}

fn draw_bridge_vibrato(p: &Painter, r: Rect, c: Color32) {
    g::wavy(p, r.left() + 4.0, r.right(), r.center().y + 2.0, 2.0, st(c));
    p.text(r.left_top() + vec2(2.0, 3.0), egui::Align2::CENTER_CENTER, "x", egui::FontId::proportional(9.0), c);
}

fn draw_bridge_vibrato_wide(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    let pts: Vec<_> = (0..=5).map(|i| a(0.2 + 0.8 * i as f32 / 5.0, if i % 2 == 0 { 0.45 } else { 0.85 })).collect();
    p.add(Shape::line(pts, st(c)));
    p.text(a(0.12, 0.2), egui::Align2::CENTER_CENTER, "x", egui::FontId::proportional(9.0), c);
}

/// Two fret numbers with a line between, rising or falling, for slides.
fn slide_between(p: &Painter, r: Rect, c: Color32, legato: bool) {
    let a = at(r);
    let f = egui::FontId::proportional(8.5);
    p.text(a(0.08, 0.75), egui::Align2::CENTER_CENTER, "1", f.clone(), c);
    p.text(a(0.92, 0.55), egui::Align2::CENTER_CENTER, "3", f, c);
    p.line_segment([a(0.25, 0.8), a(0.75, 0.55)], Stroke::new(1.2, c));
    if legato {
        g::arc(p, a(0.1, 0.3), a(0.9, 0.3), 3.0, Stroke::new(1.0, c));
    }
}

fn draw_slide_legato(p: &Painter, r: Rect, c: Color32) {
    slide_between(p, r, c, true);
}

fn draw_slide_shift(p: &Painter, r: Rect, c: Color32) {
    slide_between(p, r, c, false);
}

fn slide_one(p: &Painter, r: Rect, c: Color32, number_right: bool, rising: bool) {
    let a = at(r);
    let f = egui::FontId::proportional(8.5);
    let (nx, l0, l1) = if number_right { (0.88, 0.0, 0.65) } else { (0.12, 0.35, 1.0) };
    p.text(a(nx, 0.6), egui::Align2::CENTER_CENTER, "3", f, c);
    let (y0, y1) = if rising { (0.85, 0.45) } else { (0.45, 0.85) };
    p.line_segment([a(l0, y0), a(l1, y1)], Stroke::new(1.2, c));
}

fn draw_slide_in_below(p: &Painter, r: Rect, c: Color32) {
    slide_one(p, r, c, true, true);
}

fn draw_slide_in_above(p: &Painter, r: Rect, c: Color32) {
    slide_one(p, r, c, true, false);
}

fn draw_slide_out_down(p: &Painter, r: Rect, c: Color32) {
    slide_one(p, r, c, false, false);
}

fn draw_slide_out_up(p: &Painter, r: Rect, c: Color32) {
    slide_one(p, r, c, false, true);
}

fn draw_hopo(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.text(a(0.5, 0.15), egui::Align2::CENTER_CENTER, "H", serif_italic(9.5), c);
    g::arc(p, a(0.05, 0.75), a(0.95, 0.75), 4.0, Stroke::new(1.2, c));
}

fn draw_lh_tapping(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.text(a(0.5, 0.15), egui::Align2::CENTER_CENTER, "H/P", serif_italic(8.5), c);
    g::arc(p, a(0.05, 0.7), a(0.95, 0.7), 4.0, Stroke::new(1.2, c));
    p.text(a(0.1, 0.95), egui::Align2::CENTER_CENTER, "1", egui::FontId::proportional(7.5), c);
    p.text(a(0.9, 0.95), egui::Align2::CENTER_CENTER, "3", egui::FontId::proportional(7.5), c);
}

/// An open hand: a palm and four fingers, the thumb out to one side.
fn hand(p: &Painter, r: Rect, c: Color32, thumb_left: bool) {
    let a = at(r);
    p.rect_filled(Rect::from_min_max(a(0.2, 0.45), a(0.8, 1.0)), 2.5, c);
    for (i, h) in [0.2, 0.05, 0.1, 0.25].iter().enumerate() {
        let x = 0.25 + i as f32 * 0.15;
        p.rect_filled(Rect::from_min_max(a(x, *h), a(x + 0.11, 0.6)), 1.5, c);
    }
    let (x0, x1) = if thumb_left { (0.0, 0.25) } else { (0.75, 1.0) };
    p.add(Shape::convex_polygon(vec![a(x0, 0.45), a(x1, 0.6), a(x1, 0.85), a(x0, 0.6)], c, Stroke::NONE));
}

fn draw_hand_left(p: &Painter, r: Rect, c: Color32) {
    hand(p, r, c, false);
}

fn draw_hand_right(p: &Painter, r: Rect, c: Color32) {
    hand(p, r, c, true);
}

fn draw_string_number(p: &Painter, r: Rect, c: Color32) {
    p.circle_stroke(r.center(), r.height().min(r.width()) / 2.0, Stroke::new(1.2, c));
    p.text(r.center(), egui::Align2::CENTER_CENTER, "6", egui::FontId::proportional(10.0), c);
}

fn draw_brush_up(p: &Painter, r: Rect, c: Color32) {
    arrow(p, pos2(r.center().x, r.bottom()), pos2(r.center().x, r.top() + 2.0), c);
}

fn draw_brush_down(p: &Painter, r: Rect, c: Color32) {
    arrow(p, pos2(r.center().x, r.top()), pos2(r.center().x, r.bottom() - 2.0), c);
}

/// A wavy vertical line ending in an arrowhead.
fn arpeggio(p: &Painter, r: Rect, c: Color32, up: bool) {
    let x = r.center().x;
    let (y0, y1) = if up { (r.bottom(), r.top() + 4.0) } else { (r.top(), r.bottom() - 4.0) };
    let pts: Vec<_> = (0..=12)
        .map(|i| {
            let t = i as f32 / 12.0;
            pos2(x + 2.0 * (t * std::f32::consts::TAU * 2.0).sin(), y0 + (y1 - y0) * t)
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(1.3, c)));
    let tip = pos2(x, if up { r.top() } else { r.bottom() });
    let d = if up { -1.0 } else { 1.0 };
    p.add(Shape::convex_polygon(vec![tip, tip - vec2(3.0, 4.5 * d), tip + vec2(3.0, -4.5 * d)], c, Stroke::NONE));
}

fn draw_arpeggio_up(p: &Painter, r: Rect, c: Color32) {
    arpeggio(p, r, c, true);
}

fn draw_arpeggio_down(p: &Painter, r: Rect, c: Color32) {
    arpeggio(p, r, c, false);
}

/// A small eighth note, slashed for a grace note before the beat.
fn grace(p: &Painter, r: Rect, c: Color32, slashed: bool) {
    let a = at(r);
    p.add(Shape::ellipse_filled(a(0.35, 0.82), vec2(3.4, 2.4), c));
    p.line_segment([a(0.5, 0.8), a(0.5, 0.1)], st(c));
    p.add(Shape::line(vec![a(0.5, 0.1), a(0.75, 0.35), a(0.72, 0.55)], st(c)));
    if slashed {
        p.line_segment([a(0.25, 0.55), a(0.8, 0.2)], Stroke::new(1.2, c));
    }
}

fn draw_grace_before(p: &Painter, r: Rect, c: Color32) {
    grace(p, r, c, true);
}

fn draw_grace_on(p: &Painter, r: Rect, c: Color32) {
    grace(p, r, c, false);
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

fn draw_endings(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.add(Shape::line(vec![a(0.05, 0.9), a(0.05, 0.15), a(1.0, 0.15)], st(c)));
    p.text(a(0.45, 0.55), egui::Align2::CENTER_CENTER, "1.", serif(11.0), c);
}

fn draw_free_time(p: &Painter, r: Rect, c: Color32) {
    g::dashed(p, pos2(r.center().x, r.top()), pos2(r.center().x, r.bottom()), Stroke::new(1.5, c), 2.5);
}

fn draw_double_bar(p: &Painter, r: Rect, c: Color32) {
    for dx in [-2.5, 2.5] {
        p.line_segment([pos2(r.center().x + dx, r.top()), pos2(r.center().x + dx, r.bottom())], st(c));
    }
}

fn draw_time_sig(p: &Painter, r: Rect, c: Color32) {
    p.text(pos2(r.center().x, r.center().y - 5.0), egui::Align2::CENTER_CENTER, "4", crate::fonts::sans_bold(12.0), c);
    p.text(pos2(r.center().x, r.center().y + 6.0), egui::Align2::CENTER_CENTER, "4", crate::fonts::sans_bold(12.0), c);
}

/// Two beamed eighths under a triplet bracket.
fn draw_triplet_feel(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    for x in [0.3, 0.75] {
        p.add(Shape::ellipse_filled(a(x - 0.12, 0.88), vec2(2.8, 2.0), c));
        p.line_segment([a(x, 0.88), a(x, 0.4)], Stroke::new(1.2, c));
    }
    p.line_segment([a(0.3, 0.4), a(0.75, 0.4)], Stroke::new(2.4, c));
    bracket(p, Rect::from_min_max(a(0.0, 0.0), a(1.0, 0.25)), c, "3");
}

/// Two stems joined by a beam.
fn draw_beam(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.line_segment([a(0.3, 0.2), a(0.3, 0.8)], Stroke::new(1.5, c));
    p.line_segment([a(0.7, 0.2), a(0.7, 0.8)], Stroke::new(1.5, c));
    p.line_segment([a(0.3, 0.2), a(0.7, 0.2)], Stroke::new(4.0, c));
    p.add(Shape::ellipse_filled(a(0.2, 0.82), vec2(2.8, 2.0), c));
    p.add(Shape::ellipse_filled(a(0.6, 0.82), vec2(2.8, 2.0), c));
}

fn draw_beam_auto(p: &Painter, r: Rect, c: Color32) {
    draw_beam(p, Rect::from_min_max(r.min, pos2(r.right(), r.bottom() - 4.0)), c);
    p.text(pos2(r.center().x, r.bottom() + 2.0), egui::Align2::CENTER_CENTER, "AUTO", egui::FontId::proportional(6.0), c);
}

fn draw_beam_break(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    for x in [0.25, 0.75] {
        p.add(Shape::ellipse_filled(a(x - 0.1, 0.82), vec2(2.8, 2.0), c));
        p.line_segment([a(x, 0.8), a(x, 0.15)], Stroke::new(1.5, c));
        p.add(Shape::line(vec![a(x, 0.15), a(x + 0.18, 0.35)], Stroke::new(1.5, c)));
    }
}

fn draw_beam_secondary(p: &Painter, r: Rect, c: Color32) {
    draw_beam(p, r, c);
    let a = at(r);
    p.line_segment([a(0.3, 0.35), a(0.42, 0.35)], Stroke::new(2.0, c));
    p.line_segment([a(0.58, 0.35), a(0.7, 0.35)], Stroke::new(2.0, c));
}

fn stem_note(p: &Painter, r: Rect, c: Color32, up: bool) {
    let a = at(r);
    let head = if up { a(0.45, 0.8) } else { a(0.55, 0.2) };
    p.add(Shape::ellipse_filled(head, vec2(3.4, 2.4), c));
    if up {
        p.line_segment([a(0.6, 0.78), a(0.6, 0.1)], st(c));
    } else {
        p.line_segment([a(0.4, 0.22), a(0.4, 0.9)], st(c));
    }
}

fn draw_stem_auto(p: &Painter, r: Rect, c: Color32) {
    stem_note(p, Rect::from_min_max(r.min, pos2(r.right(), r.bottom() - 4.0)), c, true);
    p.text(pos2(r.center().x, r.bottom() + 2.0), egui::Align2::CENTER_CENTER, "AUTO", egui::FontId::proportional(6.0), c);
}

fn draw_stem_up(p: &Painter, r: Rect, c: Color32) {
    stem_note(p, r, c, true);
    arrow(p, pos2(r.left() + 2.0, r.bottom()), pos2(r.left() + 2.0, r.top() + 3.0), c);
}

fn draw_stem_down(p: &Painter, r: Rect, c: Color32) {
    stem_note(p, r, c, false);
    arrow(p, pos2(r.right() - 1.0, r.top()), pos2(r.right() - 1.0, r.bottom() - 3.0), c);
}

fn draw_tuplet_groups(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    for x0 in [0.0, 0.55] {
        p.add(Shape::line(vec![a(x0, 0.45), a(x0, 0.25), a(x0 + 0.45, 0.25), a(x0 + 0.45, 0.45)], Stroke::new(1.2, c)));
        p.add(Shape::convex_polygon(vec![a(x0 + 0.1, 0.6), a(x0 + 0.35, 0.6), a(x0 + 0.225, 0.85)], c, Stroke::NONE));
    }
}

/// A chord diagram: a grid with a dot or two.
fn draw_chord(p: &Painter, r: Rect, c: Color32) {
    let s = Stroke::new(1.0, c);
    for i in 0..4 {
        let x = r.left() + r.width() * i as f32 / 3.0;
        p.line_segment([pos2(x, r.top() + 2.0), pos2(x, r.bottom())], s);
    }
    for i in 0..4 {
        let y = r.top() + 2.0 + (r.height() - 2.0) * i as f32 / 3.0;
        p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], s);
    }
    p.line_segment([r.left_top() + vec2(0.0, 1.0), r.right_top() + vec2(0.0, 1.0)], Stroke::new(2.0, c));
    p.circle_filled(pos2(r.left() + r.width() / 3.0, r.top() + 2.0 + (r.height() - 2.0) / 2.0), 1.8, c);
}

fn draw_scale(p: &Painter, r: Rect, c: Color32) {
    let s = Stroke::new(0.8, c);
    for i in 0..6 {
        let x = r.left() + r.width() * i as f32 / 5.0;
        p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], s);
    }
    for i in 0..5 {
        let y = r.top() + r.height() * i as f32 / 4.0;
        p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], s);
    }
    for (i, j) in [(0, 1), (2, 1), (4, 2), (1, 3), (3, 3)] {
        p.circle_filled(pos2(r.left() + r.width() * i as f32 / 5.0, r.top() + r.height() * (j as f32 - 0.5) / 4.0), 1.5, c);
    }
}

fn draw_slash(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.add(Shape::convex_polygon(vec![a(0.15, 0.95), a(0.3, 0.95), a(0.55, 0.65), a(0.4, 0.65)], c, Stroke::NONE));
    p.line_segment([a(0.53, 0.68), a(0.53, 0.05)], st(c));
}

fn draw_section(p: &Painter, r: Rect, c: Color32) {
    let q = Rect::from_center_size(r.center(), vec2(14.0, 14.0));
    p.rect_stroke(q, 2.0, Stroke::new(1.2, c), egui::StrokeKind::Middle);
    p.text(q.center(), egui::Align2::CENTER_CENTER, "A", serif(11.0), c);
}

/// Bars rising left to right, as Guitar Pro draws volume automations; `master` adds an M.
fn level_bars(p: &Painter, r: Rect, c: Color32, master: bool, many: bool) {
    let n = if many { 6 } else { 4 };
    let w = r.width() / (n as f32 * 1.6);
    for i in 0..n {
        let h = r.height() * (0.3 + 0.6 * i as f32 / (n - 1) as f32) * if master { 0.75 } else { 1.0 };
        let x = r.left() + i as f32 * w * 1.6;
        p.rect_filled(Rect::from_min_max(pos2(x, r.bottom() - h), pos2(x + w, r.bottom())), 0.0, c);
    }
    if master {
        p.text(pos2(r.center().x, r.top() + 1.0), egui::Align2::CENTER_CENTER, "M", crate::fonts::sans_bold(8.0), c);
    }
}

fn draw_master_volume(p: &Painter, r: Rect, c: Color32) {
    level_bars(p, r, c, true, false);
}

fn draw_master_pan(p: &Painter, r: Rect, c: Color32) {
    level_bars(p, r, c, true, true);
}

fn draw_track_volume(p: &Painter, r: Rect, c: Color32) {
    level_bars(p, r, c, false, false);
}

fn draw_track_pan(p: &Painter, r: Rect, c: Color32) {
    level_bars(p, r, c, false, true);
}

fn draw_tempo(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.add(Shape::ellipse_filled(a(0.25, 0.8), vec2(3.4, 2.4), c));
    p.line_segment([a(0.4, 0.78), a(0.4, 0.05)], st(c));
    p.text(a(0.78, 0.6), egui::Align2::CENTER_CENTER, "=", egui::FontId::proportional(12.0), c);
}

/// Two notes with a slur over their stems, as Guitar Pro's legato button.
fn draw_slur(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    for x in [0.22, 0.72] {
        p.add(Shape::ellipse_filled(a(x, 0.85), vec2(3.0, 2.2), c));
        p.line_segment([a(x + 0.13, 0.83), a(x + 0.13, 0.3)], Stroke::new(1.2, c));
    }
    g::arc(p, a(0.3, 0.22), a(0.9, 0.22), -3.5, Stroke::new(1.4, c));
}

/// Stacked notes on a staff, for Multivoice editing.
fn draw_multivoice(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    for i in 0..4 {
        p.line_segment([a(0.0, 0.2 + i as f32 * 0.2), a(1.0, 0.2 + i as f32 * 0.2)], Stroke::new(0.7, c));
    }
    p.add(Shape::ellipse_filled(a(0.35, 0.4), vec2(2.8, 2.0), c));
    p.line_segment([a(0.47, 0.4), a(0.47, 0.0)], Stroke::new(1.2, c));
    p.add(Shape::ellipse_filled(a(0.65, 0.8), vec2(2.8, 2.0), c));
    p.line_segment([a(0.53, 0.8), a(0.53, 1.0)], Stroke::new(1.2, c));
}

/// A double-headed arrow over a downward triangle.
fn draw_design_mode(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    arrow(p, a(0.3, 0.15), a(0.0, 0.15), c);
    arrow(p, a(0.7, 0.15), a(1.0, 0.15), c);
    p.add(Shape::convex_polygon(vec![a(0.15, 0.4), a(0.85, 0.4), a(0.5, 0.95)], c, Stroke::NONE));
}

fn draw_mic(p: &Painter, r: Rect, c: Color32) {
    let a = at(r);
    p.rect_filled(Rect::from_min_max(a(0.3, 0.0), a(0.7, 0.6)), 3.0, c);
    p.add(Shape::line(vec![a(0.15, 0.45), a(0.2, 0.7), a(0.5, 0.8), a(0.8, 0.7), a(0.85, 0.45)], Stroke::new(1.2, c)));
    p.line_segment([a(0.5, 0.8), a(0.5, 1.0)], Stroke::new(1.2, c));
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    fn state(s: &Session) -> (std::sync::Arc<guitario_score::Score>, guitario_engine::Cursor) {
        (std::sync::Arc::clone(s.score()), s.cursor())
    }

    /// A section's buttons as (tip, lit, enabled).
    type Seen = (&'static str, Vec<(String, bool, bool)>);

    fn sections_at(s: &Session) -> Vec<Seen> {
        let (score, c) = state(s);
        let beat = score.tracks.get(c.track).and_then(|t| t.measures.get(c.measure)).and_then(|m| m.voices.get(c.voice)).and_then(|v| v.beats.get(c.beat));
        let note = beat.and_then(|b| b.note_on(c.string as u8));
        let st = State { beat, note, header: score.headers.get(c.measure), has_note: note.is_some(), has_beat: beat.is_some(), fermata: false };
        sections(&st).into_iter().map(|(id, bs)| (id, bs.into_iter().map(|b| (b.tip, b.on, b.enabled)).collect())).collect()
    }

    #[test]
    fn sections_match_guitar_pros_rows() {
        let s = Session::new();
        let secs = sections_at(&s);
        let names: Vec<_> = secs.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["bar", "note", "effects", "notation", "automations"]);
        let (score, c) = state(&s);
        let st = State { beat: None, note: None, header: score.headers.get(c.measure), has_note: false, has_beat: false, fermata: false };
        let counts: Vec<usize> = sections(&st).iter().map(|(_, b)| rows(b).len()).collect();
        // Bar 2 rows, note 5, effects 7, notation 2, automations 1, as in Guitar Pro 8's palette.
        assert_eq!(counts, [2, 5, 7, 2, 1]);
        for (id, bs) in &secs {
            if *id == "bar" || *id == "notation" {
                assert_eq!(bs.len() % 8, 0, "{id} fills whole rows");
            }
        }
    }

    #[test]
    fn wide_buttons_take_two_cells() {
        let one = || b(Art::Blank, "", false, false, Act::Off);
        // Six single cells and a wide one fill a row; the next wide one starts a new row.
        let mut buttons: Vec<Button> = (0..6).map(|_| one()).collect();
        buttons.push(one().wide());
        buttons.push(one().wide());
        buttons.push(one());
        let lens: Vec<usize> = rows(&buttons).iter().map(|r| r.len()).collect();
        assert_eq!(lens, [7, 2]);
        for row in rows(&buttons) {
            assert!(row.iter().map(|b| b.span).sum::<usize>() <= COLUMNS as usize);
        }
        assert!(rows(&[]).is_empty());
        // The hairpins share the last note row, two cells each, as in Guitar Pro.
        let s = Session::new();
        let (score, c) = state(&s);
        let st = State { beat: None, note: None, header: score.headers.get(c.measure), has_note: false, has_beat: false, fermata: false };
        let secs = sections(&st);
        let note = secs.iter().find(|(id, _)| *id == "note").map(|(_, b)| b).unwrap();
        let last = rows(note).last().copied().unwrap();
        assert_eq!(last.iter().map(|b| b.span).collect::<Vec<_>>(), [2, 2]);
    }

    #[test]
    fn unbuilt_symbols_are_dimmed_and_say_so() {
        let secs = sections_at(&Session::new());
        for (_, bs) in &secs {
            for (tip, _, enabled) in bs {
                if tip.ends_with(LATER) {
                    assert!(!enabled, "{tip}");
                }
            }
        }
        let tips: Vec<&String> = secs.iter().flat_map(|(_, b)| b.iter().map(|(t, _, _)| t)).collect();
        assert!(tips.iter().any(|t| t.starts_with("Clef") && t.ends_with(LATER)));
        assert!(tips.iter().any(|t| t.as_str() == "Key signature"));
    }

    #[test]
    fn note_symbols_follow_the_cursor_note() {
        let mut s = Session::new();
        let find = |secs: &[Seen], tip: &str| secs.iter().flat_map(|(_, b)| b.iter()).find(|(t, _, _)| t.starts_with(tip)).cloned().unwrap();
        // No note under the cursor: note symbols are dimmed, bar symbols are not.
        let secs = sections_at(&s);
        assert!(!find(&secs, "Palm mute").2);
        assert!(find(&secs, "Repeat start").2);
        s.run("note.setFret", &json!({"fret": 5})).unwrap();
        s.run("note.toggleEffect", &json!({"effect": "palmMute"})).unwrap();
        let secs = sections_at(&s);
        let pm = find(&secs, "Palm mute");
        assert!(pm.1 && pm.2, "lit and enabled");
        assert!(!find(&secs, "Ghost note").1);
    }

    #[test]
    fn every_enabled_command_runs_at_the_cursor() {
        let fresh = || {
            let mut s = Session::new();
            s.run("note.setFret", &json!({"fret": 5})).unwrap();
            s
        };
        let s = fresh();
        let (score, c) = state(&s);
        let beat = score.tracks[c.track].measures[c.measure].voices[c.voice].beats.get(c.beat);
        let note = beat.and_then(|b| b.note_on(c.string as u8));
        let st = State { beat, note, header: score.headers.get(c.measure), has_note: true, has_beat: true, fermata: false };
        for (_, buttons) in sections(&st) {
            for btn in buttons {
                if let (true, Act::Run(id, params)) = (btn.enabled, &btn.act) {
                    // A refusal the user can read (a whole note cannot be dotted in 4/4) is fine;
                    // an unknown command or a parameter the command does not take is a wiring bug.
                    let mut probe = fresh();
                    let r = probe.run(id, params);
                    assert!(
                        !matches!(
                            r,
                            Err(guitario_engine::EngineError::UnknownCommand(_)
                                | guitario_engine::EngineError::BadParam(_)
                                | guitario_engine::EngineError::Internal { .. })
                        ),
                        "{} → {id} {params}: {r:?}",
                        btn.tip
                    );
                }
            }
        }
    }
}
