//! A track's sound after the instrument: Guitar Pro 8's effect chain (up to six effects, such as
//! pedals, an amp and a cabinet), the mixer's equalizer, and the song's master section (volume,
//! pan and the three mastering slots: compressor, reverb, equalizer).
//!
//! Every effect is a type with a table of parameters ([`EffectType::params`]), so validation, the
//! engine's commands and the UI's knobs all read the same table. The effect models are original;
//! only the layout of Guitar Pro's chain is followed.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most effects a track's chain holds, as in Guitar Pro.
pub const MAX_TRACK_EFFECTS: usize = 6;

/// The kinds of effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectType {
    Compressor,
    NoiseGate,
    Overdrive,
    Distortion,
    Amp,
    Cabinet,
    /// A ten-band graphic equalizer (the mastering equalizer is one).
    Equalizer,
    Chorus,
    Tremolo,
    Delay,
    Reverb,
    /// The mixer's five-band equalizer: low shelf, three peaks, high shelf. Not offered in a chain.
    MixerEq,
}

/// How a parameter is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// A knob from 0 to 10.
    Knob,
    Decibels,
    Hertz,
    Milliseconds,
    Percent,
    /// A ratio such as 4:1.
    Ratio,
    /// A filter's Q.
    Q,
    /// One of `choices`, by index.
    Choice,
}

/// One parameter of an effect type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    /// The id used in files and commands (camelCase).
    pub name: &'static str,
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub unit: Unit,
    /// The names of a [`Unit::Choice`] parameter's values.
    pub choices: &'static [&'static str],
}

const fn knob(name: &'static str, label: &'static str, default: f32) -> ParamSpec {
    ParamSpec { name, label, min: 0.0, max: 10.0, default, unit: Unit::Knob, choices: &[] }
}

const fn ranged(name: &'static str, label: &'static str, min: f32, max: f32, default: f32, unit: Unit) -> ParamSpec {
    ParamSpec { name, label, min, max, default, unit, choices: &[] }
}

const fn db(name: &'static str, label: &'static str) -> ParamSpec {
    ranged(name, label, -12.0, 12.0, 0.0, Unit::Decibels)
}

const fn choice(name: &'static str, label: &'static str, choices: &'static [&'static str]) -> ParamSpec {
    ParamSpec { name, label, min: 0.0, max: (choices.len() as f32 - 1.0), default: 0.0, unit: Unit::Choice, choices }
}

