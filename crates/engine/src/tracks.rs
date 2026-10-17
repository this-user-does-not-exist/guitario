//! Track tools: information (name, short name, colour), duplicate, reorder and transpose.

use crate::commands::{CommandSpec, cmd};
use crate::params;
use crate::{EngineError, Session};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "track.setInfo",
            "Track Information",
            "Track",
            None,
            "name?, shortName? (\"\" abbreviates the name), color? (\"#rrggbb\", or null for the default), track? (index; default the cursor's)",
            true,
            set_info,
        ),
        cmd(
            "track.setNotation",
            "Notation",
            "Track",
            None,
            "standard?, tab?, slash?, numbered? (booleans), track? (index; default the cursor's); at least one stays on",
            true,
            set_notation,
        ),
        cmd(
            "track.setInterpretation",
            "Interpretation",
            "Track",
            None,
            "style? (default|pick|fingers|picking|bassSlap), palmMute? (0..=100), accentuation? (0..=100), autoLetRing?, autoBrush?, stringed?, track? (index; default the cursor's)",
            true,
            set_interpretation,
        ),
        cmd("track.duplicate", "Duplicate Track", "Track", None, "", true, duplicate),
        cmd(
            "track.transpose",
            "Transpose Track…",
            "Track",
            None,
            "semitones (-24..=24), tracks? (indexes, or \"all\" for every track but drums; default the cursor's)",
            true,
            transpose,
        ),
        cmd("track.moveUp", "Move Track Up", "Track", None, "", true, |s, _| shift(s, -1)),
        cmd("track.moveDown", "Move Track Down", "Track", None, "", true, |s, _| shift(s, 1)),
    ]
}

/// A colour written `#rrggbb`.
fn hex_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
    Some([byte(0)?, byte(2)?, byte(4)?])
}

fn set_info(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let name = p.get("name").map(|_| params::string(p, "name")).transpose()?;
    if name.is_some_and(|n| n.trim().is_empty()) {
        return Err(EngineError::BadParam("`name` cannot be empty".into()));
    }
    let short = p.get("shortName").map(|_| params::string(p, "shortName")).transpose()?;
    let color = match p.get("color") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(c)) => Some(Some(hex_color(c).ok_or_else(|| EngineError::BadParam(format!("`color` must be #rrggbb, not `{c}`")))?)),
        Some(_) => return Err(EngineError::BadParam("`color` must be a \"#rrggbb\" string or null".into())),
    };
    let t = crate::commands::mixer_track(s, p)?;
    if let Some(n) = name {
        t.name = n.trim().chars().take(100).collect();
    }
    if let Some(n) = short {
        t.short_name = n.trim().chars().take(100).collect();
    }
    if let Some(c) = color {
        t.color = c;
    }
    Ok(json!({}))
}

fn set_notation(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let flag = |key: &str| p.get(key).map(|_| params::boolean(p, key)).transpose();
    let (standard, tab, slash, numbered) = (flag("standard")?, flag("tab")?, flag("slash")?, flag("numbered")?);
    let t = crate::commands::mixer_track(s, p)?;
    let mut n = t.notation;
    for (field, value) in [(&mut n.standard, standard), (&mut n.tab, tab), (&mut n.slash, slash), (&mut n.numbered, numbered)] {
        if let Some(v) = value {
            *field = v;
        }
    }
    if n.bits() == 0 {
        return Err(EngineError::invalid("a track shows at least one notation"));
    }
    t.notation = n;
    Ok(json!({"standard": n.standard, "tab": n.tab, "slash": n.slash, "numbered": n.numbered}))
}

fn set_interpretation(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    use guitario_score::PlayingStyle as P;
    let style = match params::opt_string(p, "style") {
        None => None,
        Some("default") => Some(P::Default),
        Some("pick") => Some(P::Pick),
        Some("fingers") => Some(P::Fingers),
        Some("picking") => Some(P::Picking),
        Some("bassSlap") => Some(P::BassSlap),
        Some(other) => return Err(EngineError::BadParam(format!("unknown playing style `{other}`"))),
    };
    if p.get("style").is_some_and(|v| !v.is_string()) {
        return Err(EngineError::BadParam("`style` must be a string".into()));
    }
    let palm = params::opt_uint(p, "palmMute", 100)?;
    let accent = params::opt_uint(p, "accentuation", 100)?;
    let flag = |key: &str| p.get(key).map(|_| params::boolean(p, key)).transpose();
    let (let_ring, brush, stringed) = (flag("autoLetRing")?, flag("autoBrush")?, flag("stringed")?);
    let t = crate::commands::mixer_track(s, p)?;
    if t.is_drums() {
        return Err(EngineError::invalid("a drum kit has no interpretation settings"));
    }
    let i = &mut t.interpretation;
    if let Some(v) = style {
        i.style = v;
    }
    if let Some(v) = palm {
        i.palm_mute = v as u8;
    }
    if let Some(v) = accent {
        i.accentuation = v as u8;
    }
    for (field, value) in [(&mut i.auto_let_ring, let_ring), (&mut i.auto_brush, brush), (&mut i.stringed, stringed)] {
        if let Some(v) = value {
            *field = v;
        }
    }
    Ok(json!({}))
}

