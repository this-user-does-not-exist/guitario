//! Page setup in Guitar Pro's terms: whole millimetres for the paper and the margins, and
//! Guitar Pro's own default margins, which map to Guitario's defaults (and back) so a file nobody
//! changed stays unchanged. Real files were checked: every Guitar Pro 5 file and all but one
//! Guitar Pro 6 file in a local corpus are A4 with these margins; the other is Letter, stored as
//! 216 x 279 mm.

use guitario_score::{PageSetup, Paper};

/// Guitar Pro's default margins: left, right, top, bottom.
const GP_DEFAULT_MARGINS: [i32; 4] = [10, 10, 15, 10];
/// The same, as Guitario holds them (the bottom margin is 12 mm in its stylesheet).
const OUR_DEFAULT_MARGINS: [i32; 4] = [10, 10, 15, 12];
/// How far a stored paper size may be from the real one, in millimetres (216 x 279 for Letter).
const SIZE_TOLERANCE: f32 = 2.0;

fn margins_of(page: &PageSetup) -> [i32; 4] {
    [page.margin_left, page.margin_right, page.margin_top, page.margin_bottom].map(i32::from)
}

/// The page a file describes. `landscape` is the file's own orientation flag when it has one; without
/// it a page wider than tall is landscape. What cannot be used is replaced by the default, with a warning.
pub(crate) fn from_gp(width: i32, height: i32, landscape: Option<bool>, margins: [i32; 4], warnings: &mut Vec<String>) -> PageSetup {
    let mut page = PageSetup::default();
    let upright = (width.min(height), width.max(height));
    let custom = (u16::try_from(upright.0).ok(), u16::try_from(upright.1).ok());
    match (Paper::matching(width as f32, height as f32, SIZE_TOLERANCE), custom) {
        (Some(paper), _) => {
            page.paper = paper;
            page.landscape = landscape.unwrap_or(width > height);
        }
        (None, (Some(w), Some(h))) if PageSetup::CUSTOM_SIDES.contains(&w) && PageSetup::CUSTOM_SIDES.contains(&h) => {
            page.paper = Paper::Custom;
            (page.custom_width, page.custom_height) = (w, h);
            page.landscape = landscape.unwrap_or(width > height);
        }
        _ => warnings.push(format!("the page size {width} x {height} mm is no paper Guitario can lay out; A4 is used")),
    }
    if margins != GP_DEFAULT_MARGINS {
        let clamp = |mm: i32| u8::try_from(mm.clamp(0, i32::from(PageSetup::MAX_MARGIN))).unwrap_or(PageSetup::MAX_MARGIN);
        let wanted = PageSetup {
            margin_left: clamp(margins[0]),
            margin_right: clamp(margins[1]),
            margin_top: clamp(margins[2]),
            margin_bottom: clamp(margins[3]),
            ..page
        };
        match wanted.check() {
            Ok(()) => page = wanted,
            Err(why) => warnings.push(format!("the page margins {margins:?} were not used: {why}")),
        }
    }
    page
}