/// The amp's voicings.
pub const AMP_MODELS: [&str; 4] = ["Clean", "Crunch", "Lead", "Bass"];
/// The cabinet's speakers.
pub const CABINET_MODELS: [&str; 4] = ["1x12 Open", "2x12 Open", "4x12 Closed", "4x10 Bass"];
/// The graphic equalizer's band centres, in hertz.
pub const GRAPHIC_BANDS: [f32; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

const COMPRESSOR: &[ParamSpec] = &[
    ranged("threshold", "Threshold", -60.0, 0.0, -18.0, Unit::Decibels),
    ranged("ratio", "Ratio", 1.0, 20.0, 4.0, Unit::Ratio),
    ranged("attack", "Attack", 0.1, 100.0, 10.0, Unit::Milliseconds),
    ranged("release", "Release", 10.0, 1000.0, 120.0, Unit::Milliseconds),
    ranged("gain", "Gain", 0.0, 24.0, 3.0, Unit::Decibels),
];
const NOISE_GATE: &[ParamSpec] =
    &[ranged("threshold", "Threshold", -90.0, 0.0, -55.0, Unit::Decibels), ranged("release", "Release", 5.0, 500.0, 60.0, Unit::Milliseconds)];
const DRIVE: &[ParamSpec] = &[knob("drive", "Drive", 5.0), knob("tone", "Tone", 5.0), knob("level", "Level", 5.0)];
const AMP: &[ParamSpec] = &[
    choice("model", "Model", &AMP_MODELS),
    knob("gain", "Gain", 4.0),
    knob("bass", "Bass", 5.0),
    knob("mid", "Middle", 5.0),
    knob("treble", "Treble", 5.0),
    knob("presence", "Presence", 5.0),
    knob("master", "Master", 5.0),
];
const CABINET: &[ParamSpec] = &[choice("model", "Speakers", &CABINET_MODELS), knob("mic", "Mic position", 3.0)];
const EQUALIZER: &[ParamSpec] = &[
    db("b31", "31"),
    db("b62", "62"),
    db("b125", "125"),
    db("b250", "250"),
    db("b500", "500"),
    db("b1k", "1k"),
    db("b2k", "2k"),
    db("b4k", "4k"),
    db("b8k", "8k"),
    db("b16k", "16k"),
    db("level", "Level"),
];
const CHORUS: &[ParamSpec] =
    &[ranged("rate", "Rate", 0.1, 5.0, 0.8, Unit::Hertz), knob("depth", "Depth", 5.0), ranged("mix", "Mix", 0.0, 100.0, 50.0, Unit::Percent)];
const TREMOLO: &[ParamSpec] = &[ranged("rate", "Rate", 0.5, 15.0, 5.0, Unit::Hertz), knob("depth", "Depth", 5.0)];
const DELAY: &[ParamSpec] = &[
    ranged("time", "Time", 20.0, 2000.0, 375.0, Unit::Milliseconds),
    ranged("feedback", "Feedback", 0.0, 95.0, 35.0, Unit::Percent),
    ranged("mix", "Mix", 0.0, 100.0, 25.0, Unit::Percent),
    knob("tone", "Tone", 6.0),
];
const REVERB: &[ParamSpec] = &[
    knob("size", "Size", 5.0),
    knob("damping", "Damping", 5.0),
    ranged("predelay", "Pre-delay", 0.0, 200.0, 10.0, Unit::Milliseconds),
    ranged("mix", "Mix", 0.0, 100.0, 20.0, Unit::Percent),
];
const MIXER_EQ: &[ParamSpec] = &[
    db("low", "Low"),
    db("lowMid", "Low Medium"),
    ranged("lowMidFreq", "Low Medium Hz", 100.0, 1000.0, 250.0, Unit::Hertz),
    ranged("midQ", "Medium Q", 0.3, 6.0, 1.0, Unit::Q),
    db("mid", "Medium"),
    ranged("midFreq", "Medium Hz", 300.0, 5000.0, 1000.0, Unit::Hertz),
    db("highMid", "High Medium"),
    ranged("highMidFreq", "High Medium Hz", 1000.0, 10000.0, 3000.0, Unit::Hertz),
    db("high", "High"),
];

type Preset = (&'static str, &'static [(&'static str, f32)]);

const COMPRESSOR_PRESETS: &[Preset] = &[
    ("Gentle", &[("threshold", -12.0), ("ratio", 2.0), ("attack", 20.0), ("release", 200.0), ("gain", 2.0)]),
    ("Glue", &[("threshold", -18.0), ("ratio", 3.0), ("attack", 10.0), ("release", 150.0), ("gain", 3.0)]),
    ("Squash", &[("threshold", -30.0), ("ratio", 8.0), ("attack", 3.0), ("release", 80.0), ("gain", 10.0)]),
    ("Bass", &[("threshold", -20.0), ("ratio", 4.0), ("attack", 15.0), ("release", 150.0), ("gain", 5.0)]),
    ("Limiter", &[("threshold", -6.0), ("ratio", 20.0), ("attack", 0.5), ("release", 60.0), ("gain", 4.0)]),
];
const NOISE_GATE_PRESETS: &[Preset] = &[("Tight", &[("threshold", -45.0), ("release", 25.0)]), ("Medium", &[("threshold", -55.0), ("release", 60.0)])];
const OVERDRIVE_PRESETS: &[Preset] = &[
    ("Mild", &[("drive", 3.0), ("tone", 5.0), ("level", 5.0)]),
    ("Push", &[("drive", 2.0), ("tone", 6.0), ("level", 8.0)]),
    ("Warm", &[("drive", 6.0), ("tone", 4.0), ("level", 5.0)]),
];
const DISTORTION_PRESETS: &[Preset] = &[
    ("Crunchy", &[("drive", 4.0), ("tone", 5.0), ("level", 5.0)]),
    ("Saturated", &[("drive", 7.0), ("tone", 6.0), ("level", 5.0)]),
    ("Fuzz Wall", &[("drive", 10.0), ("tone", 4.0), ("level", 4.0)]),
];
const AMP_PRESETS: &[Preset] = &[
    ("Clean Room", &[("model", 0.0), ("gain", 3.0), ("bass", 5.0), ("mid", 5.0), ("treble", 6.0), ("presence", 5.0), ("master", 5.0)]),
    ("Crunch Stack", &[("model", 1.0), ("gain", 5.0), ("bass", 5.0), ("mid", 6.0), ("treble", 6.0), ("presence", 5.0), ("master", 5.0)]),
    ("Lead Stack", &[("model", 2.0), ("gain", 7.0), ("bass", 5.0), ("mid", 6.0), ("treble", 6.0), ("presence", 6.0), ("master", 5.0)]),
    ("Metal Rhythm", &[("model", 2.0), ("gain", 8.0), ("bass", 7.0), ("mid", 3.0), ("treble", 7.0), ("presence", 7.0), ("master", 5.0)]),
    ("Bass Rig", &[("model", 3.0), ("gain", 3.0), ("bass", 6.0), ("mid", 5.0), ("treble", 4.0), ("presence", 4.0), ("master", 5.0)]),
];
const CABINET_PRESETS: &[Preset] = &[
    ("Open 1x12", &[("model", 0.0), ("mic", 3.0)]),
    ("Open 2x12", &[("model", 1.0), ("mic", 3.0)]),
    ("Closed 4x12", &[("model", 2.0), ("mic", 3.0)]),
    ("Bass 4x10", &[("model", 3.0), ("mic", 3.0)]),
];
const EQUALIZER_PRESETS: &[Preset] = &[
    ("Flat", &[]),
    (
        "10-Band",
        &[("b31", -2.0), ("b62", 1.0), ("b125", 1.0), ("b250", 0.0), ("b500", -1.0), ("b1k", 0.0), ("b2k", 1.0), ("b4k", 1.0), ("b8k", 1.0), ("b16k", 0.0)],
    ),
    ("Loudness", &[("b31", 3.0), ("b62", 4.0), ("b125", 2.0), ("b500", -1.0), ("b1k", -1.0), ("b4k", 1.0), ("b8k", 3.0), ("b16k", 3.0)]),
    ("Scooped", &[("b62", 3.0), ("b125", 2.0), ("b250", -1.0), ("b500", -4.0), ("b1k", -5.0), ("b2k", -2.0), ("b4k", 2.0), ("b8k", 3.0)]),
    ("Mid Push", &[("b250", 1.0), ("b500", 3.0), ("b1k", 4.0), ("b2k", 3.0), ("b4k", 1.0)]),
];
const CHORUS_PRESETS: &[Preset] = &[
    ("Subtle", &[("rate", 0.5), ("depth", 3.0), ("mix", 35.0)]),
    ("Lush", &[("rate", 0.9), ("depth", 7.0), ("mix", 50.0)]),
    ("Warble", &[("rate", 3.5), ("depth", 6.0), ("mix", 50.0)]),
];
const TREMOLO_PRESETS: &[Preset] = &[("Slow", &[("rate", 2.5), ("depth", 5.0)]), ("Choppy", &[("rate", 8.0), ("depth", 9.0)])];
const DELAY_PRESETS: &[Preset] = &[
    ("Slapback", &[("time", 110.0), ("feedback", 10.0), ("mix", 25.0), ("tone", 6.0)]),
    ("Quarter", &[("time", 500.0), ("feedback", 35.0), ("mix", 25.0), ("tone", 6.0)]),
    ("Dotted Eighth", &[("time", 375.0), ("feedback", 40.0), ("mix", 30.0), ("tone", 5.0)]),
    ("Ambient", &[("time", 700.0), ("feedback", 65.0), ("mix", 35.0), ("tone", 4.0)]),
];
const REVERB_PRESETS: &[Preset] = &[
    ("Room", &[("size", 3.0), ("damping", 6.0), ("predelay", 5.0), ("mix", 18.0)]),
    ("Studio", &[("size", 5.0), ("damping", 5.0), ("predelay", 10.0), ("mix", 15.0)]),
    ("Hall", &[("size", 8.0), ("damping", 4.0), ("predelay", 25.0), ("mix", 25.0)]),
    ("Plate", &[("size", 6.0), ("damping", 2.0), ("predelay", 0.0), ("mix", 22.0)]),
];
const MIXER_EQ_PRESETS: &[Preset] = &[
    ("Flat", &[]),
    (
        "Electric Guitar",
        &[("low", -3.0), ("lowMid", -1.0), ("lowMidFreq", 300.0), ("mid", 1.0), ("midFreq", 1200.0), ("highMid", 2.0), ("highMidFreq", 3000.0), ("high", -1.0)],
    ),
    (
        "Distorted Guitar",
        &[("low", -4.0), ("lowMid", -2.0), ("lowMidFreq", 250.0), ("mid", -2.0), ("midFreq", 700.0), ("highMid", 2.0), ("highMidFreq", 3500.0), ("high", -3.0)],
    ),
    ("Acoustic Guitar", &[("low", -2.0), ("lowMid", -3.0), ("lowMidFreq", 250.0), ("highMid", 2.0), ("highMidFreq", 5000.0), ("high", 3.0)]),
    (
        "Bass",
        &[("low", 3.0), ("lowMid", -2.0), ("lowMidFreq", 400.0), ("mid", 1.0), ("midFreq", 800.0), ("highMid", 2.0), ("highMidFreq", 2500.0), ("high", -2.0)],
    ),
    ("Drums", &[("low", 2.0), ("lowMid", -3.0), ("lowMidFreq", 400.0), ("highMid", 1.0), ("highMidFreq", 5000.0), ("high", 3.0)]),
];

impl EffectType {
    /// Every type a chain may hold, in the order the library lists them.
    pub const CHAIN: [EffectType; 11] = [
        EffectType::Compressor,
        EffectType::NoiseGate,
        EffectType::Overdrive,
        EffectType::Distortion,
        EffectType::Amp,
        EffectType::Cabinet,
        EffectType::Equalizer,
        EffectType::Chorus,
        EffectType::Tremolo,
        EffectType::Delay,
        EffectType::Reverb,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EffectType::Compressor => "Compressor",
            EffectType::NoiseGate => "Noise Gate",
            EffectType::Overdrive => "Overdrive",
            EffectType::Distortion => "Distortion",
            EffectType::Amp => "Amp",
            EffectType::Cabinet => "Cabinet",
            EffectType::Equalizer => "Equalizer",
            EffectType::Chorus => "Chorus",
            EffectType::Tremolo => "Tremolo",
            EffectType::Delay => "Delay",
            EffectType::Reverb => "Reverb",
            EffectType::MixerEq => "Track Equalizer",
        }
    }

    /// The id used in commands and files.
    pub fn id(self) -> &'static str {
        match self {
            EffectType::Compressor => "compressor",
            EffectType::NoiseGate => "noiseGate",
            EffectType::Overdrive => "overdrive",
            EffectType::Distortion => "distortion",
            EffectType::Amp => "amp",
            EffectType::Cabinet => "cabinet",
            EffectType::Equalizer => "equalizer",
            EffectType::Chorus => "chorus",
            EffectType::Tremolo => "tremolo",
            EffectType::Delay => "delay",
            EffectType::Reverb => "reverb",
            EffectType::MixerEq => "mixerEq",
        }
    }

    pub fn from_id(id: &str) -> Option<EffectType> {
        EffectType::CHAIN.into_iter().chain([EffectType::MixerEq]).find(|t| t.id().eq_ignore_ascii_case(id))
    }

    /// Guitar Pro's icon families: a pedal, an amp head or cabinet, or a rack unit.
    pub fn category(self) -> EffectCategory {
        match self {
            EffectType::Overdrive | EffectType::Distortion | EffectType::Chorus | EffectType::Tremolo | EffectType::Delay | EffectType::NoiseGate => {
                EffectCategory::Pedal
            }
            EffectType::Amp | EffectType::Cabinet => EffectCategory::Amp,
            EffectType::Compressor | EffectType::Equalizer | EffectType::Reverb | EffectType::MixerEq => EffectCategory::Rack,
        }
    }

    pub fn params(self) -> &'static [ParamSpec] {
        match self {
            EffectType::Compressor => COMPRESSOR,
            EffectType::NoiseGate => NOISE_GATE,
            EffectType::Overdrive | EffectType::Distortion => DRIVE,
            EffectType::Amp => AMP,
            EffectType::Cabinet => CABINET,
            EffectType::Equalizer => EQUALIZER,
            EffectType::Chorus => CHORUS,
            EffectType::Tremolo => TREMOLO,
            EffectType::Delay => DELAY,
            EffectType::Reverb => REVERB,
            EffectType::MixerEq => MIXER_EQ,
        }
    }

    pub fn param(self, name: &str) -> Option<&'static ParamSpec> {
        self.params().iter().find(|p| p.name == name)
    }

    /// Named settings, each listing the parameters it moves from their defaults.
    pub fn presets(self) -> &'static [Preset] {
        match self {
            EffectType::Compressor => COMPRESSOR_PRESETS,
            EffectType::NoiseGate => NOISE_GATE_PRESETS,
            EffectType::Overdrive => OVERDRIVE_PRESETS,
            EffectType::Distortion => DISTORTION_PRESETS,
            EffectType::Amp => AMP_PRESETS,
            EffectType::Cabinet => CABINET_PRESETS,
            EffectType::Equalizer => EQUALIZER_PRESETS,
            EffectType::Chorus => CHORUS_PRESETS,
            EffectType::Tremolo => TREMOLO_PRESETS,
            EffectType::Delay => DELAY_PRESETS,
            EffectType::Reverb => REVERB_PRESETS,
            EffectType::MixerEq => MIXER_EQ_PRESETS,
        }
    }
}

