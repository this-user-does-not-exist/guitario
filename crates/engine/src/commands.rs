//! The command registry. Ids, labels and menu paths are the public surface: the UI builds its menus
//! from this list and the CLI and control channel dispatch by id.

use crate::session::{append_measure, fit_duration};
use crate::{EngineError, Session, params as p};
use guitario_score::{Beat, Dynamic, Harmonic, Measure, MeasureHeader, Note, NoteKind, Slide, Track};
use guitario_theory::{TimeSignature, Tuning};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

type Run = fn(&mut Session, &Value) -> Result<Value, EngineError>;

pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Menu path such as `Note/Effects`; empty when the command is keyboard-only.
    pub menu: &'static str,
    pub shortcut: Option<&'static str>,
    /// Parameters, for `commands` listings and docs.
    pub params: &'static str,
    /// Mutating commands are wrapped in an undo step and rolled back on error.
    pub mutates: bool,
    pub run: Run,
}

pub(crate) const fn cmd(
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    shortcut: Option<&'static str>,
    params: &'static str,
    mutates: bool,
    run: Run,
) -> CommandSpec {
    CommandSpec { id, label, menu, shortcut, params, mutates, run }
}

pub fn registry() -> &'static [CommandSpec] {
    &REGISTRY
}

static REGISTRY: std::sync::LazyLock<Vec<CommandSpec>> = std::sync::LazyLock::new(|| {
    CORE.into_iter()
        .chain(crate::selection::specs())
        .chain(crate::drums::specs())
        .chain(crate::bars::specs())
        .chain(crate::tracks::specs())
        .chain(crate::techniques::specs())
        .chain(crate::tools::specs())
        .chain(crate::exchange::specs())
        .chain(crate::chords::specs())
        .collect()
});

