//! Printed pages. Drawing a score on a page needs the UI's engraving code, which the engine must
//! not depend on, so the shell registers a [`PageRenderer`] and `file.exportPdf` calls it.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::Score;
use serde_json::{Value, json};
use std::sync::Arc;

/// What to put on the pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfRequest {
    /// Tracks to print, in this order; each starts on a new page.
    pub tracks: Vec<usize>,
    /// Standard notation above the tab (Guitar Pro's default), or tab alone.
    pub notation: bool,
    /// A full score: the tracks stacked in every system, instead of one after another.
    pub together: bool,
    /// The first and last bar to print (indexes, both included); `None` prints every bar.
    pub bars: Option<(usize, usize)>,
}

impl PdfRequest {
    /// Every bar of `tracks`.
    pub fn new(tracks: Vec<usize>, notation: bool, together: bool) -> PdfRequest {
        PdfRequest { tracks, notation, together, bars: None }
    }
}

/// A finished document.
#[derive(Debug)]
pub struct PdfDocument {
    pub bytes: Vec<u8>,
    pub pages: usize,
    /// Things the page could not show (a missing font, a symbol that was left out).
    pub warnings: Vec<String>,
}

/// Draws a score on paper-sized pages and returns a PDF. Implemented by the UI crate.
pub trait PageRenderer: Send + Sync {
    fn render_pdf(&self, score: &Score, request: &PdfRequest) -> Result<PdfDocument, String>;
}

impl Session {
    /// Registers the renderer `file.exportPdf` uses.
    pub fn set_renderer(&mut self, renderer: Arc<dyn PageRenderer>) {
        self.renderer = Some(renderer);
    }

    pub fn renderer(&self) -> Option<&Arc<dyn PageRenderer>> {
        self.renderer.as_ref()
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd(
        "file.exportPdf",
        "Export PDF…",
        "File/Export",
        None,
        "path, tracks? (indexes, default every track), notation? (default true: notation over tab; false for tab only), \
         together? (default false: each track on its own pages; true stacks them in every system as a full score), \
         bars? ([first, last] bar indexes, both included; default every bar)",
        false,
        export_pdf,
    )]
}

/// The tracks a request names, checked: at least one, each real, none twice.
pub(crate) fn tracks_param(score: &Score, params: &Value) -> Result<Vec<usize>, EngineError> {
    let all = || (0..score.tracks.len()).collect::<Vec<_>>();
    let Some(list) = params.get("tracks").filter(|v| !v.is_null()) else { return Ok(all()) };
    let list = list.as_array().ok_or_else(|| EngineError::BadParam("`tracks` must be an array of track indexes".into()))?;
    let mut out: Vec<usize> = Vec::new();
    for v in list {
        let i = v
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .filter(|i| *i < score.tracks.len())
            .ok_or_else(|| EngineError::BadParam(format!("`tracks` has {v}: no such track")))?;
        if out.contains(&i) {
            return Err(EngineError::BadParam(format!("`tracks` lists track {i} twice")));
        }
        out.push(i);
    }
    if out.is_empty() { Err(EngineError::BadParam("`tracks` is empty".into())) } else { Ok(out) }
}

/// The `bars` of a request, checked: `[first, last]`, both real bars, first not after last.
pub(crate) fn bars_param(score: &Score, params: &Value) -> Result<Option<(usize, usize)>, EngineError> {
    let Some(v) = params.get("bars").filter(|v| !v.is_null()) else { return Ok(None) };
    let bad = || EngineError::BadParam(format!("`bars` must be [first, last] bar indexes below {}, first not after last", score.headers.len()));
    let pair = v.as_array().filter(|a| a.len() == 2).ok_or_else(bad)?;
    let index = |i: usize| pair.get(i).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).filter(|n| *n < score.headers.len());
    match (index(0), index(1)) {
        (Some(a), Some(b)) if a <= b => Ok(Some((a, b))),
        _ => Err(bad()),
    }
}