/// Which icon an effect gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectCategory {
    Pedal,
    Amp,
    Rack,
}

/// One effect: its type, whether it is on, and its parameters (a missing one is at its default).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    #[serde(rename = "type")]
    pub kind: EffectType,
    #[serde(default = "yes")]
    pub on: bool,
    #[serde(default)]
    pub params: BTreeMap<String, f32>,
}

fn yes() -> bool {
    true
}

impl Effect {
    /// An effect at its defaults, on.
    pub fn new(kind: EffectType) -> Effect {
        Effect { kind, on: true, params: kind.params().iter().map(|p| (p.name.to_string(), p.default)).collect() }
    }

    /// An effect set to one of its type's presets (none if the type has no such preset).
    pub fn preset(kind: EffectType, name: &str) -> Option<Effect> {
        let (_, values) = kind.presets().iter().find(|(n, _)| n.eq_ignore_ascii_case(name))?;
        let mut e = Effect::new(kind);
        for (k, v) in *values {
            e.params.insert((*k).to_string(), *v);
        }
        Some(e)
    }

    /// A parameter's value, at its default when unset, kept inside its range; 0 for a name the
    /// type does not have.
    pub fn value(&self, name: &str) -> f32 {
        let Some(spec) = self.kind.param(name) else { return 0.0 };
        match self.params.get(name) {
            Some(v) if v.is_finite() => v.clamp(spec.min, spec.max),
            _ => spec.default,
        }
    }