const CORE: [CommandSpec; 54] = [
    cmd("file.new", "New", "File", Some("Cmd+N"), "", false, file_new),
    cmd("file.open", "Open…", "File", Some("Cmd+O"), "path (.gt, or a Guitar Pro .gp/.gpx/.gp5/.gp4/.gp3 to import)", false, file_open),
    cmd("file.importGp", "Import Guitar Pro…", "File/Import", None, "path (.gp, .gpx, .gp5, .gp4 or .gp3)", false, import_gp),
    cmd("file.save", "Save", "File", Some("Cmd+S"), "path?", false, file_save),
    cmd("file.exportMidi", "Export MIDI…", "File/Export", None, "path", false, export_midi),
    cmd(
        "file.exportWav",
        "Export Audio (WAV)…",
        "File/Export",
        None,
        "path, sampleRate? (8000..=192000, default 44100), sounds? (a sound bank folder with bank.json: SFZ instruments for drums, bass and guitars)",
        false,
        export_wav,
    ),
    cmd("file.exportAscii", "Export ASCII Tab", "File/Export", None, "path?", false, export_ascii),
    cmd("edit.undo", "Undo", "Edit", Some("Cmd+Z"), "", false, undo),
    cmd("edit.redo", "Redo", "Edit", Some("Cmd+Shift+Z"), "", false, redo),
    cmd("cursor.left", "Previous Beat", "", Some("Left"), "", false, |s, _| {
        s.selection = None;
        s.move_left_cmd()
    }),
    cmd("cursor.right", "Next Beat", "", Some("Right"), "", false, |s, _| {
        s.selection = None;
        s.move_right().map(|()| s.cursor_json())
    }),
    cmd("cursor.up", "Higher String", "", Some("Up"), "", false, |s, _| s.move_string_cmd(-1)),
    cmd("cursor.down", "Lower String", "", Some("Down"), "", false, |s, _| s.move_string_cmd(1)),
    cmd("cursor.set", "Set Cursor", "", None, "track?, measure?, beat?, string?", false, cursor_set),
    cmd("note.setFret", "Set Fret", "Note", None, "fret (0..=36)", true, set_fret),
    cmd("note.clear", "Delete Note", "Note", Some("Backspace"), "", true, clear_note),
    cmd("note.setKind", "Note Type", "Note", None, "kind: normal|dead|tie", true, set_kind),
    cmd(
        "note.toggleEffect",
        "Toggle Effect",
        "Note/Effects",
        None,
        "effect: hammer|vibrato|wideVibrato|palmMute|letRing|ghost|accent|heavyAccent|staccato|tapping|leftHandTapping",
        true,
        toggle_effect,
    ),
    cmd("note.setBend", "Bend", "Note/Effects", None, "quarterTones (0 clears, max 12)", true, set_bend),
    cmd("note.setSlide", "Slide", "Note/Effects", None, "slide: shift|legato|intoFromBelow|intoFromAbove|outDown|outUp|none", true, set_slide),
    cmd("note.setHarmonic", "Harmonic", "Note/Effects", None, "harmonic: natural|artificial|pinch|tapped|semi|none", true, set_harmonic),
    cmd("beat.setDuration", "Duration", "Beat", None, "value (1..64 or name), dots?, tuplet?", true, set_duration),
    cmd("beat.lengthen", "Increase Duration", "Beat", Some("Plus"), "", true, |s, _| change_value(s, true)),
    cmd("beat.shorten", "Decrease Duration", "Beat", Some("Minus"), "", true, |s, _| change_value(s, false)),
    cmd("beat.toggleDot", "Dotted", "Beat", Some("Period"), "", true, toggle_dot),
    cmd("beat.toggleTriplet", "Triplet", "Beat", Some("Slash"), "", true, toggle_triplet),
    cmd("beat.toggleRest", "Rest", "Beat", Some("R"), "", true, toggle_rest),
    cmd("note.toggleTie", "Tied Note", "Note", Some("L"), "", true, |s, _| toggle_kind(s, NoteKind::Tie)),
    cmd("note.toggleDead", "Dead Note", "Note", Some("X"), "", true, |s, _| toggle_kind(s, NoteKind::Dead)),
    cmd("beat.setDynamic", "Dynamic", "Beat", None, "dynamic: ppp..fff", true, set_dynamic),
    cmd("beat.insert", "Insert Beat", "Beat", Some("Cmd+I"), "", true, insert_beat),
    cmd("beat.delete", "Delete Beat", "Beat", Some("Cmd+Backspace"), "", true, delete_beat),
    cmd("beat.setText", "Beat Text", "Beat", None, "text (empty clears)", true, set_text),
    cmd("measure.append", "Add Measure at End", "Measure", None, "", true, measure_append),
    cmd("measure.insert", "Insert Measure", "Measure", Some("Cmd+Shift+I"), "", true, measure_insert),
    cmd("measure.delete", "Delete Measure", "Measure", None, "", true, measure_delete),
    cmd("measure.setTimeSignature", "Time Signature", "Measure", None, "numerator, denominator", true, set_time),
    cmd("measure.setKey", "Key Signature", "Measure", None, "key (-7..=7 sharps)", true, set_key),
    cmd("measure.setTempo", "Tempo Change", "Measure", None, "bpm (null clears)", true, measure_tempo),
    cmd("measure.setRepeat", "Repeat", "Measure", None, "open?, close? (extra passes)", true, set_repeat),
    cmd("measure.setMarker", "Section Marker", "Measure", None, "text (empty clears)", true, set_marker),
    cmd("track.add", "Add Track", "Track", None, "name?, tuning? (preset name)", true, track_add),
    cmd("track.remove", "Remove Track", "Track", None, "", true, track_remove),
    cmd("track.rename", "Rename Track", "Track", None, "name", true, track_rename),
    cmd("track.setTuning", "Tuning", "Track", None, "preset | strings (highest first)", true, track_tuning),
    cmd("track.setCapo", "Capo", "Track", None, "fret (0..=24)", true, track_capo),
    cmd("track.setProgram", "Instrument", "Track", None, "program (0..=127)", true, track_program),
    cmd("track.setMix", "Mix", "Track", None, "volume?, pan?, muted?, solo?", true, track_mix),
    cmd("track.select", "Select Track", "Track", None, "index", false, track_select),
    cmd("score.setMeta", "Song Information", "File", None, "title?, artist?, album?, author?, notes?", true, set_meta),
    cmd("score.setTempo", "Tempo", "Measure", None, "bpm (20..=400)", true, set_tempo),
    cmd("score.info", "Score Info", "", None, "", false, info),
    cmd("score.validate", "Validate", "", None, "", false, |s, _| {
        s.score.validate().map(|()| json!({"ok": true})).map_err(|e| EngineError::invalid(e.to_string()))
    }),
    cmd("score.timeline", "Timeline", "", None, "", false, timeline_cmd),
];

