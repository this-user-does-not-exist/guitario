//! Design tokens. Colours follow Guitar Pro's dark editor: charcoal chrome, an LCD transport
//! display, white pages on a grey workspace and a blue edit cursor. Widgets read these instead of
//! hard-coding colours.

use egui::Color32;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

/// Guitar Pro's dark editor, or a light one; chosen in Preferences and saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

/// View › Score Colors: the pages' colours on screen. Prints and exports are always paper.
/// Guitar Pro has no such setting; it is a Guitario addition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreColours {
    /// The score's own inks on white, as Guitar Pro draws it.
    #[default]
    Paper,
    /// Light inks on a charcoal page.
    Dark,
    /// Dark pages in the dark theme, paper in the light one.
    FollowTheme,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Switches the tokens every widget reads; call `apply` after it so egui's own widgets follow.
pub fn set_theme(theme: Theme) {
    CURRENT.store(theme as u8, Ordering::Relaxed);
}

pub fn theme() -> Theme {
    if CURRENT.load(Ordering::Relaxed) == Theme::Light as u8 { Theme::Light } else { Theme::Dark }
}

pub struct Tokens {
    pub chrome: Color32,
    pub chrome_dark: Color32,
    pub panel: Color32,
    pub panel_header: Color32,
    pub border: Color32,
    pub control: Color32,
    pub control_hover: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub lcd_bg: Color32,
    pub lcd_text: Color32,
    pub lcd_dim: Color32,
    pub workspace: Color32,
    pub page: Color32,
    pub ink: Color32,
    pub staff_line: Color32,
    pub bar_line: Color32,
    pub bar_number: Color32,
    /// The bar number of a bar whose length does not match its time signature: Guitar Pro marks
    /// notation errors in red on screen.
    pub bar_error: Color32,
    pub cursor_fill: Color32,
    pub cursor_stroke: Color32,
    pub selection_fill: Color32,
    pub play_bar: Color32,
    pub play_beat: Color32,
    pub effect_ink: Color32,
    /// Edit › Multivoice Editing draws each voice in its own colour, Voice 1 first.
    pub voice_ink: [Color32; 4],
    pub neck: Color32,
    pub fret_wire: Color32,
    pub string: Color32,
    pub inlay: Color32,
    /// The selected track's row in the global view.
    pub row_selected: Color32,
    /// The document tab.
    pub tab: Color32,
}

/// Guitar Pro 8's dark scheme, sampled from the manual's screenshots: blue-grey panels and
/// buttons, a near-black LCD and fields, a slate workspace around off-white pages.
pub const GP: Tokens = Tokens {
    chrome: Color32::from_rgb(0x29, 0x2a, 0x33),
    chrome_dark: Color32::from_rgb(0x1a, 0x1c, 0x22),
    panel: Color32::from_rgb(0x29, 0x2a, 0x33),
    panel_header: Color32::from_rgb(0x1a, 0x1c, 0x22),
    border: Color32::from_rgb(0x16, 0x17, 0x1d),
    control: Color32::from_rgb(0x52, 0x57, 0x71),
    control_hover: Color32::from_rgb(0x64, 0x6a, 0x88),
    text: Color32::from_rgb(0xee, 0xf0, 0xf6),
    text_dim: Color32::from_rgb(0x9a, 0x9d, 0xad),
    accent: Color32::from_rgb(0x56, 0x8f, 0xff),
    lcd_bg: Color32::from_rgb(0x0c, 0x0e, 0x12),
    lcd_text: Color32::from_rgb(0xd8, 0xe9, 0xf5),
    lcd_dim: Color32::from_rgb(0x6f, 0x80, 0x8c),
    workspace: Color32::from_rgb(0x44, 0x47, 0x51),
    page: Color32::from_rgb(0xf6, 0xf6, 0xf6),
    ink: Color32::from_rgb(0x14, 0x14, 0x14),
    staff_line: Color32::from_rgb(0x9a, 0x9a, 0x9a),
    bar_line: Color32::from_rgb(0x50, 0x50, 0x50),
    bar_number: Color32::from_rgb(0x8c, 0x8c, 0x8c),
    bar_error: Color32::from_rgb(0xd0, 0x30, 0x30),
    cursor_fill: Color32::from_rgba_premultiplied(0x18, 0x48, 0x80, 0x30),
    cursor_stroke: Color32::from_rgb(0x2f, 0x7b, 0xf5),
    selection_fill: Color32::from_rgba_premultiplied(0x10, 0x3c, 0x78, 0x48),
    play_bar: Color32::from_rgba_premultiplied(0x1a, 0x14, 0x00, 0x1a),
    play_beat: Color32::from_rgba_premultiplied(0x60, 0x40, 0x00, 0x60),
    effect_ink: Color32::from_rgb(0x30, 0x30, 0x30),
    voice_ink: [
        Color32::from_rgb(0x1d, 0x5f, 0xc4),
        Color32::from_rgb(0x2a, 0x8a, 0x3a),
        Color32::from_rgb(0xc8, 0x6a, 0x10),
        Color32::from_rgb(0xa8, 0x2c, 0x8e),
    ],
    neck: Color32::from_rgb(0x2e, 0x34, 0x3d),
    fret_wire: Color32::from_rgb(0xc8, 0xcd, 0xd5),
    string: Color32::from_rgb(0x95, 0x9c, 0xb0),
    inlay: Color32::from_rgb(0xab, 0xb8, 0xbd),
    row_selected: Color32::from_rgb(0x3d, 0x3f, 0x4c),
    tab: Color32::from_rgb(0x82, 0x8f, 0xb2),
};