    /// Sets a parameter, rounding a choice to a whole index.
    pub fn set(&mut self, name: &str, value: f32) -> Result<(), String> {
        let spec = self.kind.param(name).ok_or_else(|| format!("{} has no parameter '{name}'", self.kind.label()))?;
        if !value.is_finite() || value < spec.min || value > spec.max {
            return Err(format!("{} must be {} to {}", spec.label, spec.min, spec.max));
        }
        let value = if spec.unit == Unit::Choice { value.round() } else { value };
        self.params.insert(name.to_string(), value);
        Ok(())
    }

    /// The preset this effect is set to, if it matches one exactly.
    pub fn preset_name(&self) -> Option<&'static str> {
        let same = |p: &Effect| self.kind.params().iter().all(|s| (p.value(s.name) - self.value(s.name)).abs() < 1e-4);
        self.kind.presets().iter().map(|(name, _)| *name).find(|name| Effect::preset(self.kind, name).is_some_and(|p| same(&p)))
    }

    /// Whether every parameter is at its default (a flat equalizer is skipped).
    pub fn is_neutral(&self) -> bool {
        self.kind.params().iter().all(|s| (self.value(s.name) - s.default).abs() < 1e-4)
    }

    /// The mixer's equalizer as a new track has it (left out of saved files).
    pub fn is_flat_mixer_eq(&self) -> bool {
        self.kind == EffectType::MixerEq && self.on && self.is_neutral()
    }

    /// Checks the type's parameters: known names, finite values inside their ranges.
    pub fn check(&self) -> Result<(), String> {
        for (name, v) in &self.params {
            let spec = self.kind.param(name).ok_or_else(|| format!("{} has no parameter '{name}'", self.kind.label()))?;
            if !v.is_finite() || *v < spec.min || *v > spec.max {
                return Err(format!("{} {} is {v}, outside {} to {}", self.kind.label(), spec.label, spec.min, spec.max));
            }
        }
        Ok(())
    }
}

