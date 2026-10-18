//! The clipboard, as in Guitar Pro 8's Edit menu.
//!
//! - **Copy / Cut** (⌘C, ⌘X) take the selected beats of the cursor's track and voice, one fragment
//!   per bar; bar structure (time and key signatures) is not part of it.
//! - **All-Track Copy / Cut** (⇧⌘C, ⇧⌘X) take the bars of the selection (or the cursor's bar) on
//!   every track, with their headers (time and key signatures, repeats, sections); the cut removes
//!   the bars from the score, leaving one empty bar when every bar goes.
//! - **Paste** (⌘V): a one-bar clip overwrites from the cursor; a clip of several bars fills empty
//!   bars from the cursor's first, then inserts new bars for the rest; an all-track clip inserts
//!   its bars before the cursor's bar. Between drum and pitched tracks only the rhythm (as rests)
//!   is kept.
//! - **Paste Special…** pastes a number of times, inserting new bars or replacing what is there,
//!   into the tracks chosen.

use crate::commands::{CommandSpec, cmd};
use crate::session::voice_at_mut;
use crate::{EngineError, Session, params as p};
use guitario_score::{Beat, Measure, MeasureHeader, Score, Track, Voice};
use serde_json::{Value, json};
use std::sync::Arc;

/// Most bars one paste may add, so a hostile repeat count cannot make a huge score.
const MAX_PASTE_MEASURES: usize = 10_000;
/// Most times Paste Special repeats the clip.
const MAX_REPEATS: u64 = 100;

/// What was copied.
#[derive(Clone, Debug, PartialEq)]
pub enum Clip {
    /// Beats of one track's voice, one fragment per bar the selection touched.
    Beats {
        fragments: Vec<Vec<Beat>>,
        /// Copied from a drum track: pasting into a pitched track keeps only the rhythm.
        drums: bool,
    },
    /// Whole bars of every track, with their headers.
    Bars { headers: Vec<MeasureHeader>, tracks: Vec<TrackBars> },
}

/// One track's bars in an all-track clip.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackBars {
    pub measures: Vec<Measure>,
    pub drums: bool,
    pub strings: usize,
}

impl Clip {
    /// How many bars the clip covers.
    pub fn bars(&self) -> usize {
        match self {
            Clip::Beats { fragments, .. } => fragments.len(),
            Clip::Bars { headers, .. } => headers.len(),
        }
    }

    /// The clip holds whole bars of every track.
    pub fn is_all_tracks(&self) -> bool {
        matches!(self, Clip::Bars { .. })
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd("edit.copy", "Copy", "Edit", Some("Cmd+C"), "", false, copy),
        cmd("edit.cut", "Cut", "Edit", Some("Cmd+X"), "", true, cut),
        cmd("edit.paste", "Paste", "Edit", Some("Cmd+V"), "", true, paste),
        cmd("edit.copyAllTracks", "All-Track Copy", "Edit", Some("Cmd+Shift+C"), "", false, copy_all_tracks),
        cmd("edit.cutAllTracks", "All-Track Cut", "Edit", Some("Cmd+Shift+X"), "", true, cut_all_tracks),
        cmd(
            "edit.pasteSpecial",
            "Paste Special",
            "Edit",
            None,
            "count? (1..=100 times, default 1), mode? (insert: new bars before the cursor's bar | replace: overwrite from the cursor, flowing into the bars after; default replace), \
             tracks? ([track indices] to paste a one-track clip into, default the cursor's track; an all-track clip always goes to every track and is always inserted)",
            true,
            paste_special,
        ),
    ]
}

impl Session {
    /// What the clipboard holds, if anything.
    pub fn clipboard(&self) -> Option<&Clip> {
        self.clipboard.as_ref()
    }
}

/// The selection on the cursor's track, or the beat under the cursor.
fn range(s: &Session) -> ((usize, usize), (usize, usize)) {
    s.selection.filter(|x| x.track == s.cursor.track).map_or(((s.cursor.measure, s.cursor.beat), (s.cursor.measure, s.cursor.beat)), |x| x.ordered())
}