/// The light editor: pale chrome and panels around the same white pages. The transport display
/// stays dark, as Guitar Pro's does.
pub const LIGHT: Tokens = Tokens {
    chrome: Color32::from_rgb(0xe9, 0xe9, 0xec),
    chrome_dark: Color32::from_rgb(0xdb, 0xdb, 0xdf),
    panel: Color32::from_rgb(0xf1, 0xf1, 0xf3),
    panel_header: Color32::from_rgb(0xe3, 0xe3, 0xe7),
    border: Color32::from_rgb(0xb4, 0xb4, 0xba),
    control: Color32::from_rgb(0xdd, 0xdd, 0xe2),
    control_hover: Color32::from_rgb(0xcd, 0xcd, 0xd4),
    text: Color32::from_rgb(0x1f, 0x1f, 0x22),
    text_dim: Color32::from_rgb(0x66, 0x66, 0x6e),
    workspace: Color32::from_rgb(0xb6, 0xb6, 0xbb),
    page: Color32::WHITE,
    row_selected: Color32::from_rgb(0xd8, 0xda, 0xe3),
    tab: Color32::from_rgb(0xb3, 0xbd, 0xd6),
    ..GP
};

/// What prints and exports use whatever the theme: the score's own inks on white paper.
/// Paper shows no notation errors.
pub const PAPER: Tokens = Tokens { page: Color32::WHITE, bar_error: GP.bar_number, ..GP };

/// Light inks on a charcoal page, for the dark editor. The highlights are pale tints instead of
/// dark ones, and the workspace is a step darker so the sheets keep their edges.
pub const DARK_PAGE: Tokens = Tokens {
    workspace: Color32::from_rgb(0x34, 0x36, 0x3e),
    page: Color32::from_rgb(0x1e, 0x1f, 0x24),
    ink: Color32::from_rgb(0xe6, 0xe6, 0xe8),
    staff_line: Color32::from_rgb(0x74, 0x76, 0x7e),
    bar_line: Color32::from_rgb(0xa8, 0xaa, 0xb0),
    bar_number: Color32::from_rgb(0x8c, 0x8e, 0x96),
    bar_error: Color32::from_rgb(0xff, 0x6b, 0x6b),
    cursor_fill: Color32::from_rgba_premultiplied(0x12, 0x28, 0x48, 0x48),
    cursor_stroke: Color32::from_rgb(0x5c, 0x9d, 0xff),
    selection_fill: Color32::from_rgba_premultiplied(0x10, 0x26, 0x48, 0x48),
    play_bar: Color32::from_rgba_premultiplied(0x16, 0x12, 0x04, 0x16),
    play_beat: Color32::from_rgba_premultiplied(0x48, 0x38, 0x0c, 0x50),
    effect_ink: Color32::from_rgb(0xcf, 0xd0, 0xd4),
    voice_ink: [
        Color32::from_rgb(0x6e, 0xa8, 0xff),
        Color32::from_rgb(0x5c, 0xcf, 0x6e),
        Color32::from_rgb(0xff, 0xa2, 0x4a),
        Color32::from_rgb(0xe7, 0x7a, 0xd0),
    ],
    ..GP
};

