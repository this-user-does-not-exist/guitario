//! Fonts used on the score page. Nothing is bundled (project rule: never commit font files). At
//! start-up we look for a SMuFL music font (Bravura or Leland, both SIL OFL) and for the serif and
//! sans faces Guitar Pro's default stylesheet uses (Times New Roman, Arial). Missing fonts fall
//! back to egui's own fonts and to painter-drawn music symbols.

use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FontSet {
    /// A SMuFL music font is loaded under [`SMUFL`].
    pub smufl: bool,
}

pub const SMUFL: &str = "gc-smufl";
pub const SERIF: &str = "gc-serif";
pub const SERIF_ITALIC: &str = "gc-serif-italic";
pub const SERIF_BOLD: &str = "gc-serif-bold";
pub const SANS: &str = "gc-sans";
pub const SANS_BOLD: &str = "gc-sans-bold";
pub const SERIF_BOLD_ITALIC: &str = "gc-serif-bold-italic";
pub const SANS_ITALIC: &str = "gc-sans-italic";
pub const SANS_BOLD_ITALIC: &str = "gc-sans-bold-italic";
const MAX_FONT_BYTES: u64 = 16 * 1024 * 1024;

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in ["GUITARIO_FONTS_DIR", "CRAFT_FONTS_DIR"] {
        if let Some(d) = std::env::var_os(var) {
            dirs.push(PathBuf::from(d));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(Path::new(&home).join("Library/Fonts"));
        dirs.push(Path::new(&home).join(".local/share/fonts"));
        dirs.push(Path::new(&home).join(".fonts"));
    }
    for d in [
        "/Library/Fonts",
        "/System/Library/Fonts/Supplemental",
        "/System/Library/Fonts",
        "/usr/share/fonts/opentype/bravura",
        "/usr/share/fonts/truetype/msttcorefonts",
        "/usr/share/fonts/truetype/liberation",
        "/usr/share/fonts",
        "C:\\Windows\\Fonts",
    ] {
        dirs.push(PathBuf::from(d));
    }
    if let Some(windir) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(Path::new(&windir).join("Microsoft\\Windows\\Fonts"));
    }
    dirs
}

/// The first readable font among `names` in the search directories (one level of subfolders).
fn find(names: &[&str]) -> Option<Vec<u8>> {
    if let Some(p) = std::env::var_os("GUITARIO_MUSIC_FONT").filter(|_| names.contains(&"Bravura.otf")) {
        return read_capped(Path::new(&p));
    }
    for dir in search_dirs() {
        for name in names {
            if let Some(b) = read_capped(&dir.join(name)) {
                return Some(b);
            }
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for e in entries.flatten().take(500) {
                    if e.path().is_dir()
                        && let Some(b) = read_capped(&e.path().join(name))
                    {
                        return Some(b);
                    }
                }
            }
        }
    }
    None
}

fn read_capped(p: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(p).ok()?;
    (meta.is_file() && meta.len() <= MAX_FONT_BYTES).then(|| std::fs::read(p).ok()).flatten()
}

/// The page font families (always) and the music font (when found), as egui definitions. The PDF
/// export embeds the very same fonts.
pub fn definitions() -> (egui::FontDefinitions, FontSet) {
    let mut defs = egui::FontDefinitions::default();
    let fallback = defs.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    let mut set = FontSet::default();

    let family = |defs: &mut egui::FontDefinitions, key: &str, files: &[&str]| {
        let mut chain = Vec::new();
        if let Some(bytes) = find(files) {
            defs.font_data.insert(key.to_string(), Arc::new(egui::FontData::from_owned(bytes)));
            chain.push(key.to_string());
        }
        chain.extend(fallback.iter().cloned());
        defs.families.insert(egui::FontFamily::Name(key.into()), chain);
    };
    family(&mut defs, SERIF, &["Times New Roman.ttf", "times.ttf", "LiberationSerif-Regular.ttf", "DejaVuSerif.ttf"]);
    family(&mut defs, SERIF_ITALIC, &["Times New Roman Italic.ttf", "timesi.ttf", "LiberationSerif-Italic.ttf", "DejaVuSerif-Italic.ttf"]);
    family(&mut defs, SERIF_BOLD, &["Times New Roman Bold.ttf", "timesbd.ttf", "LiberationSerif-Bold.ttf", "DejaVuSerif-Bold.ttf"]);
    family(&mut defs, SANS, &["Arial.ttf", "arial.ttf", "LiberationSans-Regular.ttf", "DejaVuSans.ttf"]);
    family(&mut defs, SANS_BOLD, &["Arial Bold.ttf", "arialbd.ttf", "LiberationSans-Bold.ttf", "DejaVuSans-Bold.ttf"]);
    // Headers and footers may set any text bold, italic or both.
    family(&mut defs, SERIF_BOLD_ITALIC, &["Times New Roman Bold Italic.ttf", "timesbi.ttf", "LiberationSerif-BoldItalic.ttf", "DejaVuSerif-BoldItalic.ttf"]);
    family(&mut defs, SANS_ITALIC, &["Arial Italic.ttf", "ariali.ttf", "LiberationSans-Italic.ttf", "DejaVuSans-Oblique.ttf"]);
    family(&mut defs, SANS_BOLD_ITALIC, &["Arial Bold Italic.ttf", "arialbi.ttf", "LiberationSans-BoldItalic.ttf", "DejaVuSans-BoldOblique.ttf"]);

    if let Some(bytes) = find(&["Bravura.otf", "Leland.otf"]) {
        defs.font_data.insert(SMUFL.to_string(), Arc::new(egui::FontData::from_owned(bytes)));
        defs.families.insert(egui::FontFamily::Name(SMUFL.into()), vec![SMUFL.to_string()]);
        set.smufl = true;
    }
    (defs, set)
}

/// Registers the page font families (always) and the music font (when found).
pub fn install(ctx: &egui::Context) -> FontSet {
    let (defs, set) = definitions();
    ctx.set_fonts(defs);
    set
}

pub fn serif(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(SERIF.into()))
}

/// Italic serif for expression text (Fine, D.C., D.S.); the upright serif's fallback chain applies.
pub fn serif_italic(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(SERIF_ITALIC.into()))
}

/// Bold serif for tempo marks and section names, as Guitar Pro's default stylesheet sets them.
pub fn serif_bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(SERIF_BOLD.into()))
}

pub fn sans(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(SANS.into()))
}

/// Bold sans for the toolbar's LCD (falls back to egui's own font).
pub fn sans_bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(SANS_BOLD.into()))
}

/// Whether a font named in a stylesheet is set in the sans face (Arial and its kind) rather than
/// the serif one: the page has those two families, so every named font maps to the closer.
pub fn is_sans(name: &str) -> bool {
    let name = name.to_lowercase();
    ["sans", "arial", "helvetica", "verdana", "tahoma", "segoe", "calibri", "futura", "gill", "avenir"].iter().any(|k| name.contains(k))
}

/// The face for a header or footer text: the font's family, bold and italic, at `size` pixels.
pub fn page_text(font: &str, size: f32, bold: bool, italic: bool) -> egui::FontId {
    let family = match (is_sans(font), bold, italic) {
        (false, false, false) => SERIF,
        (false, true, false) => SERIF_BOLD,
        (false, false, true) => SERIF_ITALIC,
        (false, true, true) => SERIF_BOLD_ITALIC,
        (true, false, false) => SANS,
        (true, true, false) => SANS_BOLD,
        (true, false, true) => SANS_ITALIC,
        (true, true, true) => SANS_BOLD_ITALIC,
    };
    egui::FontId::new(size, egui::FontFamily::Name(family.into()))
}