impl Session {
    /// Runs a command by id. Mutating commands become one undo step; on error the score and
    /// cursor are exactly as before. Never panics on any params.
    pub fn run(&mut self, id: &str, params: &Value) -> Result<Value, EngineError> {
        let spec = REGISTRY.iter().find(|c| c.id == id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        if !spec.mutates {
            return (spec.run)(self, params);
        }
        let before = self.snapshot();
        match (spec.run)(self, params) {
            Ok(v) => {
                if *self.score != *before.0 {
                    self.push_undo(before);
                }
                self.clamp_cursor();
                self.validate_selection();
                Ok(v)
            }
            Err(e) => {
                self.restore(before);
                Err(e)
            }
        }
    }

    pub(crate) fn cursor_json(&self) -> Value {
        json!({"track": self.cursor.track, "measure": self.cursor.measure, "beat": self.cursor.beat, "string": self.cursor.string})
    }

    fn move_left_cmd(&mut self) -> Result<Value, EngineError> {
        self.move_left();
        Ok(self.cursor_json())
    }

    fn move_string_cmd(&mut self, d: i64) -> Result<Value, EngineError> {
        self.move_string(d);
        Ok(self.cursor_json())
    }

    fn track_mut(&mut self) -> Result<&mut Track, EngineError> {
        let i = self.cursor.track;
        Arc::make_mut(&mut self.score).tracks.get_mut(i).ok_or_else(|| EngineError::invalid("no such track"))
    }

    pub(crate) fn header_mut(&mut self) -> Result<&mut MeasureHeader, EngineError> {
        let i = self.cursor.measure;
        Arc::make_mut(&mut self.score).headers.get_mut(i).ok_or_else(|| EngineError::invalid("no such measure"))
    }

    fn note_mut(&mut self) -> Result<&mut Note, EngineError> {
        let string = self.cursor.string as u8;
        self.beat_mut()?.notes.iter_mut().find(|n| n.string == string).ok_or_else(|| EngineError::invalid("no note at the cursor"))
    }
}

fn ok() -> Result<Value, EngineError> {
    Ok(json!({"ok": true}))
}

// ---- files --------------------------------------------------------------------------------

fn file_new(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    s.replace_score(guitario_score::Score::new(), None);
    ok()
}

/// Opens a `.gt`, or imports a Guitar Pro file by its extension.
fn file_open(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let raw = p::string(params, "path")?;
    if guitario_gp::is_guitar_pro_path(raw) {
        return import_gp(s, params);
    }
    let path = PathBuf::from(raw);
    let score = guitario_format::open(&path)?;
    s.replace_score(score, Some(path));
    ok()
}

/// Imports any Guitar Pro version (told by the file's contents). The score stays unsaved, so a
/// later Save asks for a `.gt` path instead of overwriting the Guitar Pro file.
fn import_gp(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let import = guitario_gp::import_file(std::path::Path::new(p::string(params, "path")?))?;
    s.replace_score(import.score, None);
    s.dirty = true;
    Ok(json!({"warnings": import.warnings}))
}

fn file_save(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let path = match p::opt_string(params, "path") {
        Some(x) => PathBuf::from(x),
        None => s.path.clone().ok_or_else(|| EngineError::BadParam("`path` is required for an unsaved score".into()))?,
    };
    guitario_format::save(&s.score, &path)?;
    s.path = Some(path);
    s.dirty = false;
    ok()
}

fn export_midi(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let bytes = guitario_midi::export(&s.score)?;
    std::fs::write(p::string(params, "path")?, &bytes)?;
    Ok(json!({"bytes": bytes.len()}))
}

fn export_wav(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let path = p::string(params, "path")?.to_string();
    let rate = p::opt_uint(params, "sampleRate", 192_000)?.unwrap_or(44_100);
    if rate < 8_000 {
        return Err(EngineError::BadParam("`sampleRate` must be at least 8000".into()));
    }
    let (pcm, warnings): (Vec<f32>, Vec<String>) = match p::opt_string(params, "sounds") {
        #[cfg(not(target_arch = "wasm32"))]
        Some(dir) => {
            let bank = guitario_playback::SoundBank::open(std::path::Path::new(dir)).map_err(EngineError::BadParam)?;
            bank.render(&s.score, rate as u32)
        }
        #[cfg(target_arch = "wasm32")]
        Some(_) => return Err(EngineError::BadParam("`sounds` is not supported in the browser build".into())),
        None => (guitario_playback::render_wav(&s.score, rate as u32), Vec::new()),
    };
    let bytes = guitario_playback::wav_bytes(&pcm, rate as u32);
    std::fs::write(&path, &bytes)?;
    Ok(json!({"bytes": bytes.len(), "seconds": pcm.len() as f64 / 2.0 / rate as f64, "warnings": warnings}))
}

fn export_ascii(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let text = crate::ascii::render(&s.score);
    if let Some(path) = p::opt_string(params, "path") {
        std::fs::write(path, &text)?;
    }
    Ok(json!({"text": text}))
}

// ---- history and cursor ----------------------------------------------------------------------

fn undo(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let prev = s.undo.pop().ok_or_else(|| EngineError::invalid("nothing to undo"))?;
    let now = s.snapshot();
    s.redo.push(now);
    s.restore(prev);
    s.dirty = true;
    ok()
}

fn redo(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let next = s.redo.pop().ok_or_else(|| EngineError::invalid("nothing to redo"))?;
    let now = s.snapshot();
    s.undo.push(now);
    s.restore(next);
    s.dirty = true;
    ok()
}

fn cursor_set(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let c = s.cursor;
    if params.get("measure").is_some() || params.get("beat").is_some() || params.get("track").is_some() {
        s.selection = None;
    }
    s.cursor.track = p::opt_uint(params, "track", 10_000)?.map_or(c.track, |v| v as usize);
    s.cursor.measure = p::opt_uint(params, "measure", 1_000_000)?.map_or(c.measure, |v| v as usize);
    s.cursor.beat = p::opt_uint(params, "beat", 1_000_000)?.map_or(c.beat, |v| v as usize);
    s.cursor.string = p::opt_uint(params, "string", 100)?.map_or(c.string, |v| v as usize);
    s.clamp_cursor();
    Ok(s.cursor_json())
}

fn track_select(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let i = p::uint(params, "index", 10_000)? as usize;
    if i >= s.score.tracks.len() {
        return Err(EngineError::invalid("no such track"));
    }
    s.cursor.track = i;
    s.selection = None;
    s.clamp_cursor();
    Ok(s.cursor_json())
}

// ---- notes --------------------------------------------------------------------------------

fn set_fret(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let max = s.score.tracks.get(s.cursor.track).map_or(36, Track::max_fret);
    let fret = p::uint(params, "fret", u64::from(max))? as u8;
    let string = s.cursor.string as u8;
    let strings = s.track_mut()?.string_count();
    if usize::from(string) >= strings {
        return Err(EngineError::invalid("string out of range"));
    }
    let beat = s.beat_mut()?;
    beat.rest = false;
    match beat.notes.iter_mut().find(|n| n.string == string) {
        Some(n) => {
            n.fret = fret;
            if n.kind == NoteKind::Dead {
                n.kind = NoteKind::Normal;
            }
        }
        None => {
            beat.notes.push(Note::new(string, fret));
            beat.notes.sort_by_key(|n| n.string);
        }
    }
    ok()
}

fn clear_note(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    if s.selection.is_some() {
        crate::selection::clear_range(s)?;
        return ok();
    }
    let string = s.cursor.string as u8;
    let beat = s.beat_mut()?;
    beat.notes.retain(|n| n.string != string);
    beat.rest = beat.notes.is_empty();
    ok()
}

fn set_kind(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let kind: NoteKind = serde_json::from_value(params.get("kind").cloned().unwrap_or(Value::Null))
        .map_err(|_| EngineError::BadParam("`kind` must be normal, dead or tie".into()))?;
    s.note_mut()?.kind = kind;
    ok()
}

fn effect_flag<'a>(n: &'a mut Note, name: &str) -> Option<&'a mut bool> {
    Some(match name {
        "hammer" => &mut n.effects.hammer,
        "vibrato" => &mut n.effects.vibrato,
        "palmMute" => &mut n.effects.palm_mute,
        "letRing" => &mut n.effects.let_ring,
        "ghost" => &mut n.effects.ghost,
        "accent" => &mut n.effects.accent,
        "staccato" => &mut n.effects.staccato,
        "heavyAccent" => &mut n.effects.heavy_accent,
        "wideVibrato" => &mut n.effects.wide_vibrato,
        "tapping" => &mut n.effects.tapped,
        "leftHandTapping" => &mut n.effects.left_hand_tapped,
        _ => return None,
    })
}

