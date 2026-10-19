//! File › Stylesheet › Header & Footer: the texts printed above the music and at the foot of each
//! page, built from tokens. Stored in the score, so they are saved with the file and undone like
//! any other edit.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{HeaderFooter, PAGE_TEXT_KEYS, PAGE_TEXT_SIZES, PageText, TOKENS, TextAlign};
use serde_json::{Map, Value, json};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "score.setHeaderFooter",
            "Header & Footer",
            "File",
            None,
            "item? (one of first.title, first.subtitle, first.artist, first.album, first.words, first.music, first.wordsAndMusic, \
             first.tabber, first.copyright, first.copyright2, first.pageNumber, other.header, other.copyright, other.copyright2, \
             other.pageNumber) with on? (bool), text? (template with tokens such as %TITLE% or %PAGE%), align? (\"left\", \"center\", \"right\"), \
             font? (family name), size? (points, 4..=96), bold? italic? (bool); or items? ({item: {on?, text?, …}} to change several at once); \
             reset? (true restores Guitar Pro's defaults first: the named item's, or all of them)",
            true,
            set_header_footer,
        ),
        cmd("score.headerFooter", "Header & Footer", "", None, "", false, header_footer),
    ]
}

fn text_json(t: &PageText) -> Value {
    json!({"on": t.on, "text": t.text, "align": t.align.name(), "font": t.font, "size": t.size, "bold": t.bold, "italic": t.italic})
}

fn json_of(hf: &HeaderFooter) -> Value {
    let items: Map<String, Value> = PAGE_TEXT_KEYS.iter().filter_map(|k| hf.get(k).map(|t| (k.to_string(), text_json(t)))).collect();
    let tokens: Vec<Value> = TOKENS.iter().map(|(t, what)| json!({"token": t, "description": what})).collect();
    json!({"items": items, "tokens": tokens})
}

fn header_footer(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    Ok(json_of(&s.score.header_footer))
}

/// Applies the fields of `fields` to `t`, checking each.
fn apply(t: &mut PageText, fields: &Value, item: &str) -> Result<(), EngineError> {
    let bad = |what: &str| EngineError::BadParam(format!("{item}: {what}"));
    if !fields.is_object() {
        return Err(bad("must be an object of fields"));
    }
    for (key, field) in [("on", &mut t.on), ("bold", &mut t.bold), ("italic", &mut t.italic)] {
        if fields.get(key).is_some_and(|v| !v.is_null()) {
            *field = p::boolean(fields, key)?;
        }
    }
    if let Some(v) = fields.get("text").filter(|v| !v.is_null()) {
        t.text = v.as_str().ok_or_else(|| bad("`text` must be a string"))?.to_string();
    }
    if let Some(v) = fields.get("font").filter(|v| !v.is_null()) {
        let font = v.as_str().map(str::trim).filter(|f| !f.is_empty()).ok_or_else(|| bad("`font` must be a font name"))?;
        t.font = font.to_string();
    }
    if let Some(v) = fields.get("align").filter(|v| !v.is_null()) {
        t.align = v.as_str().and_then(TextAlign::from_name).ok_or_else(|| bad("`align` must be \"left\", \"center\" or \"right\""))?;
    }
    if let Some(v) = fields.get("size").filter(|v| !v.is_null()) {
        let size = v.as_f64().map(|f| f as f32).filter(|f| f.is_finite() && PAGE_TEXT_SIZES.contains(f));
        t.size = size.ok_or_else(|| bad(&format!("`size` must be {} to {} points", PAGE_TEXT_SIZES.start(), PAGE_TEXT_SIZES.end())))?;
    }
    Ok(())
}