fn copy(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let ((m0, b0), (m1, b1)) = range(s);
    let track = s.score.tracks.get(s.cursor.track).ok_or_else(|| EngineError::invalid("no such track"))?;
    let mut fragments = Vec::new();
    for m in m0..=m1 {
        let beats = track.measures.get(m).and_then(|x| x.voices.get(s.cursor.voice)).map(|v| v.beats.as_slice()).unwrap_or_default();
        let start = if m == m0 { b0 } else { 0 };
        let end = if m == m1 { b1.saturating_add(1) } else { beats.len() };
        fragments.push(beats.get(start.min(beats.len())..end.min(beats.len())).unwrap_or_default().to_vec());
    }
    let count: usize = fragments.iter().map(Vec::len).sum();
    if count == 0 {
        return Err(EngineError::invalid("nothing to copy"));
    }
    s.clipboard = Some(Clip::Beats { fragments, drums: track.is_drums() });
    Ok(json!({"beats": count}))
}

fn cut(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let r = copy(s, params)?;
    clear_range(s)?;
    Ok(r)
}

/// Turns the selected beats (or the cursor beat) into rests, keeping their durations.
pub(crate) fn clear_range(s: &mut Session) -> Result<(), EngineError> {
    if s.selection.is_some() {
        for b in s.selected_beats_mut()? {
            b.notes.clear();
            b.rest = true;
        }
    } else {
        let b = s.beat_mut()?;
        b.notes.clear();
        b.rest = true;
    }
    Ok(())
}

/// The bars of the selection, or the cursor's bar.
fn bar_range(s: &Session) -> (usize, usize) {
    let ((m0, _), (m1, _)) = range(s);
    let last = s.score.headers.len().saturating_sub(1);
    (m0.min(last), m1.min(last))
}

fn copy_all_tracks(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let (m0, m1) = bar_range(s);
    let headers: Vec<MeasureHeader> = s.score.headers.get(m0..=m1).ok_or_else(|| EngineError::invalid("no such bars"))?.to_vec();
    let tracks: Vec<TrackBars> = s
        .score
        .tracks
        .iter()
        .map(|t| TrackBars { measures: t.measures.get(m0..=m1).unwrap_or_default().to_vec(), drums: t.is_drums(), strings: t.string_count() })
        .collect();
    let bars = headers.len();
    s.clipboard = Some(Clip::Bars { headers, tracks });
    Ok(json!({"bars": bars}))
}

fn cut_all_tracks(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let r = copy_all_tracks(s, params)?;
    let (m0, m1) = bar_range(s);
    let score = Arc::make_mut(&mut s.score);
    let keep_one = m0 == 0 && m1 + 1 >= score.headers.len();
    let first = score.headers.first().cloned();
    score.headers.drain(m0..=m1.min(score.headers.len().saturating_sub(1)));
    for t in &mut score.tracks {
        let end = (m1 + 1).min(t.measures.len());
        if m0 < end {
            t.measures.drain(m0..end);
        }
    }
    if keep_one && let Some(h) = first {
        crate::session::append_measure(score, h);
    }
    s.selection = None;
    s.cursor.measure = m0.min(s.score.headers.len().saturating_sub(1));
    s.cursor.beat = 0;
    Ok(r)
}

/// A header for a new bar like `h`: the same meter and key, none of its marks.
fn plain_header(h: &MeasureHeader) -> MeasureHeader {
    MeasureHeader {
        tempo: None,
        tempo_linear: false,
        repeat_open: false,
        repeat_close: 0,
        marker: None,
        section_letter: None,
        double_bar: false,
        endings: 0,
        targets: Vec::new(),
        jump: None,
        fermatas: Vec::new(),
        ..h.clone()
    }
}

/// Inserts bars with `headers` before bar `at`, empty on every track.
fn insert_bars(score: &mut Score, at: usize, headers: &[MeasureHeader]) {
    let at = at.min(score.headers.len());
    for (i, h) in headers.iter().enumerate() {
        for t in &mut score.tracks {
            let pos = (at + i).min(t.measures.len());
            t.measures.insert(pos, Measure::empty_for(h));
        }
        score.headers.insert(at + i, h.clone());
    }
}

/// `beats` made fit for `track`: only their rhythm between drum and pitched tracks, refused when a
/// note sits on a string the track does not have.
fn adapt(beats: &[Beat], from_drums: bool, track: &Track) -> Result<Vec<Beat>, EngineError> {
    if from_drums != track.is_drums() {
        return Ok(beats.iter().map(|b| Beat::rest(b.duration)).collect());
    }
    let strings = track.string_count();
    if beats.iter().flat_map(|b| &b.notes).any(|n| usize::from(n.string) >= strings) {
        return Err(EngineError::invalid(format!("the clipboard uses strings track {} does not have", track.name)));
    }
    Ok(beats.to_vec())
}