/// An optional true/false param, `default` when it is absent or null.
pub(crate) fn flag(params: &Value, key: &str, default: bool) -> Result<bool, EngineError> {
    match params.get(key).filter(|v| !v.is_null()) {
        None => Ok(default),
        Some(v) => v.as_bool().ok_or_else(|| EngineError::BadParam(format!("`{key}` must be true or false"))),
    }
}

fn export_pdf(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let path = p::string(params, "path")?;
    let request = PdfRequest {
        tracks: tracks_param(&s.score, params)?,
        notation: flag(params, "notation", true)?,
        together: flag(params, "together", false)?,
        bars: bars_param(&s.score, params)?,
    };
    let renderer = s.renderer.clone().ok_or_else(|| EngineError::invalid("this build has no page renderer, so it cannot make PDFs"))?;
    let doc = renderer.render_pdf(&s.score, &request).map_err(EngineError::invalid)?;
    std::fs::write(path, &doc.bytes)?;
    Ok(json!({"bytes": doc.bytes.len(), "pages": doc.pages, "warnings": doc.warnings}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in renderer that records what it was asked for.
    struct Fake;
    impl PageRenderer for Fake {
        fn render_pdf(&self, score: &Score, request: &PdfRequest) -> Result<PdfDocument, String> {
            if request.tracks.iter().any(|t| *t >= score.tracks.len()) {
                return Err("bad track".into());
            }
            Ok(PdfDocument {
                bytes: format!("%PDF fake tracks={:?} notation={} together={}", request.tracks, request.notation, request.together).into_bytes(),
                pages: request.tracks.len(),
                warnings: vec!["w".into()],
            })
        }
    }

    fn tmp(name: &str) -> String {
        std::env::temp_dir().join(format!("gt-render-{}-{name}", std::process::id())).display().to_string()
    }

    #[test]
    fn without_a_renderer_the_command_says_so_and_changes_nothing() {
        let mut s = Session::new();
        let e = s.run("file.exportPdf", &json!({"path": tmp("none.pdf")})).unwrap_err().to_string();
        assert!(e.contains("no page renderer"), "{e}");
        assert!(!s.is_dirty());
    }

    #[test]
    fn the_registered_renderer_gets_the_request_and_its_output_is_written() {
        let mut s = Session::new();
        s.run("track.add", &json!({"name": "Bass", "tuning": "bass"})).unwrap();
        s.set_renderer(Arc::new(Fake));
        let path = tmp("ok.pdf");
        let r = s.run("file.exportPdf", &json!({"path": path})).unwrap();
        assert_eq!((r["pages"].clone(), r["warnings"][0].clone()), (json!(2), json!("w")));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "%PDF fake tracks=[0, 1] notation=true together=false");
        let r = s.run("file.exportPdf", &json!({"path": path, "tracks": [1], "notation": false})).unwrap();
        assert_eq!(r["pages"], 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "%PDF fake tracks=[1] notation=false together=false");
        s.run("file.exportPdf", &json!({"path": path, "together": true})).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "%PDF fake tracks=[0, 1] notation=true together=true");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn hostile_params_are_refused_before_anything_is_written() {
        let mut s = Session::new();
        s.set_renderer(Arc::new(Fake));
        let path = tmp("bad.pdf");
        for bad in [
            json!({"path": path, "tracks": [5]}),
            json!({"path": path, "tracks": [-1]}),
            json!({"path": path, "tracks": [0, 0]}),
            json!({"path": path, "tracks": []}),
            json!({"path": path, "tracks": "all"}),
            json!({"path": path, "tracks": [1.5]}),
            json!({"path": path, "notation": "yes"}),
            json!({"path": path, "together": 1}),
            json!({}),
            json!({"path": 3}),
        ] {
            assert!(s.run("file.exportPdf", &bad).is_err(), "{bad}");
        }
        assert!(std::fs::metadata(&path).is_err());
        // A path that cannot be written is an error, not a panic.
        assert!(s.run("file.exportPdf", &json!({"path": "/no/such/folder/x.pdf"})).is_err());
    }
}