/// Dark pages in the light editor: its chrome, a workspace a step darker than its own.
pub const LIGHT_DARK_PAGE: Tokens = Tokens {
    workspace: Color32::from_rgb(0x9a, 0x9a, 0xa0),
    page: DARK_PAGE.page,
    ink: DARK_PAGE.ink,
    staff_line: DARK_PAGE.staff_line,
    bar_line: DARK_PAGE.bar_line,
    bar_number: DARK_PAGE.bar_number,
    bar_error: DARK_PAGE.bar_error,
    cursor_fill: DARK_PAGE.cursor_fill,
    cursor_stroke: DARK_PAGE.cursor_stroke,
    selection_fill: DARK_PAGE.selection_fill,
    play_bar: DARK_PAGE.play_bar,
    play_beat: DARK_PAGE.play_beat,
    effect_ink: DARK_PAGE.effect_ink,
    voice_ink: DARK_PAGE.voice_ink,
    ..LIGHT
};

/// The tokens the editing view paints its pages with.
pub fn page_tokens(theme: Theme, colours: ScoreColours) -> &'static Tokens {
    match (theme, colours) {
        (Theme::Dark, ScoreColours::Dark | ScoreColours::FollowTheme) => &DARK_PAGE,
        (Theme::Light, ScoreColours::Dark) => &LIGHT_DARK_PAGE,
        (Theme::Dark, ScoreColours::Paper) => &GP,
        (Theme::Light, ScoreColours::Paper | ScoreColours::FollowTheme) => &LIGHT,
    }
}

pub fn tokens() -> &'static Tokens {
    match theme() {
        Theme::Dark => &GP,
        Theme::Light => &LIGHT,
    }
}

/// The colour of track `i`: its own, or the next of Guitar Pro's track colours.
pub fn track_color(score: &guitario_score::Score, i: usize) -> Color32 {
    let default = || guitario_score::TRACK_COLORS.get(i % guitario_score::TRACK_COLORS.len()).copied().unwrap_or([0xe0, 0x4f, 0x4f]);
    let rgb = score.tracks.get(i).map_or_else(default, |t| t.color_at(i));
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Guitar Pro's text field: the near-black well with rounded corners and a thin outline.
pub fn field_frame() -> egui::Frame {
    let t = tokens();
    let (fill, edge) = if theme() == Theme::Light { (Color32::WHITE, t.border) } else { (t.lcd_bg, Color32::from_rgb(0x4a, 0x4d, 0x5c)) };
    egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0, edge)).corner_radius(5).inner_margin(egui::Margin::symmetric(6, 3))
}

