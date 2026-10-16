//! Lyrics as Guitar Pro types them: words and syllables split by spaces and hyphens, laid one per
//! beat on the track's first voice from the line's bar, skipping rests.
//!
//! `Wa-ter` is two syllables, the first drawn with a hyphen after it; `+` joins words onto one beat
//! (`in+the` is sung on one note); a lone `-` leaves a beat without a syllable; trailing `_`
//! (a held vowel) is dropped; `[text]` is a comment that is not sung.

use crate::{LyricLine, Track};

/// One syllable and whether a hyphen joins it to the next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Syllable {
    pub text: String,
    pub hyphen: bool,
}

/// Most syllables taken from one line, so a hostile text cannot make an unbounded list.
const MAX_SYLLABLES: usize = 20_000;

/// The syllables of `text`, in order. An empty syllable (a lone `-`) leaves its beat without one.
pub fn syllables(text: &str) -> Vec<Syllable> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut comment = false;
    let push = |out: &mut Vec<Syllable>, word: &mut String, hyphen: bool| {
        if out.len() >= MAX_SYLLABLES {
            word.clear();
            return;
        }
        let text = word.trim_end_matches('_').replace('+', " ");
        out.push(Syllable { text, hyphen });
        word.clear();
    };
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if comment {
            comment = c != ']';
            continue;
        }
        match c {
            '[' if word.is_empty() => comment = true,
            c if c.is_whitespace() => {
                if !word.is_empty() {
                    push(&mut out, &mut word, false);
                }
            }
            '-' if word.is_empty() => {
                // A lone dash is a beat left out; dashes that follow a syllable belong to it.
                if chars.peek().is_none_or(|n| n.is_whitespace()) {
                    push(&mut out, &mut word, false);
                }
            }
            '-' => {
                while chars.peek() == Some(&'-') {
                    chars.next();
                }
                push(&mut out, &mut word, true);
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        push(&mut out, &mut word, false);
    }
    out
}

/// Where each syllable of `line` falls on `track`: `(measure, beat, syllable)`, one per beat of the
/// first voice from the line's bar, skipping rests. Syllables past the last beat are not placed.
pub fn dispatch_lyrics(track: &Track, line: &LyricLine) -> Vec<(usize, usize, Syllable)> {
    let mut beats = track
        .measures
        .iter()
        .enumerate()
        .skip(line.measure)
        .flat_map(|(mi, m)| m.voices.first().into_iter().flat_map(move |v| v.beats.iter().enumerate().map(move |(bi, b)| (mi, bi, b))))
        .filter(|(_, _, b)| !b.rest);
    let mut out = Vec::new();
    for syllable in syllables(&line.text) {
        let Some((mi, bi, _)) = beats.next() else { break };
        if !syllable.text.is_empty() {
            out.push((mi, bi, syllable));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Beat, MeasureHeader, Note};
    use guitario_theory::{Duration, TimeSignature, Tuning};

    fn texts(s: &str) -> Vec<(String, bool)> {
        syllables(s).into_iter().map(|x| (x.text, x.hyphen)).collect()
    }

    #[test]
    fn words_hyphens_joins_and_comments() {
        let want = |v: &[(&str, bool)]| v.iter().map(|(t, h)| (t.to_string(), *h)).collect::<Vec<_>>();
        assert_eq!(texts("Steel ho-ri-zon"), want(&[("Steel", false), ("ho", true), ("ri", true), ("zon", false)]));
        assert_eq!(texts("[Verse 1]\n  burn+the sky__  "), want(&[("burn the", false), ("sky", false)]));
        assert_eq!(texts("rise - again"), want(&[("rise", false), ("", false), ("again", false)]), "a lone dash skips a beat");
        assert_eq!(texts("fi--re"), want(&[("fi", true), ("re", false)]));
        assert_eq!(texts(""), vec![]);
        assert_eq!(texts("[never closed"), vec![]);
        // Hostile text stays bounded.
        assert_eq!(syllables(&"a ".repeat(50_000)).len(), MAX_SYLLABLES);
    }

    #[test]
    fn syllables_fall_on_beats_from_their_bar_skipping_rests() {
        let headers = vec![MeasureHeader::new(TimeSignature::COMMON); 2];
        let mut t = Track::new("Lead", Tuning::standard(), &headers);
        let note = Beat { rest: false, notes: vec![Note::new(0, 5)], ..Beat::rest(Duration::QUARTER) };
        let rest = Beat::rest(Duration::QUARTER);
        t.measures[0].voices[0].beats = vec![note.clone(), rest.clone(), note.clone(), note.clone()];
        t.measures[1].voices[0].beats = vec![note.clone(), note.clone(), rest, note];
        let at = |line: &LyricLine| dispatch_lyrics(&t, line).into_iter().map(|(m, b, s)| (m, b, s.text)).collect::<Vec<_>>();
        let line = LyricLine { measure: 0, text: "Steel ho-ri - zon burn".into() };
        assert_eq!(at(&line), vec![(0, 0, "Steel".into()), (0, 2, "ho".into()), (0, 3, "ri".into()), (1, 1, "zon".into()), (1, 3, "burn".into())]);
        // From the second bar; what does not fit is left out; a bar past the end places nothing.
        assert_eq!(at(&LyricLine { measure: 1, text: "a b c d e".into() }).len(), 3);
        assert!(at(&LyricLine { measure: 9, text: "a".into() }).is_empty());
    }
}
