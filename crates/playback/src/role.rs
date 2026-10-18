//! Which kind of instrument a track is, for choosing its sampled sound: the roles of a sound
//! bank (`bank.json`).

use guitario_score::Track;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Drums,
    Bass,
    GuitarClean,
    GuitarDistorted,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Drums, Role::Bass, Role::GuitarClean, Role::GuitarDistorted];

    pub fn key(self) -> &'static str {
        match self {
            Role::Drums => "drums",
            Role::Bass => "bass",
            Role::GuitarClean => "guitar_clean",
            Role::GuitarDistorted => "guitar_distorted",
        }
    }

    /// The role of a track, from its kind and General MIDI program. Programs this bank has no
    /// sampled equivalent for (strings, pads, keys) return `None`.
    pub fn of(track: &Track) -> Option<Role> {
        if track.is_drums() {
            return Some(Role::Drums);
        }
        match track.program {
            29 | 30 => Some(Role::GuitarDistorted),
            24..=28 | 31 => Some(Role::GuitarClean),
            32..=39 => Some(Role::Bass),
            _ => None,
        }
    }
}

impl Role {
    /// The role named `key` in `bank.json` (`drums`, `bass`, `guitar_clean`, `guitar_distorted`).
    pub fn from_key(key: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.key() == key)
    }
}