pub fn apply(ctx: &egui::Context) {
    let t = tokens();
    let mut v = if theme() == Theme::Light { egui::Visuals::light() } else { egui::Visuals::dark() };
    v.panel_fill = t.panel;
    v.window_fill = t.panel;
    // Text fields are near-black in Guitar Pro's dark scheme.
    v.extreme_bg_color = if theme() == Theme::Light { t.chrome_dark } else { t.lcd_bg };
    v.faint_bg_color = t.panel_header;
    v.override_text_color = Some(t.text);
    v.selection.bg_fill = t.accent;
    v.selection.stroke = egui::Stroke::new(1.0, if theme() == Theme::Light { t.ink } else { Color32::WHITE });
    v.window_stroke = egui::Stroke::new(1.0, t.border);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, t.border);
    v.widgets.noninteractive.bg_fill = t.panel;
    v.widgets.inactive.bg_fill = t.control;
    v.widgets.inactive.weak_bg_fill = t.control;
    v.widgets.hovered.bg_fill = t.control_hover;
    v.widgets.hovered.weak_bg_fill = t.control_hover;
    v.widgets.active.bg_fill = t.accent;
    v.widgets.active.weak_bg_fill = t.accent;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = egui::CornerRadius::same(3);
    }
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
        s.spacing.interact_size.y = 22.0;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2 contrast ratio between two opaque colours.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let lum = |c: Color32| {
            let lin = |v: u8| {
                let v = f32::from(v) / 255.0;
                if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// A premultiplied tint laid over `under`.
    fn over(tint: Color32, under: Color32) -> Color32 {
        let keep = 1.0 - f32::from(tint.a()) / 255.0;
        let mix = |t: u8, u: u8| (f32::from(t) + f32::from(u) * keep).round().min(255.0) as u8;
        Color32::from_rgb(mix(tint.r(), under.r()), mix(tint.g(), under.g()), mix(tint.b(), under.b()))
    }

    /// Every ink against the page, with the lowest ratio each must keep.
    fn floors(t: &Tokens, ink: f32, lines: f32, colour: f32) -> Vec<(&'static str, Color32, f32)> {
        let mut v = vec![
            ("ink", t.ink, ink),
            ("effect_ink", t.effect_ink, ink),
            ("staff_line", t.staff_line, lines),
            ("bar_line", t.bar_line, lines),
            ("bar_number", t.bar_number, lines),
            ("bar_error", t.bar_error, colour),
            ("cursor_stroke", t.cursor_stroke, lines),
        ];
        v.extend(t.voice_ink.iter().zip(["voice 1", "voice 2", "voice 3", "voice 4"]).map(|(&c, n)| (n, c, colour)));
        v
    }

    fn check(name: &str, t: &Tokens, floors: &[(&str, Color32, f32)]) {
        for &(what, c, min) in floors {
            let r = contrast(c, t.page);
            assert!(r >= min, "{name}: {what} is {r:.2}:1 on the page, below {min}:1");
        }
        // Each highlight shows on the page, and the ink still reads through it.
        for (what, tint) in [("cursor", t.cursor_fill), ("selection", t.selection_fill), ("play bar", t.play_bar), ("play beat", t.play_beat)] {
            let lit = over(tint, t.page);
            assert!(lit != t.page, "{name}: the {what} tint vanishes on the page");
            let r = contrast(t.ink, lit);
            assert!(r >= 4.5, "{name}: ink is {r:.2}:1 under the {what}");
        }
    }

    #[test]
    fn dark_pages_meet_the_contrast_targets() {
        for (name, t) in [("dark page", &DARK_PAGE), ("dark page, light theme", &LIGHT_DARK_PAGE)] {
            check(name, t, &floors(t, 7.0, 3.0, 4.5));
            assert!(contrast(t.workspace, t.page) >= 1.3, "{name}: the sheets melt into the workspace");
        }
    }

    /// Paper keeps the ratios Guitar Pro's sampled colours have today: a regression guard, not a
    /// target (its staff lines and two voice inks sit below the dark page's).
    #[test]
    fn paper_keeps_its_contrast() {
        for (name, t) in [("dark theme", &GP), ("light theme", &LIGHT), ("print", &PAPER)] {
            check(name, t, &floors(t, 7.0, 2.5, 3.3));
        }
    }

    #[test]
    fn dark_pages_change_only_the_score() {
        assert_eq!(DARK_PAGE.chrome, GP.chrome);
        assert_eq!(LIGHT_DARK_PAGE.chrome, LIGHT.chrome);
        assert_eq!(LIGHT_DARK_PAGE.text, LIGHT.text);
        assert_ne!(DARK_PAGE.workspace, GP.workspace);
        assert_ne!(LIGHT_DARK_PAGE.workspace, LIGHT.workspace);
    }

    #[test]
    fn follow_theme_is_dark_only_in_the_dark_theme() {
        use ScoreColours as S;
        let is = |t: &Tokens, want: &Tokens| (t.page, t.workspace, t.chrome) == (want.page, want.workspace, want.chrome);
        assert!(is(page_tokens(Theme::Dark, S::Paper), &GP));
        assert!(is(page_tokens(Theme::Light, S::Paper), &LIGHT));
        assert!(is(page_tokens(Theme::Dark, S::Dark), &DARK_PAGE));
        assert!(is(page_tokens(Theme::Light, S::Dark), &LIGHT_DARK_PAGE));
        assert!(is(page_tokens(Theme::Dark, S::FollowTheme), &DARK_PAGE));
        assert!(is(page_tokens(Theme::Light, S::FollowTheme), &LIGHT));
    }
}