/// A bar of an all-track clip made fit for `track`: every voice keeps only its rhythm between
/// drum and pitched tracks, or when it uses strings the track does not have.
fn adapt_measure(m: &Measure, from: &TrackBars, track: &Track) -> Measure {
    let strings = track.string_count();
    let foreign = from.drums != track.is_drums() || m.voices.iter().flat_map(|v| &v.beats).flat_map(|b| &b.notes).any(|n| usize::from(n.string) >= strings);
    if !foreign {
        return m.clone();
    }
    Measure { voices: m.voices.iter().map(|v| Voice { beats: v.beats.iter().map(|b| Beat::rest(b.duration)).collect() }).collect(), ..m.clone() }
}

/// Overwrites `voice` from beat `at` with `clip`: whole beats covering the clip's length are
/// replaced, and any time left over becomes rests. Fails if the measure would overflow. Returns
/// the index just after the pasted beats.
fn paste_into(voice: &mut Voice, at: usize, clip: &[Beat], cap: u32) -> Result<usize, EngineError> {
    let at = at.min(voice.beats.len());
    let clip_ticks: u32 = clip.iter().map(|b| b.duration.ticks()).sum();
    let mut end = at;
    let mut removed = 0u32;
    while removed < clip_ticks
        && let Some(b) = voice.beats.get(end)
    {
        removed += b.duration.ticks();
        end += 1;
    }
    let before: u32 = voice.beats.iter().take(at).map(|b| b.duration.ticks()).sum();
    let after: u32 = voice.beats.iter().skip(end).map(|b| b.duration.ticks()).sum();
    let mut pad = removed.saturating_sub(clip_ticks);
    if before + clip_ticks.max(removed) + after > cap {
        return Err(EngineError::invalid("the clipboard does not fit in the measure"));
    }
    let mut fill = Vec::new();
    let mut guard = 0;
    while pad > 0 && guard < 64 {
        // The longest plain rest that fits what is left.
        let Some(d) = guitario_theory::NoteValue::ALL.into_iter().map(guitario_theory::Duration::new).find(|x| x.ticks() <= pad) else { break };
        pad -= d.ticks();
        fill.push(Beat::rest(d));
        guard += 1;
    }
    voice.beats.drain(at..end.min(voice.beats.len()));
    let insert: Vec<Beat> = clip.iter().cloned().chain(fill).collect();
    voice.beats.splice(at..at, insert);
    if voice.beats.is_empty() {
        return Err(EngineError::invalid("paste left an empty measure"));
    }
    Ok(at + clip.len())
}

/// Whether `track`'s bar `m` holds only rests in every voice.
fn bar_is_empty(score: &Score, track: usize, m: usize) -> bool {
    score.tracks.get(track).and_then(|t| t.measures.get(m)).is_some_and(|x| x.voices.iter().flat_map(|v| &v.beats).all(|b| b.rest))
}

/// Puts `frag` into voice `voice` of `track`'s bar `m` from beat `at`.
fn put(score: &mut Score, track: usize, m: usize, voice: usize, at: usize, frag: &[Beat]) -> Result<usize, EngineError> {
    let cap = score.headers.get(m).map_or(0, MeasureHeader::capacity);
    let v = voice_at_mut(score, track, m, voice).ok_or_else(|| EngineError::invalid("no such measure"))?;
    paste_into(v, at, frag, cap)
}

fn check_size(bars: usize) -> Result<(), EngineError> {
    if bars > MAX_PASTE_MEASURES {
        return Err(EngineError::invalid("the clipboard is too large"));
    }
    Ok(())
}

