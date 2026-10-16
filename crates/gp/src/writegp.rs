//! Guitar Pro 7/8 (`.gp`) export: the score as `score.gpif` in a ZIP, written back the way
//! [`crate::gpif`] reads it.
//!
//! Every bar, voice, beat and note gets its own id (nothing is shared, which is valid and keeps the
//! writer simple); rhythms are shared. Strings count from the lowest, as in Guitar Pro. What the
//! file format can hold but the importer or model does not carry yet is left out and counted in
//! [`Export::warnings`].

use crate::write5::Export;
use crate::{GpError, MAX_MEASURES};
use guitario_score::*;
use guitario_theory::{Duration, NoteValue, TICKS_PER_QUARTER, drums};
use std::collections::BTreeMap;
use std::io::Write;

/// Brush and arpeggio lengths are stored at 960 ticks per quarter.
const GP_BRUSH_SCALE: u32 = TICKS_PER_QUARTER / 960;

/// Track colours, cycled.
const COLOURS: [&str; 6] = ["255 0 0", "0 128 255", "0 160 0", "255 160 0", "160 0 255", "0 160 160"];

/// Writes `score` as a Guitar Pro 7/8 file.
pub fn export_gp(score: &Score) -> Result<Export, GpError> {
    let mut w = Gpif::default();
    let xml = w.build(score);
    let mut warnings: Vec<String> =
        w.dropped.iter().map(|(what, n)| format!("{what} are not written to Guitar Pro 7/8 files and were left out ({n})")).collect();
    warnings.sort();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let io = |e: &dyn std::fmt::Display| GpError::Io(e.to_string());
    zip.start_file("VERSION", opts).map_err(|e| io(&e))?;
    zip.write_all(b"7.0").map_err(|e| io(&e))?;
    zip.add_directory("Content/", opts).map_err(|e| io(&e))?;
    zip.start_file("Content/score.gpif", opts).map_err(|e| io(&e))?;
    zip.write_all(xml.as_bytes()).map_err(|e| io(&e))?;
    let bytes = zip.finish().map_err(|e| io(&e))?.into_inner();
    Ok(Export { bytes, warnings })
}

/// Text safe inside CDATA: no control characters, and `]]>` split so it cannot end the section.
fn cdata(s: &str) -> String {
    let clean: String = s.chars().filter(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r')).collect();
    format!("<![CDATA[{}]]>", clean.replace("]]>", "]]]]><![CDATA[>"))
}

fn dynamic_name(d: Dynamic) -> &'static str {
    match d {
        Dynamic::Ppp => "PPP",
        Dynamic::Pp => "PP",
        Dynamic::P => "P",
        Dynamic::Mp => "MP",
        Dynamic::Mf => "MF",
        Dynamic::F => "F",
        Dynamic::Ff => "FF",
        Dynamic::Fff => "FFF",
    }
}

fn note_value_name(v: NoteValue) -> &'static str {
    match v {
        NoteValue::Whole => "Whole",
        NoteValue::Half => "Half",
        NoteValue::Quarter => "Quarter",
        NoteValue::Eighth => "Eighth",
        NoteValue::Sixteenth => "16th",
        NoteValue::ThirtySecond => "32nd",
        NoteValue::SixtyFourth => "64th",
    }
}

/// The GPIF name of a jump: `DaCapoAlFine`, `DaSegno`, `DaCoda` …
fn jump_name(j: Jump) -> String {
    let end = |e: JumpEnd| match e {
        JumpEnd::End => "",
        JumpEnd::Fine => "AlFine",
        JumpEnd::Coda => "AlCoda",
        JumpEnd::DoubleCoda => "AlDoubleCoda",
    };
    match j {
        Jump::DaCapo(e) => format!("DaCapo{}", end(e)),
        Jump::DalSegno(e) => format!("DaSegno{}", end(e)),
        Jump::DalSegnoSegno(e) => format!("DaSegnoSegno{}", end(e)),
        Jump::ToCoda => "DaCoda".into(),
        Jump::ToDoubleCoda => "DaDoubleCoda".into(),
    }
}

