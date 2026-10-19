//! Headers and footers, as Guitar Pro's stylesheet has them: each text is a template with tokens
//! (`%TITLE%`, `%PAGE%` …), switched on or off, aligned and set in its own font, separately for
//! the first page and for the pages after it. The defaults are Guitar Pro's own, from the
//! stylesheets of real files.

use crate::Meta;
use serde::{Deserialize, Serialize};

/// Longest template accepted, in characters.
pub const MAX_PAGE_TEXT: usize = 500;
/// Font sizes a page text may have, in points.
pub const PAGE_TEXT_SIZES: std::ops::RangeInclusive<f32> = 4.0..=96.0;
/// The font a page text uses unless it names another.
pub const DEFAULT_PAGE_FONT: &str = "Times New Roman";

/// Where a line of text sits across the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

impl TextAlign {
    pub const ALL: [TextAlign; 3] = [TextAlign::Left, TextAlign::Center, TextAlign::Right];

    pub fn name(self) -> &'static str {
        match self {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
        }
    }

    pub fn from_name(name: &str) -> Option<TextAlign> {
        TextAlign::ALL.into_iter().find(|a| a.name().eq_ignore_ascii_case(name.trim()))
    }

    /// Left and right swapped, as a left-hand (even) page has them.
    pub fn mirrored(self) -> TextAlign {
        match self {
            TextAlign::Left => TextAlign::Right,
            TextAlign::Center => TextAlign::Center,
            TextAlign::Right => TextAlign::Left,
        }
    }
}

/// One header or footer text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageText {
    pub on: bool,
    /// The template: plain text and tokens.
    pub text: String,
    pub align: TextAlign,
    /// The font family by name; the page draws it in the closest face it has.
    pub font: String,
    /// In points.
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

impl Default for PageText {
    fn default() -> Self {
        PageText { on: true, text: String::new(), align: TextAlign::Center, font: DEFAULT_PAGE_FONT.to_string(), size: 10.0, bold: false, italic: false }
    }
}

impl PageText {
    fn new(on: bool, text: &str, align: TextAlign, size: f32) -> PageText {
        PageText { on, text: text.to_string(), align, size, ..PageText::default() }
    }

    fn bold(self) -> PageText {
        PageText { bold: true, ..self }
    }

    /// The text as it prints on page `page` of `pages`, or `None` when it is off, empty, or every
    /// token in it stands for an empty field ("Music by %MUSIC%" with no composer prints nothing).
    pub fn expand(&self, meta: &Meta, page: usize, pages: usize) -> Option<String> {
        if !self.on {
            return None;
        }
        let out = expand(&self.text, meta, page, pages)?;
        let out = out.trim();
        (!out.is_empty()).then(|| out.to_string())
    }

    fn check(&self, what: &str) -> Result<(), String> {
        if self.text.chars().count() > MAX_PAGE_TEXT {
            return Err(format!("the {what} text is longer than {MAX_PAGE_TEXT} characters"));
        }
        if self.font.chars().count() > 100 || self.font.chars().any(char::is_control) {
            return Err(format!("the {what} font name is not a font name"));
        }
        if !self.size.is_finite() || !PAGE_TEXT_SIZES.contains(&self.size) {
            return Err(format!("the {what} size is {} pt; it must be {} to {} pt", self.size, PAGE_TEXT_SIZES.start(), PAGE_TEXT_SIZES.end()));
        }
        Ok(())
    }
}

/// The tokens a template may hold, each with the field it stands for.
pub const TOKENS: [(&str, &str); 11] = [
    ("%TITLE%", "Score title"),
    ("%SUBTITLE%", "Score subtitle"),
    ("%ARTIST%", "Score artist"),
    ("%ALBUM%", "Score album"),
    ("%WORDS%", "Score lyrics"),
    ("%MUSIC%", "Score music"),
    ("%WORDS&MUSIC%", "Score words and music"),
    ("%TABBER%", "Score transcriber"),
    ("%COPYRIGHT%", "Score copyright"),
    ("%PAGE%", "Current page number"),
    ("%PAGES%", "Total number of pages"),
];