fn paste(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let clip = s.clipboard.clone().ok_or_else(|| EngineError::invalid("the clipboard is empty"))?;
    check_size(clip.bars())?;
    let c = s.cursor;
    let result = match &clip {
        Clip::Bars { headers, tracks } => paste_bars(s, headers, tracks, 1)?,
        Clip::Beats { fragments, drums } => {
            let track = s.score.tracks.get(c.track).ok_or_else(|| EngineError::invalid("no such track"))?;
            let frags: Vec<Vec<Beat>> = fragments.iter().map(|f| adapt(f, *drums, track)).collect::<Result<_, _>>()?;
            let score = Arc::make_mut(&mut s.score);
            if frags.len() <= 1 {
                // One bar: completes the cursor's bar from the cursor.
                let frag = frags.first().map(Vec::as_slice).unwrap_or_default();
                put(score, c.track, c.measure, c.voice, c.beat, frag)?;
            } else {
                // Several bars: the empty bars from the cursor's are filled first, then new bars
                // are inserted for the rest (before the next bar that has notes).
                let mut filling = true;
                for (i, frag) in frags.iter().enumerate() {
                    let m = c.measure + i;
                    filling = filling && m < score.headers.len() && bar_is_empty(score, c.track, m);
                    if !filling {
                        let template = score
                            .headers
                            .get(m.min(score.headers.len().saturating_sub(1)))
                            .map(plain_header)
                            .unwrap_or_else(|| MeasureHeader::new(Default::default()));
                        insert_bars(score, m, &[template]);
                    }
                    put(score, c.track, m, c.voice, if i == 0 && filling { c.beat } else { 0 }, frag)?;
                }
            }
            json!({"beats": frags.iter().map(Vec::len).sum::<usize>()})
        }
    };
    s.selection = None;
    Ok(result)
}

/// Inserts the bars of an all-track clip `times` times before the cursor's bar.
fn paste_bars(s: &mut Session, headers: &[MeasureHeader], tracks: &[TrackBars], times: usize) -> Result<Value, EngineError> {
    check_size(headers.len().saturating_mul(times))?;
    let at = s.cursor.measure;
    let score = Arc::make_mut(&mut s.score);
    for k in 0..times {
        let base = at + k * headers.len();
        insert_bars(score, base, headers);
        for (ti, track) in score.tracks.iter_mut().enumerate() {
            let Some(from) = tracks.get(ti) else { continue };
            for (i, m) in from.measures.iter().enumerate() {
                let adapted = adapt_measure(m, from, track);
                if let Some(slot) = track.measures.get_mut(base + i) {
                    *slot = adapted;
                }
            }
        }
    }
    Ok(json!({"bars": headers.len() * times}))
}

fn paste_special(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let clip = s.clipboard.clone().ok_or_else(|| EngineError::invalid("the clipboard is empty"))?;
    let times = usize::try_from(p::opt_uint(params, "count", MAX_REPEATS)?.unwrap_or(1)).unwrap_or(1);
    if times == 0 {
        return Err(EngineError::BadParam("`count` must be 1..=100".into()));
    }
    let insert = match p::opt_string(params, "mode") {
        None | Some("replace") => false,
        Some("insert") => true,
        Some(other) => return Err(EngineError::BadParam(format!("`mode` must be insert or replace, not `{other}`"))),
    };
    let targets: Vec<usize> = match params.get("tracks") {
        None | Some(Value::Null) => vec![s.cursor.track],
        Some(Value::Array(list)) => {
            let mut out = Vec::new();
            for v in list {
                let i = v.as_u64().and_then(|i| usize::try_from(i).ok()).filter(|i| *i < s.score.tracks.len());
                let i = i.ok_or_else(|| EngineError::BadParam("`tracks` must list track indices of this score".into()))?;
                if !out.contains(&i) {
                    out.push(i);
                }
            }
            if out.is_empty() {
                return Err(EngineError::BadParam("`tracks` must name at least one track".into()));
            }
            out
        }
        Some(_) => return Err(EngineError::BadParam("`tracks` must be a list of track indices".into())),
    };
    check_size(clip.bars().saturating_mul(times))?;
    let result = match &clip {
        Clip::Bars { headers, tracks } => {
            if params.get("tracks").is_some_and(|t| !t.is_null()) {
                return Err(EngineError::BadParam("an all-track clip goes to every track; leave out `tracks`".into()));
            }
            paste_bars(s, headers, tracks, times)?
        }
        Clip::Beats { fragments, drums } => {
            let c = s.cursor;
            let mut per_track: Vec<(usize, Vec<Vec<Beat>>)> = Vec::new();
            for &t in &targets {
                let track = s.score.tracks.get(t).ok_or_else(|| EngineError::invalid("no such track"))?;
                per_track.push((t, fragments.iter().map(|f| adapt(f, *drums, track)).collect::<Result<_, _>>()?));
            }
            let score = Arc::make_mut(&mut s.score);
            if insert {
                // New bars before the cursor's bar, one per fragment and repeat.
                let template = score.headers.get(c.measure).map(plain_header).unwrap_or_else(|| MeasureHeader::new(Default::default()));
                let bars = fragments.len() * times;
                insert_bars(score, c.measure, &vec![template; bars]);
                for (t, frags) in &per_track {
                    for k in 0..times {
                        for (i, frag) in frags.iter().enumerate() {
                            put(score, *t, c.measure + k * frags.len() + i, c.voice, 0, frag)?;
                        }
                    }
                }
            } else {
                for (t, frags) in &per_track {
                    replace_from(score, *t, (c.measure, c.beat), c.voice, frags, times)?;
                }
            }
            json!({"beats": fragments.iter().map(Vec::len).sum::<usize>() * times * targets.len()})
        }
    };
    s.selection = None;
    Ok(result)
}