fn target_name(t: DirectionTarget) -> &'static str {
    match t {
        DirectionTarget::Segno => "Segno",
        DirectionTarget::SegnoSegno => "SegnoSegno",
        DirectionTarget::Coda => "Coda",
        DirectionTarget::DoubleCoda => "DoubleCoda",
        DirectionTarget::Fine => "Fine",
    }
}

/// A note waiting for its final XML, because a tie or hammer-on later in the file can still mark it.
#[derive(Default)]
struct NoteOut {
    /// Elements before `<Properties>`.
    head: String,
    props: String,
    tie_origin: bool,
    tie_destination: bool,
}

impl NoteOut {
    fn xml(&self, id: usize) -> String {
        let tie = if self.tie_origin || self.tie_destination {
            format!("<Tie origin=\"{}\" destination=\"{}\"/>", self.tie_origin, self.tie_destination)
        } else {
            String::new()
        };
        format!("<Note id=\"{id}\">{tie}{}<Properties>{}</Properties></Note>\n", self.head, self.props)
    }
}

/// The last note written on a string of a voice of a track, for ties and hammer-on targets.
type StringKey = (usize, usize, u8);

#[derive(Default)]
struct Gpif {
    notes: Vec<NoteOut>,
    beats: String,
    voices: String,
    bars: String,
    rhythms: Vec<Duration>,
    next_beat: usize,
    next_voice: usize,
    last_on_string: BTreeMap<StringKey, (usize, bool)>,
    dropped: BTreeMap<&'static str, usize>,
}

impl Gpif {
    fn drop(&mut self, what: &'static str) {
        *self.dropped.entry(what).or_default() += 1;
    }

    fn rhythm(&mut self, d: Duration) -> usize {
        match self.rhythms.iter().position(|r| *r == d) {
            Some(i) => i,
            None => {
                self.rhythms.push(d);
                self.rhythms.len() - 1
            }
        }
    }

    fn build(&mut self, score: &Score) -> String {
        let headers = score.headers.get(..score.headers.len().min(MAX_MEASURES)).unwrap_or_default();
        let m = &score.meta;
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<GPIF>\n<GPVersion>7.5.0</GPVersion>\n<GPRevision>12025</GPRevision>\n");
        xml.push_str("<Encoding><EncodingDescription>GP7</EncodingDescription></Encoding>\n");
        xml.push_str(&format!(
            "<Score><Title>{}</Title><SubTitle>{}</SubTitle><Artist>{}</Artist><Album>{}</Album><Words>{}</Words><Music>{}</Music><WordsAndMusic>{}</WordsAndMusic><Copyright>{}</Copyright><Tabber>{}</Tabber><Instructions>{}</Instructions><Notices>{}</Notices>{}</Score>\n",
            cdata(&m.title),
            cdata(""),
            cdata(&m.artist),
            cdata(&m.album),
            cdata(""),
            cdata(""),
            cdata(""),
            cdata(""),
            cdata(&m.author),
            cdata(""),
            cdata(&m.notes),
            page_setup(&score.page),
        ));
        // Tempo changes live on the master track.
        let track_ids: Vec<String> = (0..score.tracks.len()).map(|i| i.to_string()).collect();
        xml.push_str(&format!("<MasterTrack><Tracks>{}</Tracks><Automations>", track_ids.join(" ")));
        let tempo = |bar: usize, bpm: u16| {
            format!(
                "<Automation><Type>Tempo</Type><Linear>false</Linear><Bar>{bar}</Bar><Position>0</Position><Visible>true</Visible><Value>{bpm} 2</Value></Automation>"
            )
        };
        xml.push_str(&tempo(0, headers.first().and_then(|h| h.tempo).unwrap_or(score.tempo)));
        for (i, h) in headers.iter().enumerate().skip(1) {
            if let Some(bpm) = h.tempo {
                xml.push_str(&tempo(i, bpm));
            }
        }
        xml.push_str("</Automations></MasterTrack>\n<Tracks>\n");
        for (ti, track) in score.tracks.iter().enumerate() {
            self.track_xml(&mut xml, ti, track);
        }
        xml.push_str("</Tracks>\n<MasterBars>\n");
        let tracks = score.tracks.len();
        for (mi, h) in headers.iter().enumerate() {
            xml.push_str(&master_bar(h, (0..tracks).map(|ti| mi * tracks + ti)));
        }
        xml.push_str("</MasterBars>\n");
        for (mi, _) in headers.iter().enumerate() {
            for (ti, track) in score.tracks.iter().enumerate() {
                self.bar(mi * tracks + ti, ti, track, mi);
            }
        }
        xml.push_str(&format!("<Bars>\n{}</Bars>\n<Voices>\n{}</Voices>\n<Beats>\n{}</Beats>\n<Notes>\n", self.bars, self.voices, self.beats));
        for (id, n) in self.notes.iter().enumerate() {
            xml.push_str(&n.xml(id));
        }
        xml.push_str("</Notes>\n<Rhythms>\n");
        for (id, d) in self.rhythms.iter().enumerate() {
            let dots = if d.dots > 0 { format!("<AugmentationDot count=\"{}\"/>", d.dots.min(2)) } else { String::new() };
            let tuplet =
                if d.tuplet.0 > 1 && d.tuplet != (1, 1) { format!("<PrimaryTuplet num=\"{}\" den=\"{}\"/>", d.tuplet.0, d.tuplet.1) } else { String::new() };
            xml.push_str(&format!("<Rhythm id=\"{id}\"><NoteValue>{}</NoteValue>{dots}{tuplet}</Rhythm>\n", note_value_name(d.value)));
        }
        xml.push_str("</Rhythms>\n</GPIF>\n");
        xml
    }