/// `text` with every token replaced (case matters, as in Guitar Pro; anything else between `%`
/// signs is left alone). `None` when the text has tokens and all of them are empty.
pub fn expand(text: &str, meta: &Meta, page: usize, pages: usize) -> Option<String> {
    let field = |token: &str| -> Option<String> {
        let s = match token {
            "%TITLE%" => meta.title.trim(),
            "%SUBTITLE%" => meta.subtitle.trim(),
            "%ARTIST%" => meta.artist.trim(),
            "%ALBUM%" => meta.album.trim(),
            "%WORDS%" => meta.words.trim(),
            "%MUSIC%" => meta.music.trim(),
            "%WORDS&MUSIC%" => {
                let (w, m) = (meta.words.trim(), meta.music.trim());
                if w.is_empty() || w == m { m } else { w }
            }
            "%TABBER%" => meta.author.trim(),
            "%COPYRIGHT%" => meta.copyright.trim(),
            "%PAGE%" => return Some(page.to_string()),
            "%PAGES%" => return Some(pages.to_string()),
            _ => return None,
        };
        Some(s.to_string())
    };
    let mut out = String::with_capacity(text.len());
    let (mut tokens, mut filled) = (0, 0);
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let (before, from) = rest.split_at(start);
        out.push_str(before);
        let token = from.get(1..).and_then(|after| after.find('%')).and_then(|end| from.get(..end + 2));
        match token.and_then(|t| field(t).map(|v| (t, v))) {
            Some((t, value)) => {
                tokens += 1;
                if !value.is_empty() {
                    filled += 1;
                }
                out.push_str(&value);
                rest = from.get(t.len()..).unwrap_or("");
            }
            None => {
                out.push('%');
                rest = from.get(1..).unwrap_or("");
            }
        }
    }
    out.push_str(rest);
    (tokens == 0 || filled > 0).then_some(out)
}

/// What the first page prints above the music and at its foot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FirstPage {
    pub title: PageText,
    pub subtitle: PageText,
    pub artist: PageText,
    pub album: PageText,
    pub words: PageText,
    pub music: PageText,
    /// Printed in place of the words and the music when one person wrote both.
    pub words_and_music: PageText,
    pub tabber: PageText,
    pub copyright: PageText,
    /// A second line under the copyright, printed only when the copyright is.
    pub copyright2: PageText,
    pub page_number: PageText,
}

impl Default for FirstPage {
    fn default() -> Self {
        use TextAlign::*;
        FirstPage {
            title: PageText::new(true, "%TITLE%", Center, 23.0),
            subtitle: PageText::new(true, "%SUBTITLE%", Center, 12.0),
            artist: PageText::new(true, "%ARTIST%", Center, 12.0).bold(),
            album: PageText::new(true, "%ALBUM%", Center, 12.0),
            words: PageText::new(true, "Words by %WORDS%", Left, 9.0),
            music: PageText::new(true, "Music by %MUSIC%", Right, 9.0),
            words_and_music: PageText::new(true, "Words & Music by %MUSIC%", Right, 9.0),
            tabber: PageText::new(false, "Tabbed by %TABBER%", Right, 9.0),
            copyright: PageText::new(true, "%COPYRIGHT%", Center, 7.0),
            copyright2: PageText::new(true, "All Rights Reserved - International Copyright Secured", Center, 7.0),
            page_number: PageText::new(true, "%PAGE%/%PAGES%", Right, 7.0),
        }
    }
}

/// What every page after the first prints. Left and right swap on even pages, so a page number
/// set at the right stays at the outer edge of a two-sided print.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OtherPages {
    pub header: PageText,
    pub copyright: PageText,
    pub copyright2: PageText,
    pub page_number: PageText,
}