/// Overwrites `track` from `(bar, beat)` with `frags`, `times` times one after the other. A
/// fragment that does not fit in what is left of its bar starts the next one; bars are added at
/// the end as needed.
fn replace_from(score: &mut Score, track: usize, start: (usize, usize), voice: usize, frags: &[Vec<Beat>], times: usize) -> Result<(), EngineError> {
    let (mut m, mut at) = start;
    for _ in 0..times {
        for (i, frag) in frags.iter().enumerate() {
            if i > 0 {
                (m, at) = (m + 1, 0);
            }
            loop {
                while score.headers.len() <= m {
                    let last = score.headers.last().cloned().unwrap_or_else(|| MeasureHeader::new(Default::default()));
                    crate::session::append_measure(score, last);
                }
                if score.headers.len() > start.0 + MAX_PASTE_MEASURES {
                    return Err(EngineError::invalid("the paste would add too many bars"));
                }
                match put(score, track, m, voice, at, frag) {
                    Ok(next) => {
                        at = next;
                        break;
                    }
                    // A fragment longer than a whole bar never fits.
                    Err(e) if at == 0 => return Err(e),
                    Err(_) => (m, at) = (m + 1, 0),
                }
            }
        }
        // The next repeat starts after this one, in the next bar if this one is full.
        let full = score.tracks.get(track).and_then(|t| t.measures.get(m)).and_then(|x| x.voices.get(voice)).is_some_and(|v| at >= v.beats.len());
        if full {
            (m, at) = (m + 1, 0);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_theory::{Duration, NoteValue, TICKS_PER_QUARTER};

    fn beat(f: u8, d: Duration) -> Beat {
        Beat { rest: false, notes: vec![guitario_score::Note::new(0, f)], ..Beat::rest(d) }
    }

    fn run(s: &mut Session, id: &str, p: Value) {
        s.run(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"));
    }

    /// Quarter notes on the first track from bar 0, beat 0, on the top string.
    fn quarters(s: &mut Session, frets: &[u8]) {
        run(s, "cursor.set", json!({"track": 0, "measure": 0, "beat": 0, "string": 0}));
        for f in frets {
            run(s, "beat.setDuration", json!({"value": 4}));
            run(s, "note.setFret", json!({"fret": f}));
            run(s, "cursor.right", Value::Null);
        }
    }

    fn frets(s: &Session, track: usize, m: usize) -> Vec<Option<u8>> {
        s.score().tracks[track].measures[m].voices[0].beats.iter().map(|b| b.notes.first().map(|n| n.fret)).collect()
    }

    fn with_bars(n: usize) -> Session {
        let mut s = Session::new();
        while s.score().headers.len() < n {
            run(&mut s, "measure.append", Value::Null);
        }
        s
    }

    #[test]
    fn paste_overwrites_and_pads() {
        let mut v = Voice { beats: vec![Beat::rest(Duration::new(NoteValue::Half)), Beat::rest(Duration::new(NoteValue::Half))] };
        let q = TICKS_PER_QUARTER;
        assert_eq!(paste_into(&mut v, 0, &[beat(3, Duration::QUARTER)], 4 * q).unwrap(), 1);
        // A quarter replaced the first half; a quarter rest fills the gap.
        assert_eq!(v.beats.iter().map(|b| b.duration.ticks()).collect::<Vec<_>>(), vec![q, q, 2 * q]);
        assert_eq!(v.ticks(), 4 * q);
        assert!(paste_into(&mut v, 2, &[beat(1, Duration::WHOLE)], 4 * q).is_err());
    }

    #[test]
    fn a_clip_of_several_bars_fills_empty_bars_first_then_inserts() {
        let mut s = with_bars(4);
        quarters(&mut s, &[1, 2, 3, 4, 5, 6, 7, 8]);
        run(&mut s, "select.set", json!({"fromMeasure": 0, "fromBeat": 0, "toMeasure": 1, "toBeat": 3}));
        run(&mut s, "edit.copy", Value::Null);
        // Bars 2 and 3 are empty: the paste fills them and adds nothing.
        run(&mut s, "cursor.set", json!({"measure": 2, "beat": 0}));
        run(&mut s, "edit.paste", Value::Null);
        assert_eq!(s.score().headers.len(), 4);
        assert_eq!((frets(&s, 0, 2), frets(&s, 0, 3)), (frets(&s, 0, 0), frets(&s, 0, 1)));
        // On a bar with notes the two bars go in before it.
        run(&mut s, "cursor.set", json!({"measure": 1, "beat": 0}));
        run(&mut s, "edit.paste", Value::Null);
        assert_eq!(s.score().headers.len(), 6);
        assert_eq!(frets(&s, 0, 1)[0], Some(1));
        assert_eq!(frets(&s, 0, 3)[0], Some(5), "the old bar 1 moved after the pasted bars");
        // On the last empty bar: it is filled, then one bar is added after it.
        let mut s2 = with_bars(2);
        quarters(&mut s2, &[1, 2, 3, 4, 5, 6, 7, 8]);
        run(&mut s2, "select.set", json!({"fromMeasure": 0, "fromBeat": 0, "toMeasure": 1, "toBeat": 3}));
        run(&mut s2, "edit.copy", Value::Null);
        run(&mut s2, "measure.append", Value::Null);
        run(&mut s2, "cursor.set", json!({"measure": 2, "beat": 0}));
        run(&mut s2, "edit.paste", Value::Null);
        assert_eq!(s2.score().headers.len(), 4);
        assert_eq!(frets(&s2, 0, 3)[0], Some(5));
        assert!(s2.score().validate().is_ok());
    }

    #[test]
    fn between_drums_and_strings_only_the_rhythm_is_pasted() {
        let mut s = Session::new();
        quarters(&mut s, &[5, 7]);
        run(&mut s, "select.set", json!({"fromMeasure": 0, "fromBeat": 0, "toMeasure": 0, "toBeat": 1}));
        run(&mut s, "edit.copy", Value::Null);
        run(&mut s, "track.addDrums", json!({}));
        let drums = s.score().tracks.len() - 1;
        run(&mut s, "cursor.set", json!({"track": drums, "measure": 0, "beat": 0}));
        run(&mut s, "edit.paste", Value::Null);
        let beats = &s.score().tracks[drums].measures[0].voices[0].beats;
        assert_eq!(beats.iter().take(2).map(|b| (b.rest, b.duration)).collect::<Vec<_>>(), vec![(true, Duration::QUARTER); 2]);
        assert!(s.score().validate().is_ok());
    }

    #[test]
    fn all_track_copy_and_cut_take_whole_bars_with_their_headers() {
        let mut s = with_bars(3);
        run(&mut s, "track.addDrums", json!({}));
        quarters(&mut s, &[1, 2, 3, 4]);
        run(&mut s, "cursor.set", json!({"measure": 0, "beat": 0}));
        run(&mut s, "measure.setTempo", json!({"bpm": 150}));
        run(&mut s, "edit.copyAllTracks", Value::Null);
        assert!(s.clipboard().is_some_and(Clip::is_all_tracks));
        // Pasted before bar 2: one bar more on every track, header and all.
        run(&mut s, "cursor.set", json!({"measure": 2, "beat": 0}));
        run(&mut s, "edit.paste", Value::Null);
        assert_eq!(s.score().headers.len(), 4);
        assert!(s.score().tracks.iter().all(|t| t.measures.len() == 4));
        assert_eq!((s.score().headers[2].tempo, frets(&s, 0, 2)[0]), (Some(150), Some(1)));
        // Cut removes the bars from every track; undo brings them back in one step.
        run(&mut s, "select.set", json!({"fromMeasure": 1, "fromBeat": 0, "toMeasure": 2, "toBeat": 0}));
        run(&mut s, "edit.cutAllTracks", Value::Null);
        assert_eq!(s.score().headers.len(), 2);
        assert!(s.score().tracks.iter().all(|t| t.measures.len() == 2) && s.score().validate().is_ok());
        run(&mut s, "edit.undo", Value::Null);
        assert_eq!(s.score().headers.len(), 4);
        // Cutting every bar leaves one empty bar.
        run(&mut s, "select.all", Value::Null);
        run(&mut s, "edit.cutAllTracks", Value::Null);
        assert_eq!(s.score().headers.len(), 1);
        assert!(bar_is_empty(s.score(), 0, 0) && s.score().validate().is_ok());
    }

    #[test]
    fn paste_special_repeats_replaces_or_inserts_into_the_tracks_chosen() {
        let mut s = Session::new();
        s.run("track.duplicate", &json!({})).ok();
        let two = s.score().tracks.len() >= 2;
        quarters(&mut s, &[5, 7]);
        run(&mut s, "select.set", json!({"fromMeasure": 0, "fromBeat": 0, "toMeasure": 0, "toBeat": 1}));
        run(&mut s, "edit.copy", Value::Null);
        // Replace three times from beat 2: two fill bar 0, the third flows into bar 1.
        run(&mut s, "cursor.set", json!({"measure": 0, "beat": 2}));
        run(&mut s, "edit.pasteSpecial", json!({"count": 3}));
        assert_eq!(frets(&s, 0, 0), [Some(5), Some(7), Some(5), Some(7)]);
        assert_eq!(frets(&s, 0, 1)[..4], [Some(5), Some(7), Some(5), Some(7)]);
        assert!(s.score().validate().is_ok());
        run(&mut s, "edit.undo", Value::Null);
        // Insert twice: two new bars before the cursor's, each holding the clip.
        let bars = s.score().headers.len();
        run(&mut s, "cursor.set", json!({"measure": 0, "beat": 0}));
        let tracks: Vec<usize> = (0..s.score().tracks.len()).collect();
        run(&mut s, "edit.pasteSpecial", json!({"count": 2, "mode": "insert", "tracks": tracks}));
        assert_eq!(s.score().headers.len(), bars + 2);
        assert_eq!(frets(&s, 0, 1)[..2], [Some(5), Some(7)]);
        if two {
            assert_eq!(frets(&s, 1, 0)[..2], [Some(5), Some(7)], "the second track was chosen too");
        }
        assert_eq!(frets(&s, 0, 2)[..2], [Some(5), Some(7)], "the original bar follows");
        assert!(s.score().validate().is_ok());
    }

    #[test]
    fn bad_paste_special_params_change_nothing() {
        let mut s = Session::new();
        assert!(s.run("edit.pasteSpecial", &json!({})).is_err(), "nothing copied");
        quarters(&mut s, &[5]);
        run(&mut s, "edit.copy", Value::Null);
        let before = (**s.score()).clone();
        for bad in [
            json!({"count": 0}),
            json!({"count": 101}),
            json!({"count": "x"}),
            json!({"mode": "merge"}),
            json!({"tracks": []}),
            json!({"tracks": [99]}),
            json!({"tracks": "all"}),
            json!({"tracks": [-1]}),
        ] {
            assert!(s.run("edit.pasteSpecial", &bad).is_err(), "{bad}");
            assert_eq!(**s.score(), before, "{bad}");
        }
        run(&mut s, "edit.copyAllTracks", Value::Null);
        assert!(s.run("edit.pasteSpecial", &json!({"tracks": [0]})).is_err(), "an all-track clip takes no tracks");
        assert_eq!(**s.score(), before);
    }

    #[test]
    fn a_huge_repeat_of_a_long_clip_is_refused() {
        let mut s = with_bars(200);
        run(&mut s, "select.all", Value::Null);
        run(&mut s, "edit.copyAllTracks", Value::Null);
        assert!(s.run("edit.pasteSpecial", &json!({"count": 100})).is_err(), "20,000 bars");
        assert_eq!(s.score().headers.len(), 200);
    }
}