    fn track_xml(&mut self, xml: &mut String, ti: usize, t: &Track) {
        let state = if t.muted {
            "Mute"
        } else if t.solo {
            "Solo"
        } else {
            "Default"
        };
        xml.push_str(&format!(
            "<Track id=\"{ti}\"><Name>{}</Name><ShortName>{}</ShortName><Color>{}</Color><PlaybackState>{state}</PlaybackState>\n",
            cdata(&t.name),
            cdata(&t.name.chars().take(8).collect::<String>()),
            COLOURS.get(ti % COLOURS.len()).copied().unwrap_or("255 0 0")
        ));
        xml.push_str(&format!(
            "<Sounds><Sound><Name>{0}</Name><Label>{0}</Label><Path>{1}</Path><Role>Factory</Role><MIDI><LSB>0</LSB><MSB>0</MSB><Program>{2}</Program></MIDI></Sound></Sounds>\n",
            cdata(&t.name),
            cdata(if t.is_drums() { "Drums/Drums" } else { "Stringed/Guitars/Guitar" }),
            t.program.min(127)
        ));
        if t.is_drums() {
            xml.push_str("<InstrumentSet><Name>Drums</Name><Type>drumKit</Type><LineCount>5</LineCount><Elements>\n");
            for p in &drums::KIT {
                let kind = match p.lane {
                    7 => "kickDrum",
                    4 => "snare",
                    2 | 6 => "hiHat",
                    1 => "rideCymbal",
                    0 => "crashCymbal",
                    _ => "tomTom",
                };
                xml.push_str(&format!(
                    "<Element><Name>{0}</Name><Type>{kind}</Type><Articulations><Articulation><Name>{0}</Name><StaffLine>{1}</StaffLine><OutputMidiNumber>{2}</OutputMidiNumber></Articulation></Articulations></Element>\n",
                    cdata(p.name),
                    8 - i32::from(p.staff_pos),
                    p.midi
                ));
            }
            xml.push_str("</Elements></InstrumentSet>\n<Staves><Staff><Properties><Name><![CDATA[Drums]]></Name></Properties></Staff></Staves>\n");
        } else {
            // Guitar Pro lists the tuning from the lowest string.
            let lowest_first: Vec<String> = t.tuning.strings.iter().rev().map(u8::to_string).collect();
            xml.push_str(&format!(
                "<Staves><Staff><Properties><Property name=\"CapoFret\"><Fret>{}</Fret></Property><Property name=\"Tuning\"><Pitches>{}</Pitches><Instrument>Guitar</Instrument><Label>{}</Label><LabelVisible>true</LabelVisible></Property><Name>{}</Name></Properties></Staff></Staves>\n",
                t.capo,
                lowest_first.join(" "),
                cdata(&t.tuning.name),
                cdata(&t.tuning.name)
            ));
        }
        xml.push_str("</Track>\n");
        if t.volume != 100 || t.pan != 0 {
            self.drop("track volume and pan settings");
        }
    }

