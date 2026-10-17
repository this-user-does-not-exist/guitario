//! PDF export of the page view.
//!
//! The pages are not drawn a second time. The same engraving code that paints the page on screen
//! ([`crate::page::paint_print_page`]) paints each page into a headless egui context, and the
//! shapes it records (lines, rectangles, curves, glyph runs) become PDF drawing operations. Text
//! stays text: every glyph is placed where egui laid it out and drawn from an embedded subset of
//! the font that supplied it, so the output is vector, selectable and as sharp as the printer.
//!
//! Fonts come from the same definitions the page uses ([`crate::fonts::definitions`]). A font whose
//! licence flags forbid embedding is not embedded; the next font in the family that has the
//! character is used, and a character no font has is reported and left out.

use crate::fonts;
use crate::page::PrintSet;
use craft_pdf::write::{self, Metadata, WriteError, Writer, backend as k};
use egui::epaint::{ColorMode, PathShape, PathStroke, RectShape, StrokeKind, TextShape};
use egui::{Color32, FontFamily, LayerId, Pos2, RawInput, Rect, Shape, vec2};
use guitario_engine::{PageRenderer, PdfDocument, PdfRequest};
use guitario_score::Score;
use k::color::rgb;
use k::geom::{Path, PathBuilder, Point, Transform};
use k::num::NormalizedF32;
use k::paint::{Fill, FillRule, Stroke as KStroke};
use skrifa::{FontRef, MetadataProvider, raw::TableProvider};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

/// Layout pixels (1/96 inch) to PDF points (1/72 inch).
const PX_TO_PT: f32 = 0.75;
/// Text is laid out at this many physical pixels per point, so glyphs keep sub-pixel positions.
const LAYOUT_SCALE: f32 = 8.0;
/// A document longer than this is refused: choose fewer tracks.
const MAX_PAGES: usize = 2_000;
/// Circle control-point distance for a cubic Bézier quarter arc.
const KAPPA: f32 = 0.552_284_8;

/// Draws scores on A4 pages and writes them as PDF. Register it with
/// [`guitario_engine::Session::set_renderer`] to enable `file.exportPdf`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRenderer;

impl PageRenderer for PdfRenderer {
    fn render_pdf(&self, score: &Score, request: &PdfRequest) -> Result<PdfDocument, String> {
        write::guard(|| export(score, request)).map_err(|e: ExportError| e.0)
    }
}

/// A failure, as the sentence the user reads.
#[derive(Debug)]
struct ExportError(String);

impl From<WriteError> for ExportError {
    fn from(e: WriteError) -> Self {
        ExportError(e.to_string())
    }
}