/// Toggles an effect on the note at the cursor, or on every note of the selection (on for all
/// unless all already have it).
fn toggle_effect(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let name = p::string(params, "effect")?;
    if effect_flag(&mut Note::new(0, 0), name).is_none() {
        return Err(EngineError::BadParam(format!("unknown effect `{name}`")));
    }
    if s.selection.is_some() {
        let mut beats = s.selected_beats_mut()?;
        let mut notes: Vec<&mut Note> = beats.iter_mut().flat_map(|b| b.notes.iter_mut()).collect();
        if notes.is_empty() {
            return Err(EngineError::invalid("the selection has no notes"));
        }
        let all_on = notes.iter_mut().all(|n| effect_flag(n, name).is_some_and(|f| *f));
        for n in notes {
            if let Some(f) = effect_flag(n, name) {
                *f = !all_on;
            }
        }
        return ok();
    }
    let n = s.note_mut()?;
    if let Some(flag) = effect_flag(n, name) {
        *flag = !*flag;
    }
    ok()
}

fn set_bend(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let q = p::uint(params, "quarterTones", 12)? as u8;
    s.note_mut()?.effects.bend = (q > 0).then_some(q);
    ok()
}

fn enum_param<T: serde::de::DeserializeOwned>(params: &Value, key: &str) -> Result<Option<T>, EngineError> {
    let name = p::string(params, key)?;
    if name == "none" {
        return Ok(None);
    }
    // Accept camelCase as documented; the serde names are snake_case.
    let mut snake = String::new();
    for c in name.chars() {
        if c.is_uppercase() {
            snake.push('_');
        }
        snake.extend(c.to_lowercase());
    }
    serde_json::from_value(Value::String(snake)).map(Some).map_err(|_| EngineError::BadParam(format!("unknown `{key}` value `{name}`")))
}