    fn bar(&mut self, id: usize, ti: usize, t: &Track, mi: usize) {
        let clef = if t.is_drums() {
            "Neutral"
        } else if t.tuning.strings.iter().copied().max().unwrap_or(64) < 52 {
            "F4"
        } else {
            "G2"
        };
        let mut ids = Vec::new();
        for vi in 0..MAX_VOICES {
            let voice = t.measures.get(mi).and_then(|m| m.voices.get(vi)).filter(|v| !v.beats.is_empty());
            let Some(voice) = voice else {
                ids.push("-1".to_string());
                continue;
            };
            let vid = self.next_voice;
            self.next_voice += 1;
            ids.push(vid.to_string());
            let mut beat_ids = Vec::new();
            for beat in &voice.beats {
                beat_ids.push(self.beat(ti, vi, t, beat).to_string());
            }
            self.voices.push_str(&format!("<Voice id=\"{vid}\"><Beats>{}</Beats></Voice>\n", beat_ids.join(" ")));
        }
        self.bars.push_str(&format!("<Bar id=\"{id}\"><Clef>{clef}</Clef><Voices>{}</Voices></Bar>\n", ids.join(" ")));
    }

    fn beat(&mut self, ti: usize, vi: usize, t: &Track, b: &Beat) -> usize {
        let id = self.next_beat;
        self.next_beat += 1;
        let rhythm = self.rhythm(b.duration);
        let mut note_ids = Vec::new();
        if !b.rest {
            for n in &b.notes {
                note_ids.push(self.note(ti, vi, t, n).to_string());
            }
        }
        let mut xml = format!("<Beat id=\"{id}\"><Dynamic>{}</Dynamic>", dynamic_name(b.dynamic));
        match b.fade {
            Some(Fade::In) => xml.push_str("<Fadding>FadeIn</Fadding>"),
            Some(Fade::Out) => xml.push_str("<Fadding>FadeOut</Fadding>"),
            Some(Fade::Swell) => xml.push_str("<Fadding>VolumeSwell</Fadding>"),
            None => {}
        }
        xml.push_str(&format!("<Rhythm ref=\"{rhythm}\"/>"));
        if !note_ids.is_empty() {
            xml.push_str(&format!("<Notes>{}</Notes>", note_ids.join(" ")));
        }
        if let Some(text) = &b.text {
            xml.push_str(&format!("<FreeText>{}</FreeText>", cdata(text)));
        }
        let ticks = |t: u32| (t / GP_BRUSH_SCALE).clamp(1, 7680);
        let mut props = String::new();
        let mut xprops = String::new();
        match b.brush {
            Some(Brush { kind: BrushKind::Brush, down, ticks: len }) => {
                props.push_str(&format!("<Property name=\"Brush\"><Direction>{}</Direction></Property>", if down { "Down" } else { "Up" }));
                xprops.push_str(&format!("<XProperty id=\"687935489\"><Int>{}</Int></XProperty>", ticks(len)));
            }
            Some(Brush { kind: BrushKind::Arpeggio, down, ticks: len }) => {
                xml.push_str(&format!("<Arpeggio>{}</Arpeggio>", if down { "Down" } else { "Up" }));
                xprops.push_str(&format!("<XProperty id=\"687931393\"><Int>{}</Int></XProperty>", ticks(len)));
            }
            Some(Brush { kind: BrushKind::Rasgueado, .. }) => props.push_str("<Property name=\"Rasgueado\"><Enable/></Property>"),
            None => {}
        }
        if let Some(s) = b.stroke {
            props
                .push_str(&format!("<Property name=\"PickStroke\"><Direction>{}</Direction></Property>", if s == StrokeDirection::Up { "Up" } else { "Down" }));
        }
        match b.technique {
            Some(BeatTechnique::Slap) => props.push_str("<Property name=\"Slapped\"><Enable/></Property>"),
            Some(BeatTechnique::Pop) => props.push_str("<Property name=\"Popped\"><Enable/></Property>"),
            None => {}
        }
        if !props.is_empty() {
            xml.push_str(&format!("<Properties>{props}</Properties>"));
        }
        if !xprops.is_empty() {
            xml.push_str(&format!("<XProperties>{xprops}</XProperties>"));
        }
        if b.chord.is_some() {
            self.drop("chord names and diagrams");
        }
        xml.push_str("</Beat>\n");
        self.beats.push_str(&xml);
        id
    }