/// The mixer's equalizer: on and flat by default.
pub fn mixer_eq() -> Effect {
    Effect::new(EffectType::MixerEq)
}

/// The chain the built-in synth plays a General MIDI program through when the track has not set
/// its own: an amp and cabinet for the overdriven and distorted guitars, a room for the modelled
/// pianos and a hall for the bowed strings (heard dry they sound boxed in), nothing for the rest.
pub fn default_chain(program: u8) -> Vec<Effect> {
    let amp = |gain: f32, model: f32| {
        let mut a = Effect::new(EffectType::Amp);
        a.params.insert("model".into(), model);
        a.params.insert("gain".into(), gain);
        a
    };
    let cab = |model: f32| {
        let mut c = Effect::new(EffectType::Cabinet);
        c.params.insert("model".into(), model);
        c
    };
    let reverb = |preset: &str, mix: f32| {
        let mut r = Effect::preset(EffectType::Reverb, preset).unwrap_or_else(|| Effect::new(EffectType::Reverb));
        r.params.insert("mix".into(), mix);
        r
    };
    match program {
        29 => vec![amp(5.0, 1.0), cab(1.0)],
        30 => vec![Effect::new(EffectType::NoiseGate), amp(7.5, 2.0), cab(2.0)],
        0..=3 => vec![reverb("Studio", 14.0)],
        40..=45 | 48 | 49 | 110 => vec![reverb("Hall", 18.0)],
        _ => Vec::new(),
    }
}