fn export(score: &Score, request: &PdfRequest) -> Result<PdfDocument, ExportError> {
    let pages = PrintSet::new(score, &request.tracks, request.notation, request.together).map_err(ExportError)?;
    let total = pages.pages();
    if total > MAX_PAGES {
        return Err(ExportError(format!("the score would take {total} pages; print fewer tracks (the limit is {MAX_PAGES} pages)")));
    }

    // A context whose fonts are the page's fonts, at a fine pixel grid. Fonts apply from the next pass.
    let (defs, fonts) = fonts::definitions();
    let ctx = egui::Context::default();
    ctx.set_fonts(defs.clone());
    ctx.set_pixels_per_point(LAYOUT_SCALE);
    ctx.run_ui(RawInput::default(), |_| {}).textures_delta.clear();

    let meta = &score.meta;
    let author = [meta.artist.as_str(), meta.author.as_str()].into_iter().find(|s| !s.trim().is_empty());
    let metadata = Metadata {
        creator: "Guitario".into(),
        producer: "Guitario".into(),
        title: Some(if meta.title.trim().is_empty() { "Untitled".to_string() } else { meta.title.clone() }),
        authors: author.map(|a| vec![a.to_string()]).unwrap_or_default(),
        ..Metadata::default()
    };
    let mut writer = Writer::new(k::SerializeSettings::default(), metadata);
    let mut bank = FontBank::new(defs);
    let mut warnings = BTreeSet::new();
    for n in 0..total {
        let Some((layout, _)) = pages.page(n) else { continue };
        let m = layout.metrics;
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(m.page_w, m.page_h))), ..RawInput::default() };
        ctx.begin_pass(raw);
        let painter = ctx.layer_painter(LayerId::background());
        pages.paint(&painter, score, n, fonts);
        // Nothing is uploaded to a GPU, so the texture changes are discarded.
        let mut out = ctx.end_pass();
        out.textures_delta.clear();
        let shapes = out.shapes;

        let settings = k::page::PageSettings::from_wh(m.page_w * PX_TO_PT, m.page_h * PX_TO_PT).ok_or_else(|| ExportError("the page has no size".into()))?;
        let mut page = writer.document_mut().start_page_with(settings);
        let mut surface = page.surface();
        surface.push_transform(&Transform::from_scale(PX_TO_PT, PX_TO_PT));
        let mut conv = Converter { bank: &mut bank, warnings: &mut warnings };
        for clipped in &shapes {
            conv.shape(&mut surface, &clipped.shape);
        }
        surface.pop();
        surface.finish();
        page.finish();
    }
    warnings.extend(bank.warnings);
    let bytes = writer.finish()?;
    Ok(PdfDocument { bytes, pages: total, warnings: warnings.into_iter().collect() })
}

fn color(c: Color32) -> Option<(rgb::Color, NormalizedF32)> {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    (a > 0).then(|| (rgb::Color::new(r, g, b), NormalizedF32::new(f32::from(a) / 255.0).unwrap_or(NormalizedF32::ONE)))
}

fn fill_of(c: Color32) -> Option<Fill> {
    color(c).map(|(rgb, opacity)| Fill { paint: rgb.into(), opacity, rule: FillRule::NonZero })
}

fn stroke_of(width: f32, c: Color32) -> Option<KStroke> {
    if !(width.is_finite() && width > 0.0) {
        return None;
    }
    color(c).map(|(rgb, opacity)| KStroke { paint: rgb.into(), width, opacity, ..KStroke::default() })
}

/// A polyline, closed or not; `None` for fewer than two finite points.
fn polyline(points: &[Pos2], closed: bool) -> Option<Path> {
    let mut b = PathBuilder::new();
    let mut it = points.iter().filter(|p| p.x.is_finite() && p.y.is_finite());
    let first = it.next()?;
    b.move_to(first.x, first.y);
    let mut n = 1;
    for p in it {
        b.line_to(p.x, p.y);
        n += 1;
    }
    if n < 2 {
        return None;
    }
    if closed {
        b.close();
    }
    b.finish()
}

fn ellipse(c: Pos2, rx: f32, ry: f32) -> Option<Path> {
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let mut b = PathBuilder::new();
    b.move_to(c.x + rx, c.y);
    b.cubic_to(c.x + rx, c.y + ky, c.x + kx, c.y + ry, c.x, c.y + ry);
    b.cubic_to(c.x - kx, c.y + ry, c.x - rx, c.y + ky, c.x - rx, c.y);
    b.cubic_to(c.x - rx, c.y - ky, c.x - kx, c.y - ry, c.x, c.y - ry);
    b.cubic_to(c.x + kx, c.y - ry, c.x + rx, c.y - ky, c.x + rx, c.y);
    b.close();
    b.finish()
}

