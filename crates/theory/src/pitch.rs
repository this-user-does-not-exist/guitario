use serde::{Deserialize, Serialize};

/// A MIDI pitch: 60 is middle C.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Pitch(pub u8);

const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

impl Pitch {
    /// The pitch `semitones` above this one, or `None` if it leaves the MIDI range 0..=127.
    pub fn offset(self, semitones: i16) -> Option<Pitch> {
        let p = i16::from(self.0).checked_add(semitones)?;
        u8::try_from(p).ok().filter(|p| *p <= 127).map(Pitch)
    }

    /// Scientific pitch name, with middle C as C4 (for example `E2`, `F#3`).
    pub fn name(self) -> String {
        format!("{}{}", note_name(self.0), i32::from(self.0 / 12) - 1)
    }

    pub fn frequency(self) -> f32 {
        440.0 * 2f32.powf((f32::from(self.0) - 69.0) / 12.0)
    }
}

/// The pitch-class name (`C`, `C#`, …) of a MIDI note number.
pub fn note_name(midi: u8) -> &'static str {
    NAMES.get(usize::from(midi % 12)).copied().unwrap_or("?")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(Pitch(60).name(), "C4");
        assert_eq!(Pitch(40).name(), "E2");
        assert_eq!(Pitch(0).name(), "C-1");
    }

    #[test]
    fn offsets_stay_in_range() {
        assert_eq!(Pitch(127).offset(1), None);
        assert_eq!(Pitch(0).offset(-1), None);
        assert_eq!(Pitch(40).offset(24), Some(Pitch(64)));
        assert_eq!(Pitch(40).offset(i16::MAX), None);
    }

    #[test]
    fn a440() {
        assert!((Pitch(69).frequency() - 440.0).abs() < 1e-3);
    }
}
