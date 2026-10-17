//! Design tokens. Colours follow Guitar Pro's dark editor: charcoal chrome, an LCD transport
//! display, white pages on a grey workspace and a blue edit cursor. Widgets read these instead of
//! hard-coding colours.

use egui::Color32;

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
}

pub const GP: Tokens = Tokens {
    chrome: Color32::from_rgb(0x30, 0x30, 0x32),
    chrome_dark: Color32::from_rgb(0x24, 0x24, 0x26),
    panel: Color32::from_rgb(0x3a, 0x3a, 0x3d),
    panel_header: Color32::from_rgb(0x2b, 0x2b, 0x2e),
    border: Color32::from_rgb(0x1c, 0x1c, 0x1e),
    control: Color32::from_rgb(0x48, 0x48, 0x4c),
    control_hover: Color32::from_rgb(0x58, 0x58, 0x5d),
    text: Color32::from_rgb(0xe4, 0xe4, 0xe6),
    text_dim: Color32::from_rgb(0x9c, 0x9c, 0xa2),
    accent: Color32::from_rgb(0x2f, 0x8c, 0xf0),
    lcd_bg: Color32::from_rgb(0x15, 0x17, 0x19),
    lcd_text: Color32::from_rgb(0xd8, 0xe9, 0xf5),
    lcd_dim: Color32::from_rgb(0x6f, 0x80, 0x8c),
    workspace: Color32::from_rgb(0x6c, 0x6c, 0x70),
    page: Color32::WHITE,
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
    neck: Color32::from_rgb(0x3b, 0x27, 0x1c),
    fret_wire: Color32::from_rgb(0xb8, 0xb8, 0xbc),
    string: Color32::from_rgb(0xd9, 0xcf, 0xb8),
    inlay: Color32::from_rgb(0xe8, 0xe2, 0xd2),
};

pub fn tokens() -> &'static Tokens {
    &GP
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
    let mut v = egui::Visuals::dark();
    v.panel_fill = t.panel;
    v.window_fill = t.panel;
    v.extreme_bg_color = t.chrome_dark;
    v.faint_bg_color = t.panel_header;
    v.override_text_color = Some(t.text);
    v.selection.bg_fill = t.accent;
    v.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
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