/// A rectangle, rounded when any corner has a radius.
fn rounded_rect(r: Rect, radii: [f32; 4]) -> Option<Path> {
    let [nw, ne, se, sw] = radii.map(|x| x.clamp(0.0, r.width().min(r.height()) / 2.0));
    let mut b = PathBuilder::new();
    b.move_to(r.left() + nw, r.top());
    b.line_to(r.right() - ne, r.top());
    b.cubic_to(r.right() - ne * (1.0 - KAPPA), r.top(), r.right(), r.top() + ne * (1.0 - KAPPA), r.right(), r.top() + ne);
    b.line_to(r.right(), r.bottom() - se);
    b.cubic_to(r.right(), r.bottom() - se * (1.0 - KAPPA), r.right() - se * (1.0 - KAPPA), r.bottom(), r.right() - se, r.bottom());
    b.line_to(r.left() + sw, r.bottom());
    b.cubic_to(r.left() + sw * (1.0 - KAPPA), r.bottom(), r.left(), r.bottom() - sw * (1.0 - KAPPA), r.left(), r.bottom() - sw);
    b.line_to(r.left(), r.top() + nw);
    b.cubic_to(r.left(), r.top() + nw * (1.0 - KAPPA), r.left() + nw * (1.0 - KAPPA), r.top(), r.left() + nw, r.top());
    b.close();
    b.finish()
}

/// Fonts prepared for embedding, and the glyph each character maps to.
struct FontBank {
    defs: egui::FontDefinitions,
    /// By font key; `None` when the font cannot be embedded.
    embedded: HashMap<String, Option<k::text::Font>>,
    glyphs: HashMap<(String, char), Option<u32>>,
    warnings: BTreeSet<String>,
}

impl FontBank {
    fn new(defs: egui::FontDefinitions) -> FontBank {
        FontBank { defs, embedded: HashMap::new(), glyphs: HashMap::new(), warnings: BTreeSet::new() }
    }

    /// The first font of `family` that has `c` and may be embedded, with the glyph.
    fn lookup(&mut self, family: &FontFamily, c: char) -> Option<(k::text::Font, k::text::GlyphId)> {
        let keys = self.defs.families.get(family)?.clone();
        for key in keys {
            let Some(data) = self.defs.font_data.get(&key).cloned() else { continue };
            let glyph = *self
                .glyphs
                .entry((key.clone(), c))
                .or_insert_with(|| FontRef::from_index(&data.font, data.index).ok().and_then(|f| f.charmap().map(c)).map(|g| g.to_u32()));
            let Some(glyph) = glyph.filter(|g| *g != 0) else { continue };
            if let Some(font) = self.embeddable(&key, &data) {
                return Some((font, k::text::GlyphId::new(glyph)));
            }
        }
        None
    }

    fn embeddable(&mut self, key: &str, data: &Arc<egui::FontData>) -> Option<k::text::Font> {
        if let Some(found) = self.embedded.get(key) {
            return found.clone();
        }
        // OS/2 `fsType` low nibble 2 is "restricted licence embedding".
        let restricted = FontRef::from_index(&data.font, data.index).ok().and_then(|f| f.os2().ok()).is_some_and(|os2| os2.fs_type() & 0x000F == 2);
        let font = if restricted {
            self.warnings.insert(format!("the font `{key}` does not allow embedding and was not used"));
            None
        } else {
            match write::font(&data.font, data.index, None) {
                Ok(f) => Some(f),
                Err(e) => {
                    self.warnings.insert(format!("the font `{key}` could not be embedded: {e}"));
                    None
                }
            }
        };
        self.embedded.insert(key.to_string(), font.clone());
        font
    }
}

struct Converter<'a> {
    bank: &'a mut FontBank,
    warnings: &'a mut BTreeSet<String>,
}