fn set_slide(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let v: Option<Slide> = enum_param(params, "slide")?;
    s.note_mut()?.effects.slide = v;
    ok()
}

fn set_harmonic(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let v: Option<Harmonic> = enum_param(params, "harmonic")?;
    s.note_mut()?.effects.harmonic = v;
    ok()
}

// ---- beats --------------------------------------------------------------------------------

fn set_duration(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let d = p::duration(params)?;
    let cap = s.capacity();
    let idx = s.cursor.beat;
    let voice = s.voice_mut()?;
    let old = voice.beats.get(idx).map_or(0, |b| b.duration.ticks());
    if voice.ticks() - old + d.ticks() > cap {
        return Err(EngineError::invalid("that duration does not fit in the measure"));
    }
    voice.beats.get_mut(idx).ok_or_else(|| EngineError::invalid("the cursor is not on a beat"))?.duration = d;
    ok()
}

/// Replaces the cursor beat's duration, failing if the measure would overflow.
fn replace_duration(s: &mut Session, f: impl FnOnce(guitario_theory::Duration) -> guitario_theory::Duration) -> Result<Value, EngineError> {
    let cap = s.capacity();
    let idx = s.cursor.beat;
    let voice = s.voice_mut()?;
    let used = voice.ticks();
    let beat = voice.beats.get_mut(idx).ok_or_else(|| EngineError::invalid("the cursor is not on a beat"))?;
    let new = f(beat.duration);
    if used - beat.duration.ticks() + new.ticks() > cap {
        return Err(EngineError::invalid("that duration does not fit in the measure"));
    }
    beat.duration = new;
    ok()
}

