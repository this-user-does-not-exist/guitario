//! The tuner's analysis as a command: which note one string plays and how far, and how far each
//! string of the cursor track's tuning is in a strum. The shell feeds it the sound card's input.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_playback::tuner;
use serde_json::{Value, json};

/// Most samples one call analyses: about a second and a half at 44.1 kHz.
pub const MAX_SAMPLES: usize = 65_536;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd(
        "tools.tune",
        "Tuner",
        "",
        None,
        "samples (array of numbers, mono, at most 65536; the tuner listens to the last 0.2 s), sampleRate (8000..=192000): the note one string plays and its cents, and each string of the track's tuning in a strum (lowest first; null when not heard)",
        false,
        tune,
    )]
}

fn tune(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let rate = p::uint(params, "sampleRate", 192_000)?;
    if rate < 8_000 {
        return Err(EngineError::BadParam("`sampleRate` must be at least 8000".into()));
    }
    let list = params.get("samples").and_then(Value::as_array).ok_or_else(|| EngineError::BadParam("`samples` must be an array of numbers".into()))?;
    if list.len() > MAX_SAMPLES {
        return Err(EngineError::BadParam(format!("`samples` holds more than {MAX_SAMPLES} values")));
    }
    let samples: Vec<f32> = list
        .iter()
        .map(|v| {
            v.as_f64()
                .filter(|x| x.is_finite())
                .map(|x| x.clamp(-10.0, 10.0) as f32)
                .ok_or_else(|| EngineError::BadParam("each sample is a finite number".into()))
        })
        .collect::<Result<_, _>>()?;
    let rate = rate as u32;
    let track = s.score.tracks.get(s.cursor.track).ok_or_else(|| EngineError::invalid("no such track"))?;
    if track.is_drums() {
        return Err(EngineError::invalid("a drum track has no strings to tune"));
    }
    // Strings lowest first, as the tuner shows them, with the capo.
    let open: Vec<u8> = track.tuning.strings.iter().rev().map(|o| o.saturating_add(track.capo)).collect();
    let targets: Vec<f32> = open.iter().map(|k| tuner::frequency_of(f32::from(*k))).collect();
    let heard = tuner::strings(&samples, rate, &targets);
    let strings: Vec<Value> = open
        .iter()
        .zip(&targets)
        .zip(&heard)
        .map(|((k, f), c)| json!({"note": guitario_theory::Pitch(*k).name(), "frequency": round(*f), "cents": c.map(round)}))
        .collect();
    let single = tuner::detect(&samples, rate).map(|d| {
        let (key, cents) = tuner::nearest_note(d.frequency);
        json!({"note": guitario_theory::Pitch(key).name(), "frequency": round(d.frequency), "cents": round(cents), "clarity": round(d.clarity)})
    });
    Ok(json!({"single": single, "strings": strings}))
}

fn round(x: f32) -> f64 {
    (f64::from(x) * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::json;

    fn tone(freq: f64, rate: f64, seconds: f64) -> Vec<f64> {
        (0..(rate * seconds) as usize).map(|i| 0.3 * (2.0 * std::f64::consts::PI * freq * i as f64 / rate).sin()).collect()
    }

    #[test]
    fn a_string_a_little_flat_reads_as_such() {
        let mut s = Session::new();
        // A2 eight cents flat.
        let freq = 110.0 * 2f64.powf(-8.0 / 1200.0);
        let r = s.run("tools.tune", &json!({"samples": tone(freq, 44_100.0, 0.25), "sampleRate": 44_100})).unwrap();
        assert_eq!(r["single"]["note"], "A2");
        assert!((r["single"]["cents"].as_f64().unwrap() + 8.0).abs() <= 2.0, "{r}");
        let strings = r["strings"].as_array().unwrap();
        assert_eq!(strings.iter().map(|v| v["note"].as_str().unwrap()).collect::<Vec<_>>(), ["E2", "A2", "D3", "G3", "B3", "E4"]);
        assert!((strings[1]["cents"].as_f64().unwrap() + 8.0).abs() <= 3.0);
        assert!(strings[2]["cents"].is_null(), "the D string is not played");
        let quiet = s.run("tools.tune", &json!({"samples": vec![0.0; 9000], "sampleRate": 48_000})).unwrap();
        assert!(quiet["single"].is_null());
    }

    #[test]
    fn hostile_input_is_refused() {
        let mut s = Session::new();
        for bad in [
            json!({}),
            json!({"samples": [0.0], "sampleRate": 100}),
            json!({"samples": "loud", "sampleRate": 44100}),
            json!({"samples": [0.0, "x"], "sampleRate": 44100}),
            json!({"samples": vec![0.0; 70_000], "sampleRate": 44100}),
        ] {
            assert!(s.run("tools.tune", &bad).is_err(), "{bad}");
        }
        assert!(s.run("tools.tune", &json!({"samples": [1e30, -1e30, 0.5], "sampleRate": 44100})).is_ok(), "huge values are clamped");
    }
}