fn duplicate(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let i = s.cursor.track;
    let score = Arc::make_mut(&mut s.score);
    if score.tracks.len() >= 15 {
        return Err(EngineError::invalid("a score holds at most 15 tracks"));
    }
    let mut copy = score.tracks.get(i).cloned().ok_or_else(|| EngineError::invalid("no such track"))?;
    copy.name = format!("{} (copy)", copy.name).chars().take(100).collect();
    copy.solo = false;
    score.tracks.insert(i + 1, copy);
    s.cursor.track = i + 1;
    s.selection = None;
    Ok(json!({"track": i + 1}))
}

fn shift(s: &mut Session, d: i64) -> Result<Value, EngineError> {
    let i = s.cursor.track;
    let n = s.score.tracks.len();
    let j = i as i64 + d;
    if i >= n || j < 0 || j as usize >= n {
        return Err(EngineError::invalid(if d < 0 { "the track is already first" } else { "the track is already last" }));
    }
    Arc::make_mut(&mut s.score).tracks.swap(i, j as usize);
    s.cursor.track = j as usize;
    s.selection = None;
    Ok(json!({"track": j}))
}

/// Moves every note of the chosen tracks by `semitones`. A note keeps its string while its fret
/// stays in range; otherwise it moves to the free string nearest its own that can play the pitch.
/// A trill or grace note keeps its distance in frets from its note. Nothing changes unless every
/// note of every track fits.
fn transpose(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let semitones = params::int(p, "semitones", -24, 24)?;
    let count = s.score.tracks.len();
    let chosen: Vec<usize> = match p.get("tracks") {
        None | Some(Value::Null) => vec![s.cursor.track],
        Some(Value::String(all)) if all == "all" => (0..count).filter(|i| s.score.tracks.get(*i).is_some_and(|t| !t.is_drums())).collect(),
        Some(Value::Array(list)) => list
            .iter()
            .map(|v| {
                v.as_u64()
                    .and_then(|i| usize::try_from(i).ok())
                    .filter(|i| *i < count)
                    .ok_or_else(|| EngineError::BadParam(format!("`tracks` has {v}: no such track")))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(EngineError::BadParam("`tracks` must be track indexes or \"all\"".into())),
    };
    if chosen.is_empty() {
        return Err(EngineError::invalid("no track to transpose"));
    }
    let mut done = Vec::with_capacity(chosen.len());
    let mut notes = 0;
    for &i in &chosen {
        let track = s.score.tracks.get(i).ok_or_else(|| EngineError::invalid("no such track"))?;
        let (copy, n) = transposed(track, semitones).map_err(|e| EngineError::invalid(format!("{}: {e}", track.name)))?;
        notes += n;
        done.push((i, copy));
    }
    let score = Arc::make_mut(&mut s.score);
    for (i, copy) in done {
        if let Some(t) = score.tracks.get_mut(i) {
            *t = copy;
        }
    }
    Ok(json!({"notes": notes, "semitones": semitones, "tracks": chosen}))
}

/// `track` with every note `semitones` away, and how many notes moved; why not, when one cannot.
fn transposed(track: &guitario_score::Track, semitones: i64) -> Result<(guitario_score::Track, usize), String> {
    if track.is_drums() {
        return Err("a drum track cannot be transposed".into());
    }
    let mut copy = track.clone();
    let (max, capo) = (i64::from(copy.max_fret()), i64::from(copy.capo));
    let open: Vec<i64> = copy.tuning.strings.iter().map(|p| i64::from(*p)).collect();
    let place = |pitch: i64, string: usize| -> Option<i64> {
        let fret = pitch - open.get(string).copied()? - capo;
        (0..=max).contains(&fret).then_some(fret)
    };
    let off = || "a note would leave the fretboard".to_string();
    let mut count = 0usize;
    for beat in copy.measures.iter_mut().flat_map(|m| m.voices.iter_mut()).flat_map(|v| v.beats.iter_mut()) {
        let pitches: Vec<i64> =
            beat.notes.iter().map(|n| open.get(usize::from(n.string)).copied().unwrap_or(0) + i64::from(n.fret) + capo + semitones).collect();
        // Pass 1: notes that stay on their string. Pass 2: the others find a free string.
        let mut targets: Vec<Option<(usize, i64)>> =
            beat.notes.iter().zip(&pitches).map(|(n, pitch)| place(*pitch, usize::from(n.string)).map(|f| (usize::from(n.string), f))).collect();
        let mut taken: Vec<usize> = targets.iter().flatten().map(|(s, _)| *s).collect();
        for ((note, pitch), target) in beat.notes.iter().zip(&pitches).zip(&mut targets) {
            if target.is_none() {
                let mut order: Vec<usize> = (0..open.len()).filter(|s| !taken.contains(s)).collect();
                order.sort_by_key(|s| (*s as i64 - i64::from(note.string)).abs());
                let found = order.into_iter().find_map(|s| place(*pitch, s).map(|f| (s, f))).ok_or_else(off)?;
                taken.push(found.0);
                *target = Some(found);
            }
        }
        for (note, target) in beat.notes.iter_mut().zip(targets) {
            let (string, fret) = target.ok_or_else(off)?;
            let shift = fret - i64::from(note.fret);
            let ornaments = note.effects.trill.iter_mut().map(|t| &mut t.fret).chain(note.effects.grace.iter_mut().map(|g| &mut g.fret));
            for f in ornaments {
                let moved = i64::from(*f) + shift;
                *f = u8::try_from(moved).ok().filter(|m| i64::from(*m) <= max).ok_or_else(off)?;
            }
            note.string = u8::try_from(string).map_err(|_| off())?;
            note.fret = u8::try_from(fret).map_err(|_| off())?;
            count += 1;
        }
    }
    Ok((copy, count))
}