fn change_value(s: &mut Session, longer: bool) -> Result<Value, EngineError> {
    replace_duration(s, |d| guitario_theory::Duration { value: if longer { d.value.longer() } else { d.value.shorter() }, ..d })
}

fn toggle_dot(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    replace_duration(s, |d| d.dotted(if d.dots == 0 { 1 } else { 0 }))
}

fn toggle_triplet(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    replace_duration(s, |d| guitario_theory::Duration { tuplet: if d.tuplet == (1, 1) { (3, 2) } else { (1, 1) }, ..d })
}

fn toggle_rest(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let beat = s.beat_mut()?;
    beat.notes.clear();
    beat.rest = true;
    ok()
}

fn toggle_kind(s: &mut Session, kind: NoteKind) -> Result<Value, EngineError> {
    let n = s.note_mut()?;
    n.kind = if n.kind == kind { NoteKind::Normal } else { kind };
    ok()
}

fn set_dynamic(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let d: Dynamic = serde_json::from_value(Value::String(p::string(params, "dynamic")?.to_lowercase()))
        .map_err(|_| EngineError::BadParam("`dynamic` must be ppp, pp, p, mp, mf, f, ff or fff".into()))?;
    if s.selection.is_some() {
        for b in s.selected_beats_mut()? {
            b.dynamic = d;
        }
        return ok();
    }
    s.beat_mut()?.dynamic = d;
    ok()
}

fn insert_beat(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let cap = s.capacity();
    let idx = s.cursor.beat;
    let voice = s.voice_mut()?;
    let used = voice.ticks();
    if used >= cap {
        return Err(EngineError::invalid("the measure is full"));
    }
    let prefer = voice.beats.get(idx).map_or(guitario_theory::Duration::QUARTER, |b| b.duration);
    voice.beats.insert((idx + 1).min(voice.beats.len()), Beat::rest(fit_duration(cap - used, prefer)));
    s.cursor.beat = idx + 1;
    ok()
}

fn delete_beat(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let idx = s.cursor.beat;
    let (m, header) = (s.cursor.measure, s.score.headers.get(s.cursor.measure).cloned());
    let voice = s.voice_mut()?;
    if idx >= voice.beats.len() {
        return Err(EngineError::invalid("the cursor is not on a beat"));
    }
    voice.beats.remove(idx);
    if voice.beats.is_empty() {
        let header = header.ok_or_else(|| EngineError::invalid(format!("no measure {m}")))?;
        *voice = Measure::empty_for(&header).voices.remove(0);
    }
    ok()
}

fn set_text(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let t = p::string(params, "text")?;
    s.beat_mut()?.text = (!t.is_empty()).then(|| t.to_string());
    ok()
}

// ---- measures -----------------------------------------------------------------------------

fn measure_append(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let header = s.score.headers.last().cloned().unwrap_or_else(|| MeasureHeader::new(Default::default()));
    append_measure(Arc::make_mut(&mut s.score), header);
    ok()
}