impl Default for OtherPages {
    fn default() -> Self {
        use TextAlign::*;
        OtherPages {
            header: PageText::new(false, "%TITLE% by %ARTIST%", Right, 9.0),
            copyright: PageText::new(false, "%COPYRIGHT%", Center, 7.0),
            copyright2: PageText::new(true, "All Rights Reserved - International Copyright Secured", Center, 7.0),
            page_number: PageText::new(true, "%PAGE%/%PAGES%", Right, 7.0),
        }
    }
}

/// The stylesheet's Header & Footer tab.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeaderFooter {
    pub first: FirstPage,
    pub other: OtherPages,
}

/// The texts by the names commands use: `first.title`, `other.pageNumber` …
pub const PAGE_TEXT_KEYS: [&str; 15] = [
    "first.title",
    "first.subtitle",
    "first.artist",
    "first.album",
    "first.words",
    "first.music",
    "first.wordsAndMusic",
    "first.tabber",
    "first.copyright",
    "first.copyright2",
    "first.pageNumber",
    "other.header",
    "other.copyright",
    "other.copyright2",
    "other.pageNumber",
];

impl HeaderFooter {
    pub fn is_default(&self) -> bool {
        *self == HeaderFooter::default()
    }

    /// The text called `key` (one of [`PAGE_TEXT_KEYS`]).
    pub fn get(&self, key: &str) -> Option<&PageText> {
        let (f, o) = (&self.first, &self.other);
        Some(match key {
            "first.title" => &f.title,
            "first.subtitle" => &f.subtitle,
            "first.artist" => &f.artist,
            "first.album" => &f.album,
            "first.words" => &f.words,
            "first.music" => &f.music,
            "first.wordsAndMusic" => &f.words_and_music,
            "first.tabber" => &f.tabber,
            "first.copyright" => &f.copyright,
            "first.copyright2" => &f.copyright2,
            "first.pageNumber" => &f.page_number,
            "other.header" => &o.header,
            "other.copyright" => &o.copyright,
            "other.copyright2" => &o.copyright2,
            "other.pageNumber" => &o.page_number,
            _ => return None,
        })
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut PageText> {
        let (f, o) = (&mut self.first, &mut self.other);
        Some(match key {
            "first.title" => &mut f.title,
            "first.subtitle" => &mut f.subtitle,
            "first.artist" => &mut f.artist,
            "first.album" => &mut f.album,
            "first.words" => &mut f.words,
            "first.music" => &mut f.music,
            "first.wordsAndMusic" => &mut f.words_and_music,
            "first.tabber" => &mut f.tabber,
            "first.copyright" => &mut f.copyright,
            "first.copyright2" => &mut f.copyright2,
            "first.pageNumber" => &mut f.page_number,
            "other.header" => &mut o.header,
            "other.copyright" => &mut o.copyright,
            "other.copyright2" => &mut o.copyright2,
            "other.pageNumber" => &mut o.page_number,
            _ => return None,
        })
    }

    /// Why these texts cannot be used, if they cannot.
    pub fn check(&self) -> Result<(), String> {
        for key in PAGE_TEXT_KEYS {
            if let Some(t) = self.get(key) {
                t.check(key)?;
            }
        }
        Ok(())
    }

    /// The first page's header, as rows from the top: each row holds the texts that share a line
    /// (the words at the left and the music at the right), with the text and its style.
    pub fn first_header(&self, meta: &Meta, pages: usize) -> Vec<Vec<(String, &PageText)>> {
        let f = &self.first;
        let (w, m) = (meta.words.trim(), meta.music.trim());
        let one_writer = !w.is_empty() && w == m && f.words_and_music.on;
        let credits: Vec<&PageText> = if one_writer { vec![&f.words_and_music] } else { vec![&f.words, &f.music] };
        let groups: [Vec<&PageText>; 6] = [vec![&f.title], vec![&f.subtitle], vec![&f.artist], vec![&f.album], credits, vec![&f.tabber]];
        let mut rows = Vec::new();
        for group in groups {
            pack(group.into_iter().filter_map(|t| t.expand(meta, 1, pages).map(|s| (s, t))), &mut rows);
        }
        rows
    }

    /// The footer of page `page` (1-based) of `pages`, as rows from the top, with alignment
    /// already mirrored on even pages.
    pub fn footer(&self, meta: &Meta, page: usize, pages: usize) -> Vec<Vec<(String, PageText)>> {
        let (copyright, copyright2, number) = if page <= 1 {
            (&self.first.copyright, &self.first.copyright2, &self.first.page_number)
        } else {
            (&self.other.copyright, &self.other.copyright2, &self.other.page_number)
        };
        let mirror = |t: &PageText| if page > 1 && page.is_multiple_of(2) { PageText { align: t.align.mirrored(), ..t.clone() } } else { t.clone() };
        let line = |t: &PageText| t.expand(meta, page, pages).map(|s| (s, mirror(t)));
        // The second line belongs to the copyright: without a copyright there is nothing it reserves.
        let has_copyright = !meta.copyright.trim().is_empty();
        let mut rows: Vec<Vec<(String, PageText)>> = Vec::new();
        let first = line(copyright);
        let second = if has_copyright { line(copyright2) } else { None };
        let num = line(number);
        let mut top: Vec<(String, PageText)> = first.into_iter().collect();
        // The page number shares the copyright's line when they sit at different places.
        let mut num = num;
        if let Some((s, t)) = num.take() {
            if top.iter().all(|(_, o)| o.align != t.align) && !top.is_empty() {
                top.push((s, t));
            } else {
                num = Some((s, t));
            }
        }
        if !top.is_empty() {
            rows.push(top);
        }
        rows.extend(second.map(|r| vec![r]));
        rows.extend(num.map(|r| vec![r]));
        rows
    }

    /// The header line of page `page` (1-based, after the first) of `pages`, mirrored on even pages.
    pub fn other_header(&self, meta: &Meta, page: usize, pages: usize) -> Option<(String, PageText)> {
        let t = &self.other.header;
        let s = t.expand(meta, page, pages)?;
        Some((s, if page.is_multiple_of(2) { PageText { align: t.align.mirrored(), ..t.clone() } } else { t.clone() }))
    }
}

/// Adds `items` to `rows`: texts at different places share a row, one at a place already taken
/// starts the next.
fn pack<'a>(items: impl Iterator<Item = (String, &'a PageText)>, rows: &mut Vec<Vec<(String, &'a PageText)>>) {
    let mut row: Vec<(String, &PageText)> = Vec::new();
    for (s, t) in items {
        if row.iter().any(|(_, o)| o.align == t.align || o.align == TextAlign::Center || t.align == TextAlign::Center) {
            rows.push(std::mem::take(&mut row));
        }
        row.push((s, t));
    }
    if !row.is_empty() {
        rows.push(row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> Meta {
        Meta { title: "Iron Tide".into(), artist: "Ashgrave".into(), music: "R. Vale".into(), copyright: "2026 Ashgrave".into(), ..Meta::default() }
    }

    #[test]
    fn tokens_fill_from_the_song_information_and_the_page() {
        let m = meta();
        assert_eq!(expand("%TITLE% by %ARTIST%", &m, 1, 3).as_deref(), Some("Iron Tide by Ashgrave"));
        assert_eq!(expand("%PAGE%/%PAGES%", &m, 2, 3).as_deref(), Some("2/3"));
        assert_eq!(expand("Words by %WORDS%", &m, 1, 1), None, "every token empty");
        assert_eq!(expand("100% %UNKNOWN% %TITLE", &m, 1, 1).as_deref(), Some("100% %UNKNOWN% %TITLE"), "not tokens");
        assert_eq!(expand("%%TITLE%%", &m, 1, 1).as_deref(), Some("%Iron Tide%"));
        assert_eq!(expand("plain", &m, 1, 1).as_deref(), Some("plain"));
        assert_eq!(expand("%WORDS&MUSIC%", &m, 1, 1).as_deref(), Some("R. Vale"));
        assert_eq!(expand("ünï%TITLE%€", &m, 1, 1).as_deref(), Some("ünïIron Tide€"));
    }

    #[test]
    fn the_first_page_puts_words_and_music_on_one_line_and_skips_empty_fields() {
        let hf = HeaderFooter::default();
        let mut m = meta();
        m.words = "K. Moor".into();
        let rows = hf.first_header(&m, 1);
        let texts: Vec<Vec<&str>> = rows.iter().map(|r| r.iter().map(|(s, _)| s.as_str()).collect()).collect();
        assert_eq!(texts, vec![vec!["Iron Tide"], vec!["Ashgrave"], vec!["Words by K. Moor", "Music by R. Vale"]]);
        m.words = "R. Vale".into();
        let rows = hf.first_header(&m, 1);
        assert_eq!(rows.last().map(|r| r.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>()), Some(vec!["Words & Music by R. Vale".to_string()]));
        // The transcriber is off by default; switched on it prints at the right under the credits.
        let mut on = hf.clone();
        on.first.tabber.on = true;
        m.author = "T. Grim".into();
        assert_eq!(on.first_header(&m, 1).last().map(|r| r.len()), Some(1));
        assert_eq!(hf.first_header(&Meta::default(), 1).len(), 0, "nothing to print");
    }

    #[test]
    fn the_footer_has_the_copyright_lines_and_the_page_number_beside_them() {
        let hf = HeaderFooter::default();
        let rows = hf.footer(&meta(), 1, 2);
        let texts: Vec<Vec<&str>> = rows.iter().map(|r| r.iter().map(|(s, _)| s.as_str()).collect()).collect();
        assert_eq!(texts, vec![vec!["2026 Ashgrave", "1/2"], vec!["All Rights Reserved - International Copyright Secured"]]);
        // Other pages: no copyright line by default, the second line and the page number,
        // mirrored to the left on even pages.
        let rows = hf.footer(&meta(), 2, 2);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.last().and_then(|r| r.first()).map(|(s, t)| (s.as_str(), t.align)), Some(("2/2", TextAlign::Left)));
        let rows = hf.footer(&meta(), 3, 3);
        assert_eq!(rows.last().and_then(|r| r.first()).map(|(_, t)| t.align), Some(TextAlign::Right));
        // No copyright: only the page number.
        assert_eq!(hf.footer(&Meta::default(), 1, 1).len(), 1);
    }

    #[test]
    fn keys_reach_every_text_and_checks_refuse_bad_ones() {
        let mut hf = HeaderFooter::default();
        for key in PAGE_TEXT_KEYS {
            assert!(hf.get(key).is_some() && hf.get_mut(key).is_some(), "{key}");
        }
        assert!(hf.get("first.nothing").is_none());
        assert!(hf.check().is_ok());
        for bad in [f32::NAN, f32::INFINITY, 0.0, 200.0] {
            let mut h = HeaderFooter::default();
            h.first.title.size = bad;
            assert!(h.check().is_err(), "{bad}");
        }
        hf.other.header.text = "x".repeat(MAX_PAGE_TEXT + 1);
        assert!(hf.check().is_err());
    }

    #[test]
    fn old_and_partial_files_load_with_the_defaults() {
        let hf: HeaderFooter = serde_json::from_str(r#"{"first":{"title":{"size":30}}}"#).unwrap_or_default();
        assert_eq!(hf.first.title.size, 30.0);
        assert_eq!(hf.first.subtitle, FirstPage::default().subtitle, "texts left out keep Guitar Pro's");
        assert_eq!(hf.other, OtherPages::default());
    }
}
