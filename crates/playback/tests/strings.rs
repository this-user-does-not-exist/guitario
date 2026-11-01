//! The bowed strings against recordings of real players. The numbers here were measured on the
//! sustained, vibrato notes of the Sonatina Symphonic Orchestra's soloists and sections
//! (`docs/strings-calibration.md`): how steeply each instrument's harmonics fall. A violin scaled
//! up to a cello or a double bass falls far too slowly (the low strings sounded twice as bright
//! as real ones); each instrument's body is now voiced on its own.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use guitario_score::{Beat, Note, Score};
use guitario_theory::{Duration, Tuning};

const RATE: u32 = 48_000;

/// One long mezzo-forte note on `string` at `fret`, alone or by a section.
fn note(program: u8, strings: &[u8], string: u8, fret: u8, players: u8) -> Score {
    let mut s = Score::new();
    s.headers[0].tempo = Some(60);
    let t = &mut s.tracks[0];
    t.program = program;
    t.players = players;
    t.tuning = Tuning { name: "strings".into(), strings: strings.to_vec() };
    t.measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(string, fret)], ..Beat::rest(Duration::WHOLE) }];
    s
}

/// The power spectrum of `x` (Hann-windowed, zero-padded to a power of two), by an iterative
/// radix-2 FFT.
fn spectrum(x: &[f32]) -> Vec<f64> {
    let n = x.len().next_power_of_two() * 2;
    let len = x.len();
    let mut re: Vec<f64> =
        (0..n).map(|i| if i < len { f64::from(x[i]) * (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / len as f64).cos()) } else { 0.0 }).collect();
    let mut im = vec![0.0f64; n];
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut size = 2;
    while size <= n {
        let step = -std::f64::consts::TAU / size as f64;
        for start in (0..n).step_by(size) {
            for k in 0..size / 2 {
                let (wr, wi) = ((step * k as f64).cos(), (step * k as f64).sin());
                let (a, b) = (start + k, start + k + size / 2);
                let (tr, ti) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                (re[b], im[b]) = (re[a] - tr, im[a] - ti);
                (re[a], im[a]) = (re[a] + tr, im[a] + ti);
            }
        }
        size *= 2;
    }
    (0..n / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect()
}

/// How steeply the note's first sixteen harmonics fall, in dB an octave: a least-squares line
/// through the power within 0.45 f0 of each (a vibrato spreads a partial) against log2 of its
/// number, over two seconds of the note's steady part. The measurement the recordings had.
fn tilt(score: &Score, f0: f64) -> f64 {
    let out = guitario_playback::render_wav(score, RATE);
    let mono: Vec<f32> = out.chunks(2).map(|c| 0.5 * (c[0] + c[1])).collect();
    let spec = spectrum(&mono[(1.5 * f64::from(RATE)) as usize..(3.5 * f64::from(RATE)) as usize]);
    let hz = f64::from(RATE) / (2 * spec.len()) as f64;
    let points: Vec<(f64, f64)> = (1..=16)
        .filter(|k| f64::from(*k) * f0 < 0.45 * f64::from(RATE))
        .map(|k| {
            let c = f64::from(k) * f0;
            let band: f64 = spec.iter().enumerate().filter(|(i, _)| (*i as f64 * hz - c).abs() < 0.45 * f0).map(|(_, p)| p).sum();
            (f64::from(k).log2(), 10.0 * band.max(1e-30).log10())
        })
        .collect();
    let n = points.len() as f64;
    let (mx, my) = (points.iter().map(|p| p.0).sum::<f64>() / n, points.iter().map(|p| p.1).sum::<f64>() / n);
    let cov: f64 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let var: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    cov / var
}

#[test]
fn each_instruments_harmonics_fall_as_a_real_ones_do() {
    // (name, program, tuning, notes as (string, fret, Hz), section, the recordings' tilt: the mean
    // of the soloist and the section on the same four notes).
    type Case = (&'static str, u8, [u8; 4], [(u8, u8, f64); 4], u8, f64);
    let cases: [Case; 4] = [
        ("violin", 40, [76, 69, 62, 55], [(2, 2, 329.63), (1, 4, 554.37), (1, 1, 466.16), (0, 0, 659.26)], 16, -10.2),
        ("viola", 41, [69, 62, 55, 48], [(3, 6, 185.0), (2, 5, 261.63), (2, 2, 220.0), (0, 0, 440.0)], 12, -8.0),
        ("cello", 42, [57, 50, 43, 36], [(3, 6, 92.5), (2, 5, 130.81), (2, 2, 110.0), (1, 4, 185.0)], 10, -8.1),
        ("double bass", 43, [43, 38, 33, 28], [(2, 3, 65.41), (1, 4, 92.5), (0, 2, 110.0), (0, 5, 130.81)], 8, -8.4),
    ];
    for (name, program, tuning, notes, section, real) in cases {
        let mut tilts = Vec::new();
        for (string, fret, f0) in notes {
            for players in [1, section] {
                tilts.push(tilt(&note(program, &tuning, string, fret, players), f0));
            }
        }
        let mean = tilts.iter().sum::<f64>() / tilts.len() as f64;
        println!("{name}: {mean:.1} dB an octave against a real {real:.1} ({tilts:.1?})");
        assert!((mean - real).abs() < 2.0, "{name}: {mean} dB an octave, real players {real}");
    }
}