fn measure_insert(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let at = s.cursor.measure;
    let score = Arc::make_mut(&mut s.score);
    let template = score.headers.get(at).cloned().ok_or_else(|| EngineError::invalid("no such measure"))?;
    let fresh = MeasureHeader {
        tempo: None,
        repeat_open: false,
        repeat_close: 0,
        marker: None,
        double_bar: false,
        endings: 0,
        targets: Vec::new(),
        jump: None,
        ..template
    };
    for t in &mut score.tracks {
        t.measures.insert(at, Measure::empty_for(&fresh));
    }
    score.headers.insert(at, fresh);
    s.cursor.beat = 0;
    ok()
}

fn measure_delete(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let at = s.cursor.measure;
    let score = Arc::make_mut(&mut s.score);
    if score.headers.len() <= 1 {
        return Err(EngineError::invalid("a score needs at least one measure"));
    }
    if at >= score.headers.len() {
        return Err(EngineError::invalid("no such measure"));
    }
    score.headers.remove(at);
    for t in &mut score.tracks {
        if at < t.measures.len() {
            t.measures.remove(at);
        }
    }
    ok()
}

fn set_time(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let ts = TimeSignature::new(p::uint(params, "numerator", 32)? as u8, p::uint(params, "denominator", 64)? as u8)
        .ok_or_else(|| EngineError::BadParam("invalid time signature".into()))?;
    let at = s.cursor.measure;
    let score = Arc::make_mut(&mut s.score);
    let header = score.headers.get_mut(at).ok_or_else(|| EngineError::invalid("no such measure"))?;
    header.time_signature = ts;
    let header = header.clone();
    for t in &mut score.tracks {
        let Some(m) = t.measures.get_mut(at) else { continue };
        for v in &mut m.voices {
            let lone_rest = v.beats.len() == 1 && v.beats.first().is_some_and(|b| b.rest);
            if lone_rest {
                *v = Measure::empty_for(&header).voices.remove(0);
            } else if v.ticks() > header.capacity() {
                return Err(EngineError::invalid("existing notes do not fit the new time signature"));
            }
        }
    }
    ok()
}

fn set_key(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    s.header_mut()?.key = p::int(params, "key", -7, 7)? as i8;
    ok()
}

fn measure_tempo(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let bpm = if params.get("bpm").is_none_or(Value::is_null) { None } else { Some(p::uint(params, "bpm", 400)?) };
    if bpm.is_some_and(|b| b < 20) {
        return Err(EngineError::BadParam("`bpm` must be 20..=400".into()));
    }
    s.header_mut()?.tempo = bpm.map(|b| b as u16);
    ok()
}

fn set_repeat(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let open = params.get("open").map(|_| p::boolean(params, "open")).transpose()?;
    let close = p::opt_uint(params, "close", 32)?;
    let h = s.header_mut()?;
    if let Some(o) = open {
        h.repeat_open = o;
    }
    if let Some(c) = close {
        h.repeat_close = c as u8;
    }
    ok()
}

fn set_marker(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let t = p::string(params, "text")?;
    s.header_mut()?.marker = (!t.is_empty()).then(|| t.to_string());
    ok()
}

// ---- tracks -------------------------------------------------------------------------------

fn track_add(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let tuning = match p::opt_string(params, "tuning") {
        Some(name) => Tuning::preset(name).ok_or_else(|| EngineError::BadParam(format!("unknown tuning preset `{name}`")))?,
        None => Tuning::standard(),
    };
    let score = Arc::make_mut(&mut s.score);
    if score.tracks.len() >= 15 {
        return Err(EngineError::invalid("a score holds at most 15 tracks"));
    }
    let name = p::opt_string(params, "name").map_or_else(|| format!("Track {}", score.tracks.len() + 1), str::to_string);
    let track = Track::new(name, tuning, &score.headers);
    score.tracks.push(track);
    s.cursor.track = s.score.tracks.len() - 1;
    ok()
}

fn track_remove(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let i = s.cursor.track;
    let score = Arc::make_mut(&mut s.score);
    if score.tracks.len() <= 1 {
        return Err(EngineError::invalid("a score needs at least one track"));
    }
    if i >= score.tracks.len() {
        return Err(EngineError::invalid("no such track"));
    }
    score.tracks.remove(i);
    ok()
}

fn track_rename(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    s.track_mut()?.name = p::string(params, "name")?.chars().take(100).collect();
    ok()
}