    fn note(&mut self, ti: usize, vi: usize, t: &Track, n: &Note) -> usize {
        let id = self.notes.len();
        let mut out = NoteOut::default();
        let e = &n.effects;
        let enable = |name: &str| format!("<Property name=\"{name}\"><Enable/></Property>");
        if t.is_drums() {
            match drums::KIT.iter().position(|p| p.midi == n.fret) {
                Some(i) => out.head.push_str(&format!("<InstrumentArticulation>{i}</InstrumentArticulation>")),
                // A piece outside the kit keeps its General MIDI number.
                None => {
                    out.head.push_str("<InstrumentArticulation>0</InstrumentArticulation>");
                    out.props.push_str(&format!("<Property name=\"Midi\"><Number>{}</Number></Property>", n.fret));
                }
            }
        } else {
            let gp_string = t.string_count().saturating_sub(1).saturating_sub(usize::from(n.string));
            out.props.push_str(&format!(
                "<Property name=\"String\"><String>{gp_string}</String></Property><Property name=\"Fret\"><Fret>{}</Fret></Property>",
                n.fret
            ));
        }
        if n.kind == NoteKind::Dead {
            out.props.push_str(&enable("Muted"));
        }
        let accent = u8::from(e.staccato) | (u8::from(e.heavy_accent) << 2) | (u8::from(e.accent) << 3);
        if accent != 0 {
            out.head.push_str(&format!("<Accent>{accent}</Accent>"));
        }
        if e.ghost {
            out.head.push_str("<AntiAccent>Normal</AntiAccent>");
        }
        if e.let_ring {
            out.head.push_str("<LetRing/>");
        }
        if e.wide_vibrato {
            out.head.push_str("<Vibrato>Wide</Vibrato>");
        } else if e.vibrato {
            out.head.push_str("<Vibrato>Slight</Vibrato>");
        }
        if e.palm_mute {
            out.props.push_str(&enable("PalmMuted"));
        }
        if e.hammer {
            out.props.push_str(&enable("HopoOrigin"));
        }
        if e.tapped {
            out.props.push_str(&enable("Tapped"));
        }
        if e.left_hand_tapped {
            out.props.push_str(&enable("LeftHandTapped"));
        }
        if let Some(slide) = e.slide {
            let flag = match slide {
                Slide::Shift => 1,
                Slide::Legato => 2,
                Slide::OutDown => 4,
                Slide::OutUp => 8,
                Slide::IntoFromBelow => 16,
                Slide::IntoFromAbove => 32,
            };
            out.props.push_str(&format!("<Property name=\"Slide\"><Flags>{flag}</Flags></Property>"));
        }
        if let Some(h) = e.harmonic {
            let name = match h {
                Harmonic::Natural => "Natural",
                Harmonic::Artificial => "Artificial",
                Harmonic::Pinch => "Pinch",
                Harmonic::Tapped => "Tap",
                Harmonic::Semi => "Semi",
            };
            out.props.push_str(&format!("{}<Property name=\"HarmonicType\"><HType>{name}</HType></Property>", enable("Harmonic")));
        }
        if let Some(q) = e.bend {
            // Curve values are hundredths of a whole tone (a quarter tone is 25): up to the height
            // by the middle of the note and held.
            let top = u32::from(q) * 25;
            out.props.push_str(&enable("Bended"));
            for (name, value) in [("BendOriginValue", 0), ("BendMiddleValue", top / 2), ("BendDestinationValue", top)] {
                out.props.push_str(&format!("<Property name=\"{name}\"><Float>{value}</Float></Property>"));
            }
            for (name, value) in [("BendOriginOffset", 0), ("BendMiddleOffset1", 12), ("BendMiddleOffset2", 12), ("BendDestinationOffset", 60)] {
                out.props.push_str(&format!("<Property name=\"{name}\"><Float>{value}</Float></Property>"));
            }
        }
        // A tie joins the last note on this string of this voice: mark both ends.
        let key = (ti, vi, n.string);
        let previous = self.last_on_string.get(&key).copied();
        if n.kind == NoteKind::Tie {
            out.tie_destination = true;
            if let Some((prev, _)) = previous
                && let Some(p) = self.notes.get_mut(prev)
            {
                p.tie_origin = true;
            }
        }
        if let Some((_, true)) = previous {
            out.props.push_str(&enable("HopoDestination"));
        }
        self.last_on_string.insert(key, (id, e.hammer));
        self.notes.push(out);
        id
    }
}