impl crate::Track {
    /// The effect chain the track shows: its own, or its instrument's ([`default_chain`]).
    pub fn chain(&self) -> Vec<Effect> {
        self.effects.clone().unwrap_or_else(|| default_chain(self.program))
    }
}

/// The song's master section: volume and pan after the track mix, then the three mastering slots
/// in Guitar Pro's fixed order (compressor, reverb, equalizer), each its own type, off by default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Master {
    /// 0..=127; 100 is unity gain.
    pub volume: u8,
    /// -63..=63.
    pub pan: i8,
    pub compressor: Effect,
    pub reverb: Effect,
    pub equalizer: Effect,
}

impl Default for Master {
    fn default() -> Self {
        let off = |kind: EffectType, preset: &str| {
            let mut e = Effect::preset(kind, preset).unwrap_or_else(|| Effect::new(kind));
            e.on = false;
            e
        };
        Master {
            volume: 100,
            pan: 0,
            compressor: off(EffectType::Compressor, "Glue"),
            reverb: off(EffectType::Reverb, "Studio"),
            equalizer: off(EffectType::Equalizer, "10-Band"),
        }
    }
}

/// The names of the master's slots, in order.
pub const MASTER_SLOTS: [(&str, EffectType); 3] =
    [("compressor", EffectType::Compressor), ("reverb", EffectType::Reverb), ("equalizer", EffectType::Equalizer)];

