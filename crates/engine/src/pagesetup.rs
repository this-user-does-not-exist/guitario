//! File › Page Setup: the paper, its orientation and the margins every page is laid out on, in the
//! page view, the PDF and the printer. Stored in the score, so it is saved with the file and undone
//! like any other edit.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{PageSetup, Paper};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "score.setPage",
            "Page Setup…",
            "File",
            None,
            "paper? (\"a4\", \"a5\", \"letter\", \"legal\"), landscape? (bool), marginLeft? marginRight? marginTop? marginBottom? (mm, 0..=60), \
             bendPitch? (bool: standard notation shows the pitch each bend reaches), \
             reset? (true restores A4 upright with Guitar Pro's margins before the other fields apply; bendPitch is kept)",
            true,
            set_page,
        ),
        cmd("score.page", "Page Setup", "", None, "", false, page),
    ]
}

fn json_of(page: &PageSetup) -> Value {
    let (w, h) = page.size_mm();
    // Tenths of a millimetre, as a double that prints as 215.9 rather than 215.89999.
    let tenth = |mm: f32| f64::from((mm * 10.0).round()) / 10.0;
    json!({
        "paper": page.paper.name().to_lowercase(), "landscape": page.landscape,
        "marginLeft": page.margin_left, "marginRight": page.margin_right, "marginTop": page.margin_top, "marginBottom": page.margin_bottom,
        "widthMm": tenth(w), "heightMm": tenth(h), "bendPitch": page.bend_pitch,
    })
}

fn page(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    Ok(json_of(&s.score.page))
}

fn set_page(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let mut next = if params.get("reset").is_some() && p::boolean(params, "reset")? {
        PageSetup { bend_pitch: s.score.page.bend_pitch, ..PageSetup::default() }
    } else {
        s.score.page
    };
    if params.get("bendPitch").is_some_and(|v| !v.is_null()) {
        next.bend_pitch = p::boolean(params, "bendPitch")?;
    }
    if let Some(v) = params.get("paper").filter(|v| !v.is_null()) {
        let name = v.as_str().ok_or_else(|| EngineError::BadParam("`paper` must be \"a4\", \"a5\", \"letter\" or \"legal\"".into()))?;
        next.paper = Paper::from_name(name).ok_or_else(|| EngineError::BadParam(format!("`paper` {name:?} is not one of a4, a5, letter, legal")))?;
    }
    if params.get("landscape").is_some_and(|v| !v.is_null()) {
        next.landscape = p::boolean(params, "landscape")?;
    }
    for (key, field) in [
        ("marginLeft", &mut next.margin_left),
        ("marginRight", &mut next.margin_right),
        ("marginTop", &mut next.margin_top),
        ("marginBottom", &mut next.margin_bottom),
    ] {
        if let Some(mm) = p::opt_uint(params, key, u64::from(PageSetup::MAX_MARGIN))? {
            *field = u8::try_from(mm).unwrap_or(PageSetup::MAX_MARGIN);
        }
    }
    next.check().map_err(EngineError::invalid)?;
    Arc::make_mut(&mut s.score).page = next;
    Ok(json_of(&next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_changes_keeps_what_is_not_named_and_undoes() {
        let mut s = Session::new();
        assert_eq!(s.run("score.page", &Value::Null).unwrap()["paper"], "a4");
        let r = s.run("score.setPage", &json!({"paper": "Letter", "marginTop": 20})).unwrap();
        assert_eq!((r["paper"].clone(), r["marginTop"].clone(), r["marginLeft"].clone()), (json!("letter"), json!(20), json!(10)));
        assert_eq!(r["widthMm"], 215.9);
        let r = s.run("score.setPage", &json!({"bendPitch": true})).unwrap();
        assert_eq!(r["bendPitch"], true);
        assert!(s.run("score.setPage", &json!({"bendPitch": "yes"})).is_err());
        let r = s.run("score.setPage", &json!({"reset": true})).unwrap();
        assert_eq!(r["bendPitch"], true, "a reset of the page keeps the notation option");
        s.run("score.setPage", &json!({"paper": "Letter", "marginTop": 20})).unwrap();
        let r = s.run("score.setPage", &json!({"landscape": true})).unwrap();
        assert_eq!((r["paper"].clone(), r["widthMm"].clone(), r["heightMm"].clone()), (json!("letter"), json!(279.4), json!(215.9)));
        assert!(s.is_dirty());
        s.run("edit.undo", &Value::Null).unwrap();
        assert!(!s.score().page.landscape);
        assert_eq!(s.score().page.paper, Paper::Letter);
        s.run("score.setPage", &json!({"reset": true})).unwrap();
        assert_eq!(s.score().page, PageSetup { bend_pitch: true, ..PageSetup::default() });
    }

    #[test]
    fn reset_applies_before_the_other_fields() {
        let mut s = Session::new();
        s.run("score.setPage", &json!({"paper": "legal", "marginLeft": 30})).unwrap();
        s.run("score.setPage", &json!({"reset": true, "paper": "a5"})).unwrap();
        assert_eq!(s.score().page, PageSetup { paper: Paper::A5, ..PageSetup::default() });
    }

    #[test]
    fn hostile_pages_are_refused_and_change_nothing() {
        let mut s = Session::new();
        for bad in [
            json!({"paper": "tabloid"}),
            json!({"paper": 4}),
            json!({"landscape": "yes"}),
            json!({"reset": "yes"}),
            json!({"marginLeft": -1}),
            json!({"marginLeft": 61}),
            json!({"marginTop": 1.5}),
            json!({"marginRight": "10"}),
            json!({"marginLeft": 60, "marginRight": 60}),
            json!({"paper": "a5", "landscape": true, "marginTop": 60, "marginBottom": 60}),
            json!({"marginBottom": 18446744073709551615u64}),
        ] {
            assert!(s.run("score.setPage", &bad).is_err(), "{bad}");
        }
        assert_eq!(s.score().page, PageSetup::default());
        assert!(!s.is_dirty());
    }
}
