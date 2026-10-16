use crate::Pitch;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TuningError {
    #[error("a tuning needs 1 to 12 strings, got {0}")]
    StringCount(usize),
    #[error("open-string pitch {0} is outside MIDI range 0..=127")]
    Pitch(u8),
}

/// Open-string pitches, **highest string first** (string 0 is the thin high E on a guitar, which is
/// the top line of a tab staff).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tuning {
    pub name: String,
    pub strings: Vec<u8>,
}

impl Tuning {
    pub const MAX_STRINGS: usize = 12;

    pub fn new(name: impl Into<String>, strings: Vec<u8>) -> Result<Tuning, TuningError> {
        let t = Tuning { name: name.into(), strings };
        t.validate()?;
        Ok(t)
    }

    pub fn validate(&self) -> Result<(), TuningError> {
        if !(1..=Self::MAX_STRINGS).contains(&self.strings.len()) {
            return Err(TuningError::StringCount(self.strings.len()));
        }
        match self.strings.iter().find(|p| **p > 127) {
            Some(p) => Err(TuningError::Pitch(*p)),
            None => Ok(()),
        }
    }

    pub fn string_count(&self) -> usize {
        self.strings.len()
    }

    /// The sounding pitch of `fret` on `string` with a capo at `capo`, or `None` when the string
    /// does not exist or the result is outside MIDI range.
    pub fn pitch(&self, string: usize, fret: u8, capo: u8) -> Option<Pitch> {
        let open = *self.strings.get(string)?;
        Pitch(open).offset(i16::from(fret) + i16::from(capo))
    }

    pub fn standard() -> Tuning {
        Tuning { name: "Standard".into(), strings: vec![64, 59, 55, 50, 45, 40] }
    }

    pub fn drop_d() -> Tuning {
        Tuning { name: "Drop D".into(), strings: vec![64, 59, 55, 50, 45, 38] }
    }

    pub fn dadgad() -> Tuning {
        Tuning { name: "DADGAD".into(), strings: vec![62, 57, 55, 50, 45, 38] }
    }

    pub fn open_g() -> Tuning {
        Tuning { name: "Open G".into(), strings: vec![62, 59, 55, 50, 43, 38] }
    }

    pub fn half_step_down() -> Tuning {
        Tuning { name: "Half step down".into(), strings: vec![63, 58, 54, 49, 44, 39] }
    }

    pub fn bass() -> Tuning {
        Tuning { name: "Bass (4 string)".into(), strings: vec![43, 38, 33, 28] }
    }

    pub fn seven_string() -> Tuning {
        Tuning { name: "7 string".into(), strings: vec![64, 59, 55, 50, 45, 40, 35] }
    }

    pub fn ukulele() -> Tuning {
        Tuning { name: "Ukulele".into(), strings: vec![69, 64, 60, 67] }
    }

    /// Whole step down: D G C F A D.
    pub fn d_standard() -> Tuning {
        Tuning { name: "D Standard".into(), strings: vec![62, 57, 53, 48, 43, 38] }
    }

    /// Two whole steps down: C G C F A D.
    pub fn drop_c() -> Tuning {
        Tuning { name: "Drop C".into(), strings: vec![62, 57, 53, 48, 43, 36] }
    }

    /// B F# B E G# C#, the usual metal and metalcore tuning.
    pub fn drop_b() -> Tuning {
        Tuning { name: "Drop B".into(), strings: vec![61, 56, 52, 47, 42, 35] }
    }

    /// Standard seven-string with the low string dropped to A: A E A D G B E.
    pub fn seven_string_drop_a() -> Tuning {
        Tuning { name: "7 String Drop A".into(), strings: vec![64, 59, 55, 50, 45, 40, 33] }
    }

    /// A drum track's eight kit lanes (see [`crate::drums::LANES`]). Every lane is "tuned" to 0, so
    /// a note's fret is its General MIDI drum number.
    pub fn drum_kit() -> Tuning {
        Tuning { name: "Drum kit".into(), strings: vec![0; crate::drums::LANES.len()] }
    }

    pub fn presets() -> Vec<Tuning> {
        vec![
            Tuning::standard(),
            Tuning::drop_d(),
            Tuning::d_standard(),
            Tuning::drop_c(),
            Tuning::drop_b(),
            Tuning::half_step_down(),
            Tuning::dadgad(),
            Tuning::open_g(),
            Tuning::seven_string(),
            Tuning::seven_string_drop_a(),
            Tuning::bass(),
            Tuning::ukulele(),
        ]
    }

    /// A preset by case-insensitive name, ignoring spaces and punctuation (`"drop d"`, `"DropD"`).
    pub fn preset(name: &str) -> Option<Tuning> {
        let key = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
        let want = key(name);
        let all = Tuning::presets();
        let exact = all.iter().position(|t| key(&t.name) == want);
        let prefix = || all.iter().position(|t| !want.is_empty() && key(&t.name).starts_with(&want));
        exact.or_else(prefix).and_then(|i| all.into_iter().nth(i))
    }
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning::standard()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_pitches() {
        let t = Tuning::standard();
        assert_eq!(t.pitch(0, 0, 0), Some(Pitch(64)));
        assert_eq!(t.pitch(5, 0, 0), Some(Pitch(40)));
        assert_eq!(t.pitch(5, 3, 0), Some(Pitch(43)));
        assert_eq!(t.pitch(5, 3, 2), Some(Pitch(45)));
        assert_eq!(t.pitch(6, 0, 0), None);
        assert_eq!(t.pitch(0, 255, 255), None);
    }

    #[test]
    fn validation() {
        assert_eq!(Tuning::new("x", vec![]), Err(TuningError::StringCount(0)));
        assert_eq!(Tuning::new("x", vec![200]), Err(TuningError::Pitch(200)));
        assert!(Tuning::new("x", vec![40; 12]).is_ok());
        assert!(Tuning::new("x", vec![40; 13]).is_err());
    }

    #[test]
    fn preset_lookup() {
        assert_eq!(Tuning::preset("drop d").map(|t| t.strings.len()), Some(6));
        assert_eq!(Tuning::preset("DropD"), Some(Tuning::drop_d()));
        assert_eq!(Tuning::preset("nope"), None);
        assert_eq!(Tuning::preset("bass"), Some(Tuning::bass()));
        assert_eq!(Tuning::preset("7 string"), Some(Tuning::seven_string()));
        assert_eq!(Tuning::preset("7 String Drop A"), Some(Tuning::seven_string_drop_a()));
        assert_eq!(Tuning::preset("drop c").map(|t| t.strings[5]), Some(36));
    }

    #[test]
    fn metal_tunings_are_sane() {
        // Strings are listed highest first, so pitches must not rise down the list, and every
        // tuning must validate.
        for t in Tuning::presets() {
            assert_eq!(t.validate(), Ok(()), "{}", t.name);
        }
        assert_eq!(Tuning::drop_c().strings, vec![62, 57, 53, 48, 43, 36]);
        // Drop B is Drop C a whole step lower, except for the dropped string which is lower still.
        assert_eq!(Tuning::drop_b().strings.first(), Some(&61));
        assert_eq!(Tuning::drop_b().strings.last(), Some(&35));
        assert_eq!(Tuning::seven_string_drop_a().string_count(), 7);
    }
}