impl Master {
    pub fn is_default(&self) -> bool {
        *self == Master::default()
    }

    pub fn slots(&self) -> [&Effect; 3] {
        [&self.compressor, &self.reverb, &self.equalizer]
    }

    pub fn slot_mut(&mut self, index: usize) -> Option<&mut Effect> {
        match index {
            0 => Some(&mut self.compressor),
            1 => Some(&mut self.reverb),
            2 => Some(&mut self.equalizer),
            _ => None,
        }
    }

    pub fn check(&self) -> Result<(), String> {
        if self.volume > 127 || !(-63..=63).contains(&self.pan) {
            return Err("master volume must be 0..=127 and pan -63..=63".into());
        }
        for (e, (name, kind)) in self.slots().into_iter().zip(MASTER_SLOTS) {
            if e.kind != kind {
                return Err(format!("the master {name} slot holds a {}", e.kind.label()));
            }
            e.check()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_and_defaults_are_inside_their_ranges() {
        for kind in EffectType::CHAIN.into_iter().chain([EffectType::MixerEq]) {
            assert_eq!(EffectType::from_id(kind.id()), Some(kind));
            for spec in kind.params() {
                assert!(spec.min <= spec.default && spec.default <= spec.max, "{kind:?} {}", spec.name);
                assert!(spec.unit != Unit::Choice || spec.choices.len() as f32 - 1.0 == spec.max);
            }
            for (name, _) in kind.presets() {
                let e = Effect::preset(kind, name).unwrap();
                e.check().unwrap();
                assert_eq!(e.preset_name(), Some(*name), "{kind:?} {name}");
            }
        }
        assert!(Effect::new(EffectType::Equalizer).is_neutral());
        assert_eq!(Effect::new(EffectType::Equalizer).preset_name(), Some("Flat"));
    }

    #[test]
    fn values_stay_in_range_and_bad_ones_are_refused() {
        let mut e = Effect::new(EffectType::Delay);
        assert!(e.set("time", 5000.0).is_err());
        assert!(e.set("time", f32::NAN).is_err());
        assert!(e.set("nope", 1.0).is_err());
        e.set("time", 250.0).unwrap();
        assert_eq!(e.value("time"), 250.0);
        e.params.insert("time".into(), 1e9);
        assert_eq!(e.value("time"), 2000.0, "read back clamped");
        assert!(e.check().is_err());
        let mut amp = Effect::new(EffectType::Amp);
        amp.set("model", 1.6).unwrap();
        assert_eq!(amp.value("model"), 2.0);
        assert_eq!(e.value("nope"), 0.0);
    }

    #[test]
    fn files_round_trip_and_older_ones_load() {
        let mut master = Master::default();
        master.reverb.on = true;
        master.volume = 90;
        let text = serde_json::to_string(&master).unwrap();
        assert_eq!(serde_json::from_str::<Master>(&text).unwrap(), master);
        assert_eq!(serde_json::from_str::<Master>("{}").unwrap(), Master::default());
        let e: Effect = serde_json::from_str(r#"{"type":"reverb"}"#).unwrap();
        assert!(e.on && e.value("mix") == 20.0);
        master.reverb = Effect::new(EffectType::Delay);
        assert!(master.check().is_err());
        assert!(default_chain(30).len() == 3 && default_chain(25).is_empty());
        assert!(default_chain(0).iter().chain(&default_chain(42)).all(|e| e.kind == EffectType::Reverb));
    }
}
