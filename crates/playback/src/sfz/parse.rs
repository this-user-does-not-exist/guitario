//! SFZ text to regions. Pure: no file access (includes come through a resolver), so it builds for
//! wasm and can be fuzzed with hostile text.
//!
//! Supported: `<control> <global> <master> <group> <region>` inheritance, `#define`/`$VAR`,
//! `#include`, `default_path`, `set_ccN`, `_onccN`/`_ccN` modulation evaluated against the CC
//! defaults, `locc/hicc` region conditions, key and velocity ranges, `pitch_keycenter`,
//! `tune/transpose/volume/amplitude/pan`, `offset/end`, loops, an AHDSR amplitude envelope,
//! `group/off_by/off_mode`, `trigger`, round robin (`seq_*`, `lorand/hirand`) and the default
//! keyswitch (`sw_*`). Anything else is ignored, never an error.

use std::collections::HashMap;

const MAX_TEXT: usize = 8 * 1024 * 1024;
const MAX_REGIONS: usize = 200_000;
const MAX_INCLUDES: usize = 8192;
const MAX_DEPTH: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    NoLoop,
    OneShot,
    LoopContinuous,
    LoopSustain,
}

/// One playable zone, fully resolved (inheritance and CC modulation already applied).
#[derive(Clone, Debug)]
pub struct Region {
    /// Sample path relative to the instrument's base directory, `/`-separated.
    pub sample: String,
    pub lokey: u8,
    pub hikey: u8,
    pub keycenter: u8,
    pub lovel: u8,
    pub hivel: u8,
    pub lorand: f32,
    pub hirand: f32,
    pub seq_length: u32,
    pub seq_position: u32,
    /// Keyswitch: the region plays only while this key is the active switch (None = always).
    pub sw_last: Option<u8>,
    pub volume_db: f32,
    pub pan: f32,
    pub tune_cents: f32,
    pub keytrack: f32,
    pub veltrack: f32,
    pub offset: u64,
    pub end: Option<u64>,
    pub loop_mode: LoopMode,
    pub loop_start: Option<u64>,
    pub loop_end: Option<u64>,
    pub delay: f32,
    pub attack: f32,
    pub hold: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub group: u32,
    pub off_by: u32,
    pub off_fast: bool,
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub regions: Vec<Region>,
    /// Problems worth showing to a person; none are fatal.
    pub warnings: Vec<String>,
}

type Opcodes = HashMap<String, String>;

