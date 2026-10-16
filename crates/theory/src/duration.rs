use serde::{Deserialize, Serialize};

/// Sequencer resolution: ticks in a quarter note. Divisible by every value down to a 64th, by
/// triplets and by quintuplets, so durations stay exact integers.
pub const TICKS_PER_QUARTER: u32 = 960;

/// The written note value, whole note down to a 64th.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl NoteValue {
    pub const ALL: [NoteValue; 7] =
        [NoteValue::Whole, NoteValue::Half, NoteValue::Quarter, NoteValue::Eighth, NoteValue::Sixteenth, NoteValue::ThirtySecond, NoteValue::SixtyFourth];

    /// The divisor: 1 for a whole note, 4 for a quarter, 64 for a 64th.
    pub fn divisor(self) -> u32 {
        match self {
            NoteValue::Whole => 1,
            NoteValue::Half => 2,
            NoteValue::Quarter => 4,
            NoteValue::Eighth => 8,
            NoteValue::Sixteenth => 16,
            NoteValue::ThirtySecond => 32,
            NoteValue::SixtyFourth => 64,
        }
    }

    pub fn from_divisor(divisor: u32) -> Option<NoteValue> {
        NoteValue::ALL.into_iter().find(|v| v.divisor() == divisor)
    }

    /// One step longer (whole stays whole).
    pub fn longer(self) -> NoteValue {
        NoteValue::from_divisor(self.divisor() / 2).unwrap_or(NoteValue::Whole)
    }

    /// One step shorter (a 64th stays a 64th).
    pub fn shorter(self) -> NoteValue {
        NoteValue::from_divisor(self.divisor() * 2).unwrap_or(NoteValue::SixtyFourth)
    }
}

/// A beat's length: a note value, up to two dots, and an optional `n` in the time of `m` tuplet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Duration {
    pub value: NoteValue,
    #[serde(default)]
    pub dots: u8,
    /// `(enters, times)`: `(3, 2)` is a triplet. `(1, 1)` is no tuplet.
    #[serde(default = "no_tuplet")]
    pub tuplet: (u8, u8),
}

fn no_tuplet() -> (u8, u8) {
    (1, 1)
}

impl Duration {
    pub const WHOLE: Duration = Duration::new(NoteValue::Whole);
    pub const QUARTER: Duration = Duration::new(NoteValue::Quarter);
    pub const EIGHTH: Duration = Duration::new(NoteValue::Eighth);

    pub const fn new(value: NoteValue) -> Duration {
        Duration { value, dots: 0, tuplet: (1, 1) }
    }

    pub fn dotted(self, dots: u8) -> Duration {
        Duration { dots: dots.min(2), ..self }
    }

    /// Length in ticks. Never zero for a valid duration; a malformed tuplet counts as none.
    pub fn ticks(self) -> u32 {
        let base = TICKS_PER_QUARTER * 4 / self.value.divisor();
        let dots = u32::from(self.dots.min(2));
        // base * (2 - 2^-dots), kept in integers.
        let dotted = base * ((2u32 << dots) - 1) / (1u32 << dots);
        let (enters, times) = self.tuplet;
        if enters == 0 || times == 0 { dotted } else { dotted * u32::from(times) / u32::from(enters) }
    }
}

impl Default for Duration {
    fn default() -> Self {
        Duration::QUARTER
    }
}

/// A time signature such as 4/4 or 6/8.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl TimeSignature {
    pub const COMMON: TimeSignature = TimeSignature { numerator: 4, denominator: 4 };

    /// `None` unless the numerator is 1..=32 and the denominator a power of two from 1 to 64.
    pub fn new(numerator: u8, denominator: u8) -> Option<TimeSignature> {
        let ok = (1..=32).contains(&numerator) && NoteValue::from_divisor(u32::from(denominator)).is_some();
        ok.then_some(TimeSignature { numerator, denominator })
    }

    /// The measure length in ticks.
    pub fn measure_ticks(self) -> u32 {
        u32::from(self.numerator) * TICKS_PER_QUARTER * 4 / u32::from(self.denominator.max(1))
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        TimeSignature::COMMON
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_values() {
        assert_eq!(Duration::WHOLE.ticks(), 3840);
        assert_eq!(Duration::QUARTER.ticks(), 960);
        assert_eq!(Duration::new(NoteValue::SixtyFourth).ticks(), 60);
    }

    #[test]
    fn dots_and_tuplets() {
        assert_eq!(Duration::QUARTER.dotted(1).ticks(), 1440);
        assert_eq!(Duration::QUARTER.dotted(2).ticks(), 1680);
        assert_eq!(Duration::new(NoteValue::SixtyFourth).dotted(2).ticks(), 105);
        let triplet = Duration { tuplet: (3, 2), ..Duration::EIGHTH };
        assert_eq!(triplet.ticks(), 320);
    }

    #[test]
    fn hostile_input_does_not_panic() {
        let d = Duration { value: NoteValue::Whole, dots: 255, tuplet: (0, 0) };
        assert_eq!(d.ticks(), 3840 * 7 / 4);
        assert_eq!(TimeSignature::new(0, 4), None);
        assert_eq!(TimeSignature::new(4, 5), None);
        assert_eq!(TimeSignature { numerator: 255, denominator: 0 }.measure_ticks(), 255 * 3840);
    }

    #[test]
    fn measure_lengths() {
        assert_eq!(TimeSignature::COMMON.measure_ticks(), 3840);
        assert_eq!(TimeSignature::new(6, 8).map(TimeSignature::measure_ticks), Some(2880));
    }

    #[test]
    fn stepping() {
        assert_eq!(NoteValue::Quarter.longer(), NoteValue::Half);
        assert_eq!(NoteValue::Whole.longer(), NoteValue::Whole);
        assert_eq!(NoteValue::SixtyFourth.shorter(), NoteValue::SixtyFourth);
    }
}