fn master_bar(h: &MeasureHeader, bars: impl Iterator<Item = usize>) -> String {
    let ids: Vec<String> = bars.map(|b| b.to_string()).collect();
    let ts = h.time_signature;
    let mut x = format!(
        "<MasterBar><Key><AccidentalCount>{}</AccidentalCount><Mode>Major</Mode></Key><Time>{}/{}</Time><Bars>{}</Bars>",
        h.key,
        ts.numerator,
        ts.denominator,
        ids.join(" ")
    );
    if h.repeat_open || h.repeat_close > 0 {
        // The count is the total number of plays.
        x.push_str(&format!(
            "<Repeat start=\"{}\" end=\"{}\" count=\"{}\"/>",
            h.repeat_open,
            h.repeat_close > 0,
            if h.repeat_close > 0 { u16::from(h.repeat_close) + 1 } else { 0 }
        ));
    }
    if let Some(marker) = &h.marker {
        x.push_str(&format!("<Section><Letter>{}</Letter><Text>{}</Text></Section>", cdata(""), cdata(marker)));
    }
    if h.endings != 0 {
        let numbers: Vec<String> = (1..=8u8).filter(|n| h.endings & (1 << (n - 1)) != 0).map(|n| n.to_string()).collect();
        x.push_str(&format!("<AlternateEndings>{}</AlternateEndings>", numbers.join(" ")));
    }
    if !h.targets.is_empty() || h.jump.is_some() {
        x.push_str("<Directions>");
        for t in &h.targets {
            x.push_str(&format!("<Target>{}</Target>", target_name(*t)));
        }
        if let Some(j) = h.jump {
            x.push_str(&format!("<Jump>{}</Jump>", jump_name(j)));
        }
        x.push_str("</Directions>");
    }
    let feel = match h.triplet_feel {
        TripletFeel::None => None,
        TripletFeel::Triplet8th => Some("Triplet8th"),
        TripletFeel::Triplet16th => Some("Triplet16th"),
        TripletFeel::Dotted8th => Some("Dotted8th"),
        TripletFeel::Dotted16th => Some("Dotted16th"),
        TripletFeel::Scottish8th => Some("Scottish8th"),
        TripletFeel::Scottish16th => Some("Scottish16th"),
    };
    if let Some(feel) = feel {
        x.push_str(&format!("<TripletFeel>{feel}</TripletFeel>"));
    }
    if h.free_time {
        x.push_str("<FreeTime/>");
    }
    if h.double_bar {
        x.push_str("<DoubleBar/>");
    }
    x.push_str("</MasterBar>\n");
    x
}

/// The score's `PageSetup`, left out for the default page so a file nobody changed stays as it was.
fn page_setup(page: &guitario_score::PageSetup) -> String {
    if *page == guitario_score::PageSetup::default() {
        return String::new();
    }
    let (width, height, [left, right, top, bottom]) = crate::page::to_gp(page);
    let orientation = if page.landscape { "Landscape" } else { "Portrait" };
    format!(
        "<PageSetup><Width>{width}</Width><Height>{height}</Height><Orientation>{orientation}</Orientation><TopMargin>{top}</TopMargin><LeftMargin>{left}</LeftMargin><RightMargin>{right}</RightMargin><BottomMargin>{bottom}</BottomMargin><Scale>1</Scale></PageSetup>"
    )
}