enum Token {
    Header(String),
    Opcode(String, String),
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'/') {
            for d in chars.by_ref() {
                if d == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut prev = ' ';
            for d in chars.by_ref() {
                if d == '\n' {
                    out.push('\n');
                }
                if prev == '*' && d == '/' {
                    break;
                }
                prev = d;
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// Splits one line into headers and `name=value` pairs. A value runs to the next opcode, the next
/// header or the end of the line, so sample paths may contain spaces.
fn tokenize_line(line: &str, out: &mut Vec<Token>) {
    let bytes: Vec<(usize, char)> = line.char_indices().collect();
    // Positions (byte offsets) where an opcode name starts, with where its `=` ends.
    let mut starts: Vec<(usize, usize, usize)> = Vec::new(); // (name_start, name_end, value_start)
    let mut i = 0;
    while let Some(&(pos, c)) = bytes.get(i) {
        let at_boundary = i == 0 || bytes.get(i - 1).is_some_and(|(_, p)| p.is_whitespace() || *p == '>');
        if at_boundary && is_name_char(c) {
            let mut j = i;
            while bytes.get(j).is_some_and(|(_, d)| is_name_char(*d)) {
                j += 1;
            }
            if let Some(&(eq, '=')) = bytes.get(j) {
                starts.push((pos, eq, eq + 1));
                i = j + 1;
                continue;
            }
            i = j;
            continue;
        }
        i += 1;
    }
    // Headers: `<name>` outside values; scan the line for them in order with the opcodes.
    let mut events: Vec<(usize, Token)> = Vec::new();
    let mut k = 0;
    while let Some(&(pos, c)) = bytes.get(k) {
        if c == '<'
            && let Some(rel) = line.get(pos..).and_then(|s| s.find('>'))
        {
            let name = line.get(pos + 1..pos + rel).unwrap_or("").trim().to_ascii_lowercase();
            events.push((pos, Token::Header(name)));
        }
        k += 1;
    }
    for (n, &(name_start, name_end, value_start)) in starts.iter().enumerate() {
        let next_opcode = starts.get(n + 1).map_or(line.len(), |s| s.0);
        let next_header = events.iter().map(|(p, _)| *p).find(|p| *p > name_start).unwrap_or(line.len());
        let value_end = next_opcode.min(next_header).max(value_start);
        let name = line.get(name_start..name_end).unwrap_or("").to_ascii_lowercase();
        let value = line.get(value_start..value_end).unwrap_or("").trim().to_string();
        events.push((name_start, Token::Opcode(name, value)));
    }
    events.sort_by_key(|(p, _)| *p);
    // A header's `>` may be adjacent to an opcode; drop opcodes that start inside a header tag.
    out.extend(events.into_iter().map(|(_, t)| t));
}

/// Expands `#include` and `#define`, returning clean text.
struct Preprocessor<'a> {
    resolve: &'a mut dyn FnMut(&str) -> Option<String>,
    defines: HashMap<String, String>,
    includes: usize,
    warnings: Vec<String>,
}

impl Preprocessor<'_> {
    fn expand(&mut self, text: &str, depth: usize, out: &mut String) {
        if depth > MAX_DEPTH || out.len() > MAX_TEXT {
            return;
        }
        for raw in strip_comments(text).lines() {
            if !self.line(raw.trim(), depth, out) {
                return;
            }
        }
    }

    /// Handles one line, in which directives may follow other text or each other
    /// (`<group> #include "a.sfz"#define $x 1`). Returns false to stop (too many includes).
    fn line(&mut self, mut line: &str, depth: usize, out: &mut String) -> bool {
        loop {
            let next = ["#include", "#define"].iter().filter_map(|d| line.find(d)).min();
            let Some(at) = next else {
                if !line.trim().is_empty() {
                    out.push_str(&self.substitute(line.trim()));
                    out.push('\n');
                }
                return true;
            };
            let (before, rest) = line.split_at(at);
            if !before.trim().is_empty() {
                out.push_str(&self.substitute(before.trim()));
                out.push('\n');
            }
            if let Some(args) = rest.strip_prefix("#include") {
                let (name, after) = match args.split_once('"').and_then(|(_, q)| q.split_once('"')) {
                    Some((name, after)) => (name, after),
                    None => (args.trim(), ""),
                };
                let name = name.trim().replace('\\', "/");
                self.includes += 1;
                if self.includes > MAX_INCLUDES {
                    self.warnings.push("too many #include directives".into());
                    return false;
                }
                match (self.resolve)(&name) {
                    Some(inc) if inc.len() <= MAX_TEXT => self.expand(&inc, depth + 1, out),
                    Some(_) => self.warnings.push(format!("include too large: {name}")),
                    None => self.warnings.push(format!("include not found: {name}")),
                }
                line = after;
            } else {
                let args = rest.strip_prefix("#define").unwrap_or("");
                let end = args.find("#include").into_iter().chain(args.find("#define")).min().unwrap_or(args.len());
                let (def, after) = args.split_at(end);
                let mut parts = def.trim().splitn(2, char::is_whitespace);
                if let (Some(name), Some(value)) = (parts.next(), parts.next())
                    && name.starts_with('$')
                {
                    self.defines.insert(name.to_string(), value.trim().to_string());
                }
                line = after;
            }
        }
    }

    fn substitute(&self, line: &str) -> String {
        if !line.contains('$') || self.defines.is_empty() {
            return line.to_string();
        }
        // Longest names first so `$KEYS` wins over `$KEY`.
        let mut names: Vec<&String> = self.defines.keys().collect();
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        let mut s = line.to_string();
        for n in names {
            if let Some(v) = self.defines.get(n) {
                s = s.replace(n.as_str(), v);
            }
        }
        s
    }
}

/// Note names (`c4`, `f#3`, `eb2`) or numbers. C4 = 60 (the SFZ convention).
pub fn parse_key(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Ok(n) = s.parse::<i32>() {
        return u8::try_from(n.clamp(0, 127)).ok();
    }
    let mut chars = s.chars();
    let base = match chars.next()?.to_ascii_lowercase() {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (acc, oct) = if let Some(r) = rest.strip_prefix('#') {
        (1, r)
    } else if let Some(r) = rest.strip_prefix('b') {
        (-1, r)
    } else {
        (0, rest.as_str())
    };
    let octave: i32 = oct.parse().ok()?;
    u8::try_from((12 * (octave + 1) + base + acc).clamp(0, 127)).ok()
}

fn num(map: &Opcodes, key: &str) -> Option<f32> {
    map.get(key).and_then(|v| v.parse::<f32>().ok()).filter(|x| x.is_finite())
}

fn key_of(map: &Opcodes, key: &str) -> Option<u8> {
    map.get(key).and_then(|v| parse_key(v))
}

/// Base value plus every `<name>_onccN` / `<name>_ccN` contribution at the CC defaults.
fn modulated(map: &Opcodes, name: &str, default: f32, cc: &[f32; 128]) -> f32 {
    let mut v = num(map, name).unwrap_or(default);
    for (k, val) in map {
        let Some(rest) = k.strip_prefix(name).and_then(|r| r.strip_prefix('_')) else { continue };
        let digits = rest.strip_prefix("oncc").or_else(|| rest.strip_prefix("cc"));
        let Some(n) = digits.and_then(|d| d.parse::<usize>().ok()) else { continue };
        if let (Some(amount), Some(c)) = (val.parse::<f32>().ok().filter(|x| x.is_finite()), cc.get(n)) {
            v += amount * c / 127.0;
        }
    }
    v
}

fn cc_ok(map: &Opcodes, cc: &[f32; 128]) -> bool {
    for (k, v) in map {
        let Some(n) = k.strip_prefix("locc").and_then(|d| d.parse::<usize>().ok()) else { continue };
        if let (Some(lo), Some(c)) = (v.parse::<f32>().ok(), cc.get(n))
            && *c < lo
        {
            return false;
        }
    }
    for (k, v) in map {
        let Some(n) = k.strip_prefix("hicc").and_then(|d| d.parse::<usize>().ok()) else { continue };
        if let (Some(hi), Some(c)) = (v.parse::<f32>().ok(), cc.get(n))
            && *c > hi
        {
            return false;
        }
    }
    true
}

fn build_region(map: &Opcodes, cc: &[f32; 128], default_path: &str, key_offset: i32) -> Option<Region> {
    let sample = map.get("sample")?.trim();
    if sample.is_empty() || sample.starts_with('*') {
        return None; // generators are not supported
    }
    if matches!(map.get("trigger").map(String::as_str), Some("release" | "release_key")) {
        return None;
    }
    if !cc_ok(map, cc) {
        return None;
    }
    let shift = |k: u8| u8::try_from((i32::from(k) + key_offset).clamp(0, 127)).unwrap_or(k);
    let (mut lokey, mut hikey) = (key_of(map, "lokey").unwrap_or(0), key_of(map, "hikey").unwrap_or(127));
    let mut keycenter = key_of(map, "pitch_keycenter").unwrap_or(60);
    if let Some(k) = key_of(map, "key") {
        (lokey, hikey) = (k, k);
        if !map.contains_key("pitch_keycenter") {
            keycenter = k;
        }
    }
    let sample = format!("{}{}", default_path, sample.replace('\\', "/"));
    let loop_mode = match map.get("loop_mode").or_else(|| map.get("loopmode")).map(String::as_str) {
        Some("one_shot") => LoopMode::OneShot,
        Some("loop_continuous") => LoopMode::LoopContinuous,
        Some("loop_sustain") => LoopMode::LoopSustain,
        _ => LoopMode::NoLoop,
    };
    let to_u64 = |x: f32| if x >= 0.0 { x as u64 } else { 0 };
    let amplitude = modulated(map, "amplitude", 100.0, cc).clamp(0.0, 400.0) / 100.0;
    let volume_db = modulated(map, "volume", 0.0, cc) + if amplitude > 0.0 { 20.0 * amplitude.log10() } else { -120.0 };
    let tune = modulated(map, "tune", 0.0, cc) + 100.0 * modulated(map, "transpose", 0.0, cc);
    Some(Region {
        sample,
        lokey: shift(lokey),
        hikey: shift(hikey),
        keycenter: shift(keycenter),
        lovel: key_of(map, "lovel").unwrap_or(0),
        hivel: key_of(map, "hivel").unwrap_or(127),
        lorand: num(map, "lorand").unwrap_or(0.0),
        hirand: num(map, "hirand").unwrap_or(1.0),
        seq_length: num(map, "seq_length").map_or(1, |x| x.max(1.0) as u32),
        seq_position: num(map, "seq_position").map_or(1, |x| x.max(1.0) as u32),
        sw_last: key_of(map, "sw_last"),
        volume_db: volume_db.clamp(-144.0, 48.0),
        pan: (modulated(map, "pan", 0.0, cc) / 100.0).clamp(-1.0, 1.0),
        tune_cents: tune.clamp(-9600.0, 9600.0),
        keytrack: num(map, "pitch_keytrack").unwrap_or(100.0),
        veltrack: (num(map, "amp_veltrack").unwrap_or(100.0) / 100.0).clamp(-1.0, 1.0),
        offset: to_u64(modulated(map, "offset", 0.0, cc)),
        end: num(map, "end").filter(|e| *e >= 0.0).map(|e| e as u64),
        loop_mode,
        loop_start: num(map, "loop_start").or_else(|| num(map, "loopstart")).map(to_u64),
        loop_end: num(map, "loop_end").or_else(|| num(map, "loopend")).map(to_u64),
        delay: modulated(map, "delay", 0.0, cc).clamp(0.0, 10.0),
        attack: modulated(map, "ampeg_attack", 0.0, cc).clamp(0.0, 20.0),
        hold: modulated(map, "ampeg_hold", 0.0, cc).clamp(0.0, 20.0),
        decay: modulated(map, "ampeg_decay", 0.0, cc).clamp(0.0, 20.0),
        sustain: (modulated(map, "ampeg_sustain", 100.0, cc) / 100.0).clamp(0.0, 1.0),
        release: modulated(map, "ampeg_release", 0.001, cc).clamp(0.0, 20.0),
        group: num(map, "group").map_or(0, |x| x.max(0.0) as u32),
        off_by: num(map, "off_by").map_or(0, |x| x.max(0.0) as u32),
        off_fast: map.get("off_mode").is_some_and(|m| m == "fast"),
    })
}

/// Parses SFZ `text`. `resolve` returns the text of an included file by its path as written.
/// Keyswitched regions are kept for the file's own `sw_default` (see [`parse_switched`]).
pub fn parse(text: &str, resolve: &mut dyn FnMut(&str) -> Option<String>) -> Parsed {
    parse_switched(text, resolve, None)
}

/// [`parse`] with `switch` as the active keyswitch: regions without a switch are kept, and those
/// whose `sw_last` is `switch` (None: the file's own `sw_default`). One program can so give each
/// articulation it records by keyswitch.
pub fn parse_switched(text: &str, resolve: &mut dyn FnMut(&str) -> Option<String>, switch: Option<u8>) -> Parsed {
    let mut pre = Preprocessor { resolve, defines: HashMap::new(), includes: 0, warnings: Vec::new() };
    let mut clean = String::new();
    pre.expand(text, 0, &mut clean);
    let mut parsed = Parsed { regions: Vec::new(), warnings: std::mem::take(&mut pre.warnings) };

    let mut tokens = Vec::new();
    for line in clean.lines() {
        tokenize_line(line, &mut tokens);
    }

    let mut cc = [0.0f32; 128];
    let mut default_path = String::new();
    let mut key_offset = 0i32;
    let mut sw_default: Option<u8> = None;
    // Inheritance levels: global, master, group, region.
    let mut levels: [Opcodes; 4] = Default::default();
    let mut current: Option<usize> = None; // index into levels, or None while in <control> etc.
    let mut in_control = false;
    let mut pending: Vec<Opcodes> = Vec::new(); // region opcodes, resolved after CC defaults are known

    let flush = |levels: &[Opcodes; 4], pending: &mut Vec<Opcodes>| {
        let mut merged = Opcodes::new();
        for l in levels.iter().take(3) {
            merged.extend(l.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        if let Some(r) = levels.get(3) {
            merged.extend(r.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        pending.push(merged);
    };
    let mut region_open = false;

    for tok in tokens {
        match tok {
            Token::Header(name) => {
                if region_open {
                    flush(&levels, &mut pending);
                    region_open = false;
                }
                in_control = false;
                current = None;
                match name.as_str() {
                    "control" => in_control = true,
                    "global" => {
                        levels = Default::default();
                        current = Some(0);
                    }
                    "master" => {
                        levels.iter_mut().skip(1).for_each(Opcodes::clear);
                        current = Some(1);
                    }
                    "group" => {
                        levels.iter_mut().skip(2).for_each(Opcodes::clear);
                        current = Some(2);
                    }
                    "region" => {
                        if let Some(r) = levels.get_mut(3) {
                            r.clear();
                        }
                        current = Some(3);
                        region_open = true;
                    }
                    _ => {}
                }
            }
            Token::Opcode(name, value) => {
                if in_control {
                    if let Some(n) = name.strip_prefix("set_cc").or_else(|| name.strip_prefix("set_hdcc")).and_then(|d| d.parse::<usize>().ok()) {
                        let scale = if name.starts_with("set_hdcc") { 127.0 } else { 1.0 };
                        if let (Some(slot), Ok(v)) = (cc.get_mut(n), value.parse::<f32>()) {
                            *slot = (v * scale).clamp(0.0, 127.0);
                        }
                    } else if name == "default_path" {
                        default_path = value.replace('\\', "/");
                        if !default_path.is_empty() && !default_path.ends_with('/') {
                            default_path.push('/');
                        }
                    } else if name == "octave_offset" {
                        key_offset += 12 * value.parse::<i32>().unwrap_or(0).clamp(-10, 10);
                    } else if name == "note_offset" {
                        key_offset += value.parse::<i32>().unwrap_or(0).clamp(-127, 127);
                    }
                } else if let Some(l) = current.and_then(|i| levels.get_mut(i)) {
                    if name == "sw_default" {
                        sw_default = parse_key(&value);
                    }
                    l.insert(name, value);
                }
            }
        }
    }
    if region_open {
        flush(&levels, &mut pending);
    }

    for map in pending {
        if parsed.regions.len() >= MAX_REGIONS {
            parsed.warnings.push("region limit reached; the rest are ignored".into());
            break;
        }
        if let Some(r) = build_region(&map, &cc, &default_path, key_offset) {
            // Keyswitched regions play only for the active switch.
            if r.sw_last.is_none() || r.sw_last == switch.or(sw_default) {
                parsed.regions.push(r);
            }
        }
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Parsed {
        parse(text, &mut |_| None)
    }

    #[test]
    fn keys_and_names() {
        assert_eq!(parse_key("c4"), Some(60));
        assert_eq!(parse_key("a#3"), Some(58));
        assert_eq!(parse_key("eb2"), Some(39));
        assert_eq!(parse_key("-5"), Some(0));
        assert_eq!(parse_key("999"), Some(127));
        assert_eq!(parse_key("zz"), None);
    }

    #[test]
    fn inheritance_and_spaces_in_paths() {
        let r = p(
            "<global> ampeg_release=0.3 lovel=10\n<group> hivel=90\n<region> sample=My Folder/a b.wav key=40\n<region> sample=c.flac lokey=c3 hikey=e3 pitch_keycenter=d3 hivel=127",
        );
        assert_eq!(r.regions.len(), 2);
        let (a, b) = (&r.regions[0], &r.regions[1]);
        assert_eq!(a.sample, "My Folder/a b.wav");
        assert_eq!((a.lokey, a.hikey, a.keycenter, a.lovel, a.hivel), (40, 40, 40, 10, 90));
        assert!((a.release - 0.3).abs() < 1e-6);
        assert_eq!((b.lokey, b.hikey, b.keycenter, b.hivel), (48, 52, 50, 127));
    }

    #[test]
    fn defines_includes_comments_default_path() {
        let mut inc = |n: &str| (n == "sub/x.sfz").then(|| "<region> sample=$S key=$K // trailing\n".to_string());
        let r = parse("#define $S one.wav\n#define $K 33\n<control> default_path=Samples\\\n/* block\ncomment */\n#include \"sub/x.sfz\"", &mut inc);
        assert_eq!(r.regions.len(), 1);
        assert_eq!(r.regions[0].sample, "Samples/one.wav");
        assert_eq!(r.regions[0].lokey, 33);
        assert!(parse("#include \"nope.sfz\"", &mut |_| None).warnings[0].contains("not found"));
    }

    #[test]
    fn cc_defaults_modulate_and_gate() {
        // pan -100 + 200 * (64/127) is centred, like Karoryfer's bass maps.
        let r = p(
            "<control> set_cc10=64 set_cc118=0\n<global> pan=-100 pan_oncc10=200\n<master> hicc118=63\n<region> sample=a.wav key=1\n<master> locc118=64\n<region> sample=b.wav key=2",
        );
        assert_eq!(r.regions.len(), 1);
        assert!(r.regions[0].pan.abs() < 0.02, "pan {}", r.regions[0].pan);
    }

    #[test]
    fn keyswitch_default_and_release_triggers_and_generators() {
        let r = p(
            "<global> sw_default=c2 sw_lokey=c2 sw_hikey=d2\n<region> sample=a.wav key=60 sw_last=c2\n<region> sample=b.wav key=60 sw_last=d2\n<region> sample=r.wav key=60 trigger=release\n<region> sample=*sine key=61",
        );
        assert_eq!(r.regions.len(), 1);
        assert_eq!(r.regions[0].sample, "a.wav");
        // Another switch picks the other articulation; regions without a switch stay.
        let text = "<global> sw_default=c2\n<region> sample=a.wav key=60 sw_last=c2\n<region> sample=b.wav key=60 sw_last=d2\n<region> sample=n.wav key=40";
        let names = |switch| parse_switched(text, &mut |_| None, switch).regions.into_iter().map(|r| r.sample).collect::<Vec<_>>();
        assert_eq!(names(parse_key("d2")), ["b.wav", "n.wav"]);
        assert_eq!(names(None), ["a.wav", "n.wav"]);
        assert_eq!(names(Some(0)), ["n.wav"], "a switch nothing uses keeps only the unswitched regions");
    }

    #[test]
    fn hostile_text_never_panics() {
        for t in [
            "",
            "<",
            "<>",
            "=",
            "====",
            "<region>sample=",
            "<region> sample=é key=\u{0}",
            "#include",
            "#define",
            "#define $ ",
            "<control>set_cc999=5 set_cc1=abc",
            "<region> lokey=\u{1F3B8} sample=x",
            "/*",
            "//",
            "<region> sample=a.wav pan=NaN volume=inf tune=1e99 offset=-5 end=-1",
        ] {
            let _ = p(t);
        }
        let deep = "#include \"a\"";
        let _ = parse(deep, &mut |_| Some("#include \"a\"".to_string()));
        let inline = parse("<group> #include \"g.sfz\"#define $k 7\n<region> sample=$k.wav key=$k", &mut |_| Some("<group> ampeg_release=0.4".to_string()));
        assert_eq!((inline.regions.len(), inline.regions[0].lokey), (1, 7));
        assert!((inline.regions[0].release - 0.4).abs() < 1e-6);
        let two = parse("#include \"a.sfz\" #include \"b.sfz\"", &mut |n| Some(format!("<region> sample={n} key=1")));
        assert_eq!(two.regions.len(), 2);
        let r = p("<region> sample=a.wav key=60 volume=1e30 tune=-1e30 ampeg_release=NaN");
        assert!(r.regions.iter().all(|x| x.volume_db.is_finite() && x.tune_cents.is_finite()));
    }
}