impl Converter<'_> {
    fn shape(&mut self, s: &mut k::surface::Surface, shape: &Shape) {
        match shape {
            Shape::Noop => {}
            Shape::Vec(inner) => inner.iter().for_each(|x| self.shape(s, x)),
            Shape::LineSegment { points, stroke } => draw(s, polyline(points, false), None, stroke_of(stroke.width, stroke.color)),
            Shape::Circle(c) => draw(s, ellipse(c.center, c.radius, c.radius), fill_of(c.fill), stroke_of(c.stroke.width, c.stroke.color)),
            Shape::Ellipse(e) => draw(s, ellipse(e.center, e.radius.x, e.radius.y), fill_of(e.fill), stroke_of(e.stroke.width, e.stroke.color)),
            Shape::Path(p) => self.path(s, p),
            Shape::Rect(r) => self.rect(s, r),
            Shape::Text(t) => self.text(s, t),
            Shape::QuadraticBezier(q) => {
                let mut b = PathBuilder::new();
                let [p0, p1, p2] = q.points;
                b.move_to(p0.x, p0.y);
                b.quad_to(p1.x, p1.y, p2.x, p2.y);
                if q.closed {
                    b.close();
                }
                draw(s, b.finish(), fill_of(q.fill), self.path_stroke(&q.stroke));
            }
            Shape::CubicBezier(c) => {
                let mut b = PathBuilder::new();
                let [p0, p1, p2, p3] = c.points;
                b.move_to(p0.x, p0.y);
                b.cubic_to(p1.x, p1.y, p2.x, p2.y, p3.x, p3.y);
                if c.closed {
                    b.close();
                }
                draw(s, b.finish(), fill_of(c.fill), self.path_stroke(&c.stroke));
            }
            Shape::Mesh(_) => {
                self.warnings.insert("an image or gradient on the page was left out".into());
            }
            Shape::Callback(_) => {
                self.warnings.insert("custom-painted content on the page was left out".into());
            }
        }
    }

    fn path_stroke(&mut self, st: &PathStroke) -> Option<KStroke> {
        match &st.color {
            ColorMode::Solid(c) => stroke_of(st.width, *c),
            ColorMode::UV(_) => {
                self.warnings.insert("a gradient stroke on the page was drawn without a stroke".into());
                None
            }
        }
    }

    fn path(&mut self, s: &mut k::surface::Surface, p: &PathShape) {
        let stroke = self.path_stroke(&p.stroke);
        draw(s, polyline(&p.points, p.closed), fill_of(p.fill).filter(|_| p.closed), stroke);
    }

    fn rect(&mut self, s: &mut k::surface::Surface, r: &RectShape) {
        let w = r.stroke.width.max(0.0);
        // A stroke sits on the edge (middle), inside it or outside it.
        let rect = match r.stroke_kind {
            StrokeKind::Inside => r.rect.shrink(w / 2.0),
            StrokeKind::Middle => r.rect,
            StrokeKind::Outside => r.rect.expand(w / 2.0),
        };
        let radii = [r.corner_radius.nw, r.corner_radius.ne, r.corner_radius.se, r.corner_radius.sw].map(f32::from);
        let fill = rounded_rect(r.rect, radii);
        let outline = rounded_rect(rect, radii);
        draw(s, fill, fill_of(r.fill), None);
        draw(s, outline, None, stroke_of(r.stroke.width, r.stroke.color));
    }

    fn text(&mut self, s: &mut k::surface::Surface, t: &TextShape) {
        let galley = &t.galley;
        let Some(fmt) = galley.job.sections.first().map(|sec| &sec.format) else { return };
        let ink = t.override_text_color.unwrap_or(if fmt.color == Color32::PLACEHOLDER { t.fallback_color } else { fmt.color });
        let Some(fill) = fill_of(ink.gamma_multiply(t.opacity_factor)) else { return };
        let (family, size) = (&fmt.font_id.family, fmt.font_id.size);
        s.set_fill(Some(fill));
        s.set_stroke(None);
        for row in &galley.rows {
            for g in &row.row.glyphs {
                if g.chr.is_whitespace() || g.chr.is_control() {
                    continue;
                }
                let at = Point::from_xy(t.pos.x + row.pos.x + g.pos.x, t.pos.y + row.pos.y + g.pos.y);
                match self.bank.lookup(family, g.chr) {
                    Some((font, id)) => {
                        let text = g.chr.to_string();
                        let glyph = k::text::KrillaGlyph::new(id, 0.0, 0.0, 0.0, 0.0, 0..text.len(), None);
                        s.draw_glyphs(at, &[glyph], font, &text, size, false);
                    }
                    None => {
                        self.warnings.insert(format!("the character {:?} is in none of the page fonts and was left out", g.chr));
                    }
                }
            }
        }
    }
}