fn set_header_footer(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let mut next = s.score.header_footer.clone();
    let item = p::opt_string(params, "item");
    if params.get("item").is_some_and(|v| !v.is_null() && !v.is_string()) {
        return Err(EngineError::BadParam("`item` must be the name of a header or footer text".into()));
    }
    let known = |k: &str| PAGE_TEXT_KEYS.contains(&k);
    if let Some(k) = item.filter(|k| !known(k)) {
        return Err(EngineError::BadParam(format!("`item` {k:?} is not one of {}", PAGE_TEXT_KEYS.join(", "))));
    }
    if params.get("reset").is_some_and(|v| !v.is_null()) && p::boolean(params, "reset")? {
        let defaults = HeaderFooter::default();
        match item {
            Some(k) => {
                if let (Some(t), Some(d)) = (next.get_mut(k), defaults.get(k)) {
                    *t = d.clone();
                }
            }
            None => next = defaults,
        }
    }
    if let Some(k) = item
        && let Some(t) = next.get_mut(k)
    {
        apply(t, params, k)?;
    }
    if let Some(items) = params.get("items").filter(|v| !v.is_null()) {
        let items = items.as_object().ok_or_else(|| EngineError::BadParam("`items` must be an object of {item: fields}".into()))?;
        for (k, fields) in items {
            let t = next.get_mut(k).ok_or_else(|| EngineError::BadParam(format!("`items` has {k:?}: not a header or footer text")))?;
            apply(t, fields, k)?;
        }
    }
    next.check().map_err(EngineError::invalid)?;
    Arc::make_mut(&mut s.score).header_footer = next;
    Ok(json_of(&s.score.header_footer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_text_changes_and_the_rest_stay() {
        let mut s = Session::new();
        let r = s.run("score.headerFooter", &Value::Null).unwrap();
        assert_eq!(r["items"]["first.title"]["text"], "%TITLE%");
        assert_eq!(r["tokens"].as_array().map(Vec::len), Some(11));
        let r = s
            .run(
                "score.setHeaderFooter",
                &json!({"item": "first.music", "text": "Composed by %MUSIC%", "align": "left", "size": 11.5, "bold": true, "font": "Arial"}),
            )
            .unwrap();
        let m = &r["items"]["first.music"];
        assert_eq!(
            (m["text"].clone(), m["align"].clone(), m["size"].clone(), m["bold"].clone(), m["font"].clone()),
            (json!("Composed by %MUSIC%"), json!("left"), json!(11.5), json!(true), json!("Arial"))
        );
        assert_eq!(s.score().header_footer.first.words, HeaderFooter::default().first.words);
        s.run("edit.undo", &Value::Null).unwrap();
        assert!(s.score().header_footer.is_default());
    }

    #[test]
    fn several_texts_change_in_one_step_and_reset_restores_guitar_pros() {
        let mut s = Session::new();
        s.run("score.setHeaderFooter", &json!({"items": {"first.tabber": {"on": true}, "other.header": {"on": true, "align": "center"}}})).unwrap();
        let hf = &s.score().header_footer;
        assert!(hf.first.tabber.on && hf.other.header.on && hf.other.header.align == TextAlign::Center);
        s.run("score.setHeaderFooter", &json!({"item": "first.tabber", "reset": true})).unwrap();
        assert!(!s.score().header_footer.first.tabber.on && s.score().header_footer.other.header.on, "only the named text is reset");
        s.run("score.setHeaderFooter", &json!({"reset": true})).unwrap();
        assert!(s.score().header_footer.is_default());
    }

    #[test]
    fn hostile_texts_are_refused_and_change_nothing() {
        let mut s = Session::new();
        for bad in [
            json!({"item": "first.nothing", "on": true}),
            json!({"item": 3}),
            json!({"item": "first.title", "size": 0}),
            json!({"item": "first.title", "size": 1e40}),
            json!({"item": "first.title", "size": "big"}),
            json!({"item": "first.title", "align": "justify"}),
            json!({"item": "first.title", "on": "yes"}),
            json!({"item": "first.title", "font": ""}),
            json!({"item": "first.title", "font": "a\u{7}b"}),
            json!({"item": "first.title", "text": "x".repeat(guitario_score::MAX_PAGE_TEXT + 1)}),
            json!({"items": [1]}),
            json!({"items": {"other.nothing": {}}}),
            json!({"items": {"other.header": 5}}),
            json!({"items": {"other.header": {"on": true}, "first.title": {"size": -1}}}),
            json!({"reset": "yes"}),
        ] {
            assert!(s.run("score.setHeaderFooter", &bad).is_err(), "{bad}");
        }
        assert!(s.score().header_footer.is_default());
        assert!(!s.is_dirty());
    }

    #[test]
    fn headers_and_footers_are_saved_and_read_back() {
        let mut s = Session::new();
        s.run("score.setHeaderFooter", &json!({"item": "other.pageNumber", "text": "Page %PAGE% of %PAGES%", "italic": true})).unwrap();
        let bytes = guitario_format::to_bytes(s.score()).unwrap();
        let back = guitario_format::from_bytes(&bytes).unwrap();
        assert_eq!(back.header_footer, s.score().header_footer);
        // A score with Guitar Pro's texts writes none of them.
        let plain = guitario_format::to_bytes(&guitario_score::Score::new()).unwrap();
        assert!(!String::from_utf8_lossy(&plain).contains("header_footer"));
    }
}