/// A page as Guitar Pro stores it: width and height as the page lies, in whole millimetres, and the
/// margins left, right, top, bottom.
pub(crate) fn to_gp(page: &PageSetup) -> (i32, i32, [i32; 4]) {
    let (w, h) = page.size_mm();
    let margins = margins_of(page);
    (w.round() as i32, h.round() as i32, if margins == OUR_DEFAULT_MARGINS { GP_DEFAULT_MARGINS } else { margins })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(w: i32, h: i32, landscape: Option<bool>, margins: [i32; 4]) -> (PageSetup, Vec<String>) {
        let mut warnings = Vec::new();
        (from_gp(w, h, landscape, margins, &mut warnings), warnings)
    }

    #[test]
    fn guitar_pros_defaults_are_ours() {
        assert_eq!(read(210, 297, None, GP_DEFAULT_MARGINS), (PageSetup::default(), vec![]));
        assert_eq!(to_gp(&PageSetup::default()), (210, 297, GP_DEFAULT_MARGINS));
    }

    #[test]
    fn real_letter_files_are_letter() {
        // As stored by a real Guitar Pro 6 file: whole millimetres.
        let (page, warnings) = read(216, 279, Some(false), GP_DEFAULT_MARGINS);
        assert_eq!((page.paper, page.landscape, warnings.len()), (Paper::Letter, false, 0));
        assert_eq!(read(216, 356, None, GP_DEFAULT_MARGINS).0.paper, Paper::Legal);
        assert_eq!(read(148, 210, None, GP_DEFAULT_MARGINS).0.paper, Paper::A5);
    }

    #[test]
    fn orientation_comes_from_the_flag_or_the_shape() {
        assert!(read(297, 210, None, GP_DEFAULT_MARGINS).0.landscape);
        assert!(!read(210, 297, None, GP_DEFAULT_MARGINS).0.landscape);
        // A file that says landscape but keeps the paper upright is still landscape, and the other way round.
        assert!(read(210, 297, Some(true), GP_DEFAULT_MARGINS).0.landscape);
        assert!(!read(297, 210, Some(false), GP_DEFAULT_MARGINS).0.landscape);
    }

    #[test]
    fn an_unknown_size_is_a_custom_paper() {
        let (page, warnings) = read(280, 200, None, GP_DEFAULT_MARGINS);
        assert_eq!((page.paper, page.custom_width, page.custom_height, page.landscape, warnings.len()), (Paper::Custom, 200, 280, true, 0));
        assert_eq!(to_gp(&page), (280, 200, GP_DEFAULT_MARGINS));
        assert_eq!(read(297, 420, None, GP_DEFAULT_MARGINS).0.paper, Paper::A3);
        assert_eq!(read(279, 432, None, GP_DEFAULT_MARGINS).0.paper, Paper::Tabloid);
    }

    #[test]
    fn what_cannot_be_used_becomes_the_default_with_a_warning() {
        let (page, warnings) = read(50, 2000, None, GP_DEFAULT_MARGINS);
        assert_eq!(page, PageSetup::default());
        assert!(warnings.iter().any(|w| w.contains("50 x 2000")), "{warnings:?}");
        for (w, h) in [(0, 0), (-5, 297), (i32::MAX, i32::MIN), (1, 1)] {
            let (page, warnings) = read(w, h, None, GP_DEFAULT_MARGINS);
            assert_eq!(page, PageSetup::default());
            assert_eq!(warnings.len(), 1);
        }
        // Margins that leave no room, or nonsense ones, are not used; the paper still is.
        let (page, warnings) = read(148, 210, None, [60, 60, 15, 10]);
        assert_eq!((page.paper, page.margin_left), (Paper::A5, 10));
        assert_eq!(warnings.len(), 1);
        let (page, _) = read(210, 297, None, [i32::MIN, i32::MAX, -3, 1000]);
        assert_eq!((page.margin_left, page.margin_right, page.margin_top, page.margin_bottom), (0, 60, 0, 60));
    }

    #[test]
    fn every_page_goes_out_and_comes_back() {
        for paper in Paper::ALL {
            for landscape in [false, true] {
                for margins in [(10, 10, 15, 12), (5, 25, 20, 8), (0, 0, 0, 0), (30, 30, 30, 30)] {
                    let page = PageSetup {
                        paper,
                        landscape,
                        margin_left: margins.0,
                        margin_right: margins.1,
                        margin_top: margins.2,
                        margin_bottom: margins.3,
                        ..PageSetup::default()
                    };
                    if page.check().is_err() {
                        continue;
                    }
                    let (w, h, m) = to_gp(&page);
                    let (back, warnings) = read(w, h, Some(landscape), m);
                    assert_eq!((back, warnings), (page, vec![]), "{page:?}");
                }
            }
        }
    }
}
