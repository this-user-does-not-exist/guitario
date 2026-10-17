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
    bar_number: Color32::from_rgb(0xc8, 0x3a, 0x3a),
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
pub const PAPER: Tokens = Tokens { page: Color32::WHITE, ..GP };

pub fn tokens() -> &'static Tokens {
    match theme() {
        Theme::Dark => &GP,
        Theme::Light => &LIGHT,
    }
}

/// Guitar Pro-like track colours, by track index.
pub fn track_color(i: usize) -> Color32 {
    const C: [Color32; 8] = [
        Color32::from_rgb(0xe0, 0x4f, 0x4f),
        Color32::from_rgb(0x3f, 0x9b, 0xe8),
        Color32::from_rgb(0x5c, 0xbd, 0x5c),
        Color32::from_rgb(0xe8, 0xa2, 0x3a),
        Color32::from_rgb(0xa8, 0x6b, 0xe0),
        Color32::from_rgb(0x3c, 0xc4, 0xb8),
        Color32::from_rgb(0xe0, 0x6b, 0xb5),
        Color32::from_rgb(0xc4, 0xc4, 0x48),
    ];
    C.get(i % C.len()).copied().unwrap_or(Color32::GRAY)
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
