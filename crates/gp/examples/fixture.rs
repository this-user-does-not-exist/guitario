//! Writes `demos/guitar-pro/import-test.gp`, a small hand-built Guitar Pro 7 file that exercises
//! the importer with real notes: `cargo run -p guitario-gp --example fixture`.
//!
//! The XML follows the public format notes: strings are numbered lowest first, notes, beats and
//! rhythms are shared by id (the gallop beats are reused), a repeat end carries the total number of
//! plays, and a tie is a pair of `Tie` flags.

use std::io::Write;
use std::process::ExitCode;
use zip::write::SimpleFileOptions;

pub const GPIF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<GPIF>
<GPVersion>7.5.0</GPVersion>
<GPRevision>12025</GPRevision>
<Encoding><EncodingDescription>GP7</EncodingDescription></Encoding>
<Score><Title><![CDATA[GP7 Import Test]]></Title><SubTitle><![CDATA[]]></SubTitle><Artist><![CDATA[Guitario]]></Artist><Album><![CDATA[Demos]]></Album><Words><![CDATA[]]></Words><Music><![CDATA[]]></Music><WordsAndMusic><![CDATA[]]></WordsAndMusic><Copyright><![CDATA[]]></Copyright><Tabber><![CDATA[Guitario demo]]></Tabber><Instructions><![CDATA[]]></Instructions><Notices><![CDATA[Hand-built fixture for the importer.]]></Notices><MultiVoice>0></MultiVoice></Score>
<MasterTrack><Tracks>0 1</Tracks><Automations><Automation><Type>Tempo</Type><Linear>false</Linear><Bar>0</Bar><Position>0</Position><Visible>true</Visible><Value>160 2</Value></Automation></Automations></MasterTrack>
<Tracks>
<Track id="0"><Name><![CDATA[Rhythm Guitar]]></Name><ShortName><![CDATA[r.guit.]]></ShortName><Color>255 0 0</Color><PlaybackState>Default</PlaybackState>
<Sounds><Sound><Name><![CDATA[Distortion]]></Name><Label><![CDATA[Distortion]]></Label><Path>Stringed/Electric Guitars/Distortion Guitar</Path><Role>Factory</Role><MIDI><LSB>0</LSB><MSB>0</MSB><Program>30</Program></MIDI></Sound></Sounds>
<Staves><Staff><Properties>
<Property name="CapoFret"><Fret>0</Fret></Property>
<Property name="Tuning"><Pitches>38 45 50 55 59 64</Pitches><Instrument>Guitar</Instrument><Label><![CDATA[Drop D]]></Label><LabelVisible>true</LabelVisible></Property>
<Name><![CDATA[Drop D]]></Name>
</Properties></Staff></Staves>
</Track>
<Track id="1"><Name><![CDATA[Drums]]></Name><ShortName><![CDATA[drms]]></ShortName><PlaybackState>Default</PlaybackState>
<InstrumentSet><Name>Drums</Name><Type>drumKit</Type><LineCount>5</LineCount><Elements>
<Element><Name>Snare</Name><Type>snare</Type><Articulations><Articulation><Name>Snare (hit)</Name><StaffLine>3</StaffLine><OutputMidiNumber>38</OutputMidiNumber></Articulation></Articulations></Element>
<Element><Name>Kick</Name><Type>kickDrum</Type><Articulations><Articulation><Name>Kick (hit)</Name><StaffLine>7</StaffLine><OutputMidiNumber>36</OutputMidiNumber></Articulation></Articulations></Element>
<Element><Name>Hi-Hat</Name><Type>hiHat</Type><Articulations><Articulation><Name>Hi-Hat (closed)</Name><StaffLine>-1</StaffLine><OutputMidiNumber>42</OutputMidiNumber></Articulation><Articulation><Name>Hi-Hat (open)</Name><StaffLine>-1</StaffLine><OutputMidiNumber>46</OutputMidiNumber></Articulation></Articulations></Element>
<Element><Name>Crash</Name><Type>crashCymbal</Type><Articulations><Articulation><Name>Crash (hit)</Name><StaffLine>-2</StaffLine><OutputMidiNumber>49</OutputMidiNumber></Articulation></Articulations></Element>
</Elements></InstrumentSet>
<Staves><Staff><Properties><Name><![CDATA[Drums]]></Name></Properties></Staff></Staves>
</Track>
</Tracks>
<MasterBars>
<MasterBar><Key><AccidentalCount>0</AccidentalCount><Mode>Major</Mode></Key><Time>4/4</Time><Bars>0 3</Bars><Repeat start="true" end="false" count="0"/><Section><Letter><![CDATA[A]]></Letter><Text><![CDATA[Riff]]></Text></Section></MasterBar>
<MasterBar><Key><AccidentalCount>0</AccidentalCount><Mode>Major</Mode></Key><Time>4/4</Time><Bars>1 4</Bars><Repeat start="false" end="true" count="3"/></MasterBar>
<MasterBar><Key><AccidentalCount>0</AccidentalCount><Mode>Major</Mode></Key><Time>3/4</Time><Bars>2 5</Bars></MasterBar>
</MasterBars>
<Bars>
<Bar id="0"><Clef>G2</Clef><Voices>0 -1 -1 -1</Voices></Bar>
<Bar id="1"><Clef>G2</Clef><Voices>1 -1 -1 -1</Voices></Bar>
<Bar id="2"><Clef>G2</Clef><Voices>2 -1 -1 -1</Voices></Bar>
<Bar id="3"><Clef>Neutral</Clef><Voices>3 -1 -1 -1</Voices></Bar>
<Bar id="4"><Clef>Neutral</Clef><Voices>4 -1 -1 -1</Voices></Bar>
<Bar id="5"><Clef>Neutral</Clef><Voices>5 -1 -1 -1</Voices></Bar>
</Bars>
<Voices>
<Voice id="0"><Beats>0 1 1 0 1 1 2 3</Beats></Voice>
<Voice id="1"><Beats>4 5</Beats></Voice>
<Voice id="2"><Beats>6 7 6 6</Beats></Voice>
<Voice id="3"><Beats>10 11 10 11 10 11 10 11</Beats></Voice>
<Voice id="4"><Beats>12 13</Beats></Voice>
<Voice id="5"><Beats>14 14 14</Beats></Voice>
</Voices>
<Beats>
<Beat id="0"><Dynamic>F</Dynamic><Rhythm ref="0"/><Notes>0</Notes></Beat>
<Beat id="1"><Dynamic>F</Dynamic><Rhythm ref="1"/><Notes>0</Notes></Beat>
<Beat id="2"><Dynamic>FF</Dynamic><Rhythm ref="2"/><Notes>1 2</Notes></Beat>
<Beat id="3"><Rhythm ref="2"/></Beat>
<Beat id="4"><Dynamic>MF</Dynamic><Rhythm ref="3"/><Notes>3</Notes></Beat>
<Beat id="5"><Dynamic>MF</Dynamic><Rhythm ref="3"/><Notes>4</Notes></Beat>
<Beat id="6"><Dynamic>MF</Dynamic><Rhythm ref="4"/><Notes>5</Notes></Beat>
<Beat id="7"><GraceNotes>BeforeBeat</GraceNotes><Rhythm ref="1"/><Notes>5</Notes></Beat>
<Beat id="10"><Dynamic>F</Dynamic><Rhythm ref="0"/><Notes>20 21</Notes></Beat>
<Beat id="11"><Dynamic>F</Dynamic><Rhythm ref="0"/><Notes>22 21</Notes></Beat>
<Beat id="12"><Dynamic>FF</Dynamic><Rhythm ref="3"/><Notes>23 20</Notes></Beat>
<Beat id="13"><Rhythm ref="3"/></Beat>
<Beat id="14"><Dynamic>MF</Dynamic><Rhythm ref="4"/><Notes>24</Notes></Beat>
</Beats>
<Notes>
<Note id="0"><InstrumentArticulation>0</InstrumentArticulation><Properties><Property name="String"><String>0</String></Property><Property name="Fret"><Fret>0</Fret></Property><Property name="PalmMuted"><Enable/></Property></Properties></Note>
<Note id="1"><Accent>8</Accent><InstrumentArticulation>0</InstrumentArticulation><Properties><Property name="String"><String>0</String></Property><Property name="Fret"><Fret>3</Fret></Property></Properties></Note>
<Note id="2"><Accent>8</Accent><InstrumentArticulation>0</InstrumentArticulation><Properties><Property name="String"><String>1</String></Property><Property name="Fret"><Fret>3</Fret></Property></Properties></Note>
<Note id="3"><Tie origin="true" destination="false"/><LetRing/><Vibrato>Slight</Vibrato><InstrumentArticulation>0</InstrumentArticulation><Properties><Property name="String"><String>0</String></Property><Property name="Fret"><Fret>5</Fret></Property><Property name="Slide"><Flags>1</Flags></Property></Properties></Note>
<Note id="4"><Tie origin="false" destination="true"/><InstrumentArticulation>0</InstrumentArticulation><Properties><Property name="String"><String>0</String></Property><Property name="Fret"><Fret>7</Fret></Property><Property name="Bended"><Enable/></Property><Property name="BendOriginValue"><Float>0</Float></Property><Property name="BendDestinationValue"><Float>100</Float></Property></Properties></Note>
<Note id="5"><InstrumentArticulation>0</InstrumentArticulation><Properties><Property name="String"><String>3</String></Property><Property name="Fret"><Fret>12</Fret></Property><Property name="Harmonic"><Enable/></Property><Property name="HarmonicType"><HType>Natural</HType></Property></Properties></Note>
<Note id="20"><InstrumentArticulation>1</InstrumentArticulation><Properties><Property name="ConcertPitch"><Pitch><Step>C</Step><Accidental></Accidental><Octave>-1</Octave></Pitch></Property></Properties></Note>
<Note id="21"><InstrumentArticulation>2</InstrumentArticulation><Properties></Properties></Note>
<Note id="22"><InstrumentArticulation>0</InstrumentArticulation><Properties><Property name="Midi"><Number>38</Number></Property></Properties></Note>
<Note id="23"><InstrumentArticulation>4</InstrumentArticulation><Properties></Properties></Note>
<Note id="24"><InstrumentArticulation>0</InstrumentArticulation><Properties><Property name="Midi"><Number>51</Number></Property></Properties></Note>
</Notes>
<Rhythms>
<Rhythm id="0"><NoteValue>Eighth</NoteValue></Rhythm>
<Rhythm id="1"><NoteValue>16th</NoteValue></Rhythm>
<Rhythm id="2"><NoteValue>Quarter</NoteValue></Rhythm>
<Rhythm id="3"><NoteValue>Half</NoteValue></Rhythm>
<Rhythm id="4"><NoteValue>Quarter</NoteValue></Rhythm>
</Rhythms>
</GPIF>
"#;

fn main() -> ExitCode {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/guitar-pro/import-test.gp");
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path)?);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("VERSION", opts)?;
        zip.write_all(b"7.0")?;
        zip.add_directory("Content/", opts)?;
        zip.start_file("Content/score.gpif", opts)?;
        zip.write_all(GPIF.as_bytes())?;
        zip.finish()?;
        Ok(())
    };
    match write() {
        Ok(()) => {
            println!("wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