fn draw(s: &mut k::surface::Surface, path: Option<Path>, fill: Option<Fill>, stroke: Option<KStroke>) {
    let Some(path) = path else { return };
    if fill.is_none() && stroke.is_none() {
        return;
    }
    s.set_fill(fill);
    s.set_stroke(stroke);
    s.draw_path(&path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;
    use serde_json::json;
    use std::path::Path;

    fn render(score: &Score, tracks: Vec<usize>, notation: bool) -> Result<PdfDocument, String> {
        PdfRenderer.render_pdf(score, &PdfRequest { tracks, notation, together: false })
    }

    /// A short two-track score typed in through commands.
    fn two_tracks() -> Session {
        let mut s = Session::new();
        let run = |s: &mut Session, id: &str, p: serde_json::Value| {
            s.run(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"));
        };
        run(&mut s, "score.setMeta", json!({"title": "Test Piece", "artist": "Nobody"}));
        for fret in [0, 3, 5, 7, 5, 3] {
            run(&mut s, "note.setFret", json!({"fret": fret}));
            run(&mut s, "cursor.right", serde_json::Value::Null);
        }
        run(&mut s, "track.add", json!({"name": "Bass", "tuning": "bass"}));
        run(&mut s, "note.setFret", json!({"fret": 2}));
        s
    }

    fn pages_of(bytes: &[u8]) -> Vec<(f32, f32)> {
        let pdf = hayro_syntax::Pdf::new(Arc::new(bytes.to_vec())).unwrap();
        pdf.pages().iter().map(|p| p.render_dimensions()).collect()
    }

    #[test]
    fn writes_a_readable_a4_pdf_with_embedded_fonts_and_the_title() {
        let s = two_tracks();
        let doc = render(s.score(), vec![0], true).unwrap();
        assert!(doc.bytes.starts_with(b"%PDF-"));
        let sizes = pages_of(&doc.bytes);
        assert_eq!(sizes.len(), doc.pages);
        for (w, h) in sizes {
            // A4 is 595.28 x 841.89 pt.
            assert!((w - 595.3).abs() < 0.5 && (h - 841.9).abs() < 0.5, "{w}x{h}");
        }
        let text = String::from_utf8_lossy(&doc.bytes);
        assert!(text.contains("FontFile2") || text.contains("FontFile3"), "no embedded font program");
        assert!(text.contains("Test Piece") && text.contains("Guitario"), "metadata missing");
        assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
    }

    #[test]
    fn the_pages_are_the_size_of_the_pages_setup() {
        let mut s = two_tracks();
        let size = |s: &Session| {
            let doc = render(s.score(), vec![0], true).unwrap();
            pages_of(&doc.bytes).first().copied().unwrap()
        };
        let near = |(w, h): (f32, f32), (ew, eh): (f32, f32)| (w - ew).abs() < 0.5 && (h - eh).abs() < 0.5;
        // Points: Letter is 8.5 x 11 in, Legal 8.5 x 14 in, A5 is 148 x 210 mm.
        s.run("score.setPage", &json!({"paper": "letter"})).unwrap();
        assert!(near(size(&s), (612.0, 792.0)), "{:?}", size(&s));
        s.run("score.setPage", &json!({"landscape": true})).unwrap();
        assert!(near(size(&s), (792.0, 612.0)), "{:?}", size(&s));
        s.run("score.setPage", &json!({"paper": "legal", "landscape": false})).unwrap();
        assert!(near(size(&s), (612.0, 1008.0)), "{:?}", size(&s));
        s.run("score.setPage", &json!({"paper": "a5"})).unwrap();
        assert!(near(size(&s), (419.5, 595.3)), "{:?}", size(&s));
        s.run("score.setPage", &json!({"reset": true})).unwrap();
        assert!(near(size(&s), (595.3, 841.9)), "{:?}", size(&s));
    }

    #[test]
    fn every_track_starts_a_new_page_and_the_order_is_the_request_order() {
        let s = two_tracks();
        let one = render(s.score(), vec![0], true).unwrap();
        let both = render(s.score(), vec![0, 1], true).unwrap();
        assert!(both.pages > one.pages, "{} vs {}", both.pages, one.pages);
        assert_eq!(pages_of(&both.bytes).len(), both.pages);
        let swapped = render(s.score(), vec![1, 0], true).unwrap();
        assert_eq!(swapped.pages, both.pages);
        assert_ne!(swapped.bytes, both.bytes);
    }

    #[test]
    fn a_full_score_stacks_the_tracks_in_every_system() {
        let s = two_tracks();
        let together = |tracks: Vec<usize>| PdfRenderer.render_pdf(s.score(), &PdfRequest { tracks, notation: true, together: true }).unwrap();
        let parts = render(s.score(), vec![0, 1], true).unwrap();
        let score = together(vec![0, 1]);
        // One short page holds both staves, where the parts take a page each.
        assert_eq!((score.pages, parts.pages), (1, 2));
        assert_eq!(pages_of(&score.bytes).len(), 1);
        assert!(score.warnings.is_empty(), "{:?}", score.warnings);
        // The order is the request's, and one track alone is the same as its part.
        assert_ne!(together(vec![1, 0]).bytes, score.bytes);
        assert_eq!(together(vec![0]).bytes, render(s.score(), vec![0], true).unwrap().bytes);
        assert!(PdfRenderer.render_pdf(s.score(), &PdfRequest { tracks: vec![0, 9], notation: true, together: true }).is_err());
    }

    #[test]
    fn tab_only_leaves_out_the_staff() {
        let s = two_tracks();
        let with = render(s.score(), vec![0], true).unwrap();
        let without = render(s.score(), vec![0], false).unwrap();
        assert_ne!(with.bytes, without.bytes);
    }

    #[test]
    fn the_same_score_gives_the_same_bytes() {
        let s = two_tracks();
        assert_eq!(render(s.score(), vec![0, 1], true).unwrap().bytes, render(s.score(), vec![0, 1], true).unwrap().bytes);
    }

    #[test]
    fn hostile_requests_are_errors_not_panics() {
        let s = two_tracks();
        assert!(render(s.score(), vec![], true).is_err());
        assert!(render(s.score(), vec![7], true).is_err());
        assert!(render(s.score(), vec![usize::MAX], true).is_err());
        // An empty score still makes a one-page document.
        let blank = Session::new();
        assert!(render(blank.score(), vec![0], true).unwrap().pages >= 1);
    }

    #[test]
    fn a_title_with_odd_characters_is_kept_or_reported_never_a_crash() {
        let mut s = two_tracks();
        s.run("score.setMeta", &json!({"title": "Ünïcödé \u{1F3B8} 日本語 \u{0}\u{7}"})).unwrap();
        let doc = render(s.score(), vec![0], true).unwrap();
        assert!(doc.bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn the_app_registers_the_renderer_so_the_command_works_from_the_menu() {
        let app = crate::GuitarioApp::new(two_tracks(), None);
        let path = std::env::temp_dir().join(format!("gt-pdf-{}.pdf", std::process::id()));
        let mut session = app.session;
        let r = session.run("file.exportPdf", &json!({"path": path.display().to_string(), "tracks": [0]})).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(r["bytes"], bytes.len());
        assert!(bytes.starts_with(b"%PDF-"));
    }

    /// Every demo score prints without leaving a character out or failing.
    #[test]
    fn every_demo_exports_without_warnings() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "gt") {
                continue;
            }
            let mut s = Session::new();
            s.run("file.open", &json!({"path": path.display().to_string()})).unwrap();
            let all: Vec<usize> = (0..s.score().tracks.len()).collect();
            let doc = render(s.score(), all, true).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(doc.pages >= 1 && doc.warnings.is_empty(), "{}: {:?}", path.display(), doc.warnings);
            seen += 1;
        }
        assert!(seen >= 10, "found only {seen} demos in {}", dir.display());
    }
}