fn track_tuning(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let tuning = if let Some(name) = p::opt_string(params, "preset") {
        Tuning::preset(name).ok_or_else(|| EngineError::BadParam(format!("unknown tuning preset `{name}`")))?
    } else {
        let strings: Vec<u8> = serde_json::from_value(params.get("strings").cloned().unwrap_or(Value::Null))
            .map_err(|_| EngineError::BadParam("give `preset` or `strings` (array of MIDI pitches)".into()))?;
        Tuning::new("Custom", strings).map_err(|e| EngineError::BadParam(e.to_string()))?
    };
    let n = tuning.string_count();
    let track = s.track_mut()?;
    if track.is_drums() {
        return Err(EngineError::invalid("a drum track has kit lanes, not a tuning"));
    }
    if track.measures.iter().flat_map(|m| &m.voices).flat_map(|v| &v.beats).flat_map(|b| &b.notes).any(|note| usize::from(note.string) >= n) {
        return Err(EngineError::invalid("existing notes use strings the new tuning does not have"));
    }
    track.tuning = tuning;
    ok()
}

fn track_capo(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let capo = p::uint(params, "fret", 24)? as u8;
    let track = s.track_mut()?;
    if track.is_drums() && capo != 0 {
        return Err(EngineError::invalid("a drum track has no capo"));
    }
    track.capo = capo;
    ok()
}

fn track_program(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    s.track_mut()?.program = p::uint(params, "program", 127)? as u8;
    ok()
}

fn track_mix(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let volume = p::opt_uint(params, "volume", 127)?;
    let pan = params.get("pan").map(|_| p::int(params, "pan", -63, 63)).transpose()?;
    let muted = params.get("muted").map(|_| p::boolean(params, "muted")).transpose()?;
    let solo = params.get("solo").map(|_| p::boolean(params, "solo")).transpose()?;
    let t = s.track_mut()?;
    if let Some(v) = volume {
        t.volume = v as u8;
    }
    if let Some(v) = pan {
        t.pan = v as i8;
    }
    if let Some(v) = muted {
        t.muted = v;
    }
    if let Some(v) = solo {
        t.solo = v;
    }
    ok()
}

// ---- score --------------------------------------------------------------------------------

fn set_meta(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let meta = &mut Arc::make_mut(&mut s.score).meta;
    for (key, field) in
        [("title", &mut meta.title), ("artist", &mut meta.artist), ("album", &mut meta.album), ("author", &mut meta.author), ("notes", &mut meta.notes)]
    {
        if params.get(key).is_some() {
            *field = p::string(params, key)?.chars().take(10_000).collect();
        }
    }
    ok()
}

fn set_tempo(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let bpm = p::uint(params, "bpm", 400)?;
    if bpm < 20 {
        return Err(EngineError::BadParam("`bpm` must be 20..=400".into()));
    }
    Arc::make_mut(&mut s.score).tempo = bpm as u16;
    ok()
}

fn info(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let sc = &s.score;
    Ok(json!({
        "title": sc.meta.title, "artist": sc.meta.artist, "tempo": sc.tempo, "measures": sc.headers.len(),
        "tracks": sc.tracks.iter().map(|t| json!({"name": t.name, "drums": t.is_drums(), "tuning": t.tuning.name, "strings": t.string_count(), "capo": t.capo, "program": t.program})).collect::<Vec<_>>(),
        "cursor": s.cursor_json(), "dirty": s.dirty, "path": s.path.as_ref().map(|p| p.display().to_string()),
        "canUndo": s.can_undo(), "canRedo": s.can_redo(),
    }))
}

fn timeline_cmd(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let t = guitario_score::timeline(&s.score);
    Ok(json!(t.iter().take(100_000).map(|n| json!({"track": n.track, "measure": n.measure, "tick": n.start_tick, "ticks": n.ticks, "string": n.string, "pitch": n.pitch.map(|p| p.0), "velocity": n.velocity, "dead": n.dead})).collect::<Vec<_>>()))
}
