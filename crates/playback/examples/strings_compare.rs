//! Compares the modelled bowed strings with recordings of real players, note for note: the
//! sustained vibrato notes of the Sonatina Symphonic Orchestra's soloists and sections, which
//! `docs/strings-calibration.md` says how to fetch (they are not part of Guitario):
//! `cargo run -p guitario-playback --release --example strings_compare -- <dir with the recordings>`.
//!
//! For each instrument it renders the same notes alone and by an orchestra's section and prints
//! how much stronger the recordings are than the model in each half octave (each note's levels
//! lined up on its partials below 500 Hz), then, for soloists and sections, the medians of what
//! tells them apart: brightness, how steeply the harmonics fall, the noise between them, how far
//! the players' pitches spread, how much a partial's level wanders, and how wide the stereo is.

// A measuring tool over fixed tables and files the user provides.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use guitario_score::{Beat, Note, Score};
use guitario_theory::{Duration, Tuning};
use std::path::{Path, PathBuf};

const RATE: u32 = 48_000;

struct Audio {
    left: Vec<f32>,
    right: Vec<f32>,
    rate: f32,
}

/// A WAV (16, 24 or 32-bit, or float) or FLAC file, mono made stereo.
fn read(path: &Path) -> Option<Audio> {
    let (channels, rate, samples): (usize, u32, Vec<f32>) = if path.extension()? == "flac" {
        let mut r = claxon::FlacReader::open(path).ok()?;
        let info = r.streaminfo();
        let scale = (1u64 << (info.bits_per_sample - 1)) as f32;
        let s = r.samples().map(|s| s.map(|v| v as f32 / scale)).collect::<Result<_, _>>().ok()?;
        (info.channels as usize, info.sample_rate, s)
    } else {
        let mut r = hound::WavReader::open(path).ok()?;
        let spec = r.spec();
        let s = match spec.sample_format {
            hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>().ok()?,
            hound::SampleFormat::Int => {
                let scale = (1u64 << (spec.bits_per_sample - 1)) as f32;
                r.samples::<i32>().map(|s| s.map(|v| v as f32 / scale)).collect::<Result<_, _>>().ok()?
            }
        };
        (spec.channels as usize, spec.sample_rate, s)
    };
    let left: Vec<f32> = samples.chunks(channels).map(|c| c[0]).collect();
    let right: Vec<f32> = samples.chunks(channels).map(|c| *c.get(1).unwrap_or(&c[0])).collect();
    Some(Audio { left, right, rate: rate as f32 })
}

/// MIDI note of a name like `c#4`.
fn midi(name: &str) -> u8 {
    let base = match name.as_bytes()[0] {
        b'c' => 0,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        _ => 11,
    };
    let sharp = name.contains('#');
    let octave: i32 = name.trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == '#').parse().unwrap();
    (12 * (octave + 1) + base + i32::from(sharp)) as u8
}

/// One five-second mezzo-forte note, on the highest string that reaches it, alone or by a section.
fn render(program: u8, strings: &[u8], note: u8, players: u8) -> Audio {
    let string = (0..strings.len()).find(|&s| strings[s] <= note).unwrap();
    let mut s = Score::new();
    s.headers[0].tempo = Some(48);
    let t = &mut s.tracks[0];
    t.program = program;
    t.players = players;
    t.tuning = Tuning { name: "strings".into(), strings: strings.to_vec() };
    let n = Note::new(string as u8, note - strings[string]);
    t.measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![n], ..Beat::rest(Duration::WHOLE) }];
    let out = guitario_playback::render_wav(&s, RATE);
    Audio { left: out.chunks(2).map(|c| c[0]).collect(), right: out.chunks(2).map(|c| c[1]).collect(), rate: RATE as f32 }
}

/// The power spectrum of `x` with a Hann window, zero-padded to `n` (a power of two).
fn spectrum(x: &[f32], n: usize) -> Vec<f64> {
    let len = x.len().min(n);
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

fn median(mut v: Vec<f64>) -> f64 {
    v.retain(|x| x.is_finite());
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

#[derive(Clone)]
struct Measures {
    f0: f64,
    /// Harmonic levels against the whole, dB.
    harm: Vec<f64>,
    tilt: f64,
    centroid: f64,
    noise: f64,
    spread: f64,
    wander: f64,
    corr: f64,
}

/// Measures two and a half seconds of a sustained note, a second after it starts.
fn measure(a: &Audio, nominal: f64) -> Measures {
    let rate = f64::from(a.rate);
    let mono: Vec<f32> = a.left.iter().zip(&a.right).map(|(l, r)| 0.5 * (l + r)).collect();
    let hop = (rate * 0.01) as usize;
    let env: Vec<f32> = mono.chunks(hop).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt()).collect();
    let peak = env.iter().copied().fold(0.0f32, f32::max);
    let onset = env.iter().position(|e| *e > 0.1 * peak).unwrap_or(0) * hop;
    let start = (onset + rate as usize).min(mono.len().saturating_sub(rate as usize));
    let end = (start + (2.5 * rate) as usize).min(mono.len());
    let seg = &mono[start..end];
    let n = (seg.len().next_power_of_two() * 8).min(1 << 21);
    let spec = spectrum(seg, n);
    let hz = rate / n as f64;
    let power = |lo: f64, hi: f64| -> f64 { spec[((lo / hz).max(0.0) as usize).min(spec.len())..((hi / hz) as usize).min(spec.len())].iter().sum() };
    // The pitch: the strongest of the first four harmonics' peaks near where they should be.
    let (k, peak_bin) = (1..=4)
        .map(|k| {
            let (lo, hi) = ((k as f64 * nominal * 0.97 / hz) as usize, (k as f64 * nominal * 1.03 / hz) as usize);
            let i = (lo..hi.min(spec.len())).max_by(|a, b| spec[*a].total_cmp(&spec[*b])).unwrap_or(lo);
            (k, i)
        })
        .max_by(|a, b| spec[a.1].total_cmp(&spec[b.1]))
        .unwrap();
    let f0 = peak_bin as f64 * hz / k as f64;
    let total = power(0.5 * f0, rate / 2.0);
    let (mut harm, mut widths, mut tonal) = (Vec::new(), Vec::new(), 0.0);
    for k in 1..=24 {
        let c = k as f64 * f0;
        if c > 0.45 * rate {
            break;
        }
        let band = power(c - 0.45 * f0, c + 0.45 * f0);
        tonal += power(c - 0.25 * f0, c + 0.25 * f0);
        harm.push(10.0 * (band / total).max(1e-30).log10());
        if k <= 10 {
            let (lo, hi) = (((c - 0.45 * f0) / hz) as usize, ((c + 0.45 * f0) / hz) as usize);
            let (mut s0, mut s1) = (0.0, 0.0);
            for (i, p) in spec.iter().enumerate().take(hi).skip(lo) {
                s0 += p;
                s1 += p * i as f64 * hz;
            }
            let mu = s1 / s0;
            let var: f64 = (lo..hi.min(spec.len())).map(|i| spec[i] * (i as f64 * hz - mu).powi(2)).sum::<f64>() / s0;
            widths.push(1200.0 * ((mu + var.sqrt()) / mu).log2());
        }
    }
    let strong: Vec<f64> = widths.iter().zip(&harm).filter(|(_, h)| **h > -25.0).map(|(w, _)| *w).collect();
    let spread = median(if strong.len() > 2 { strong[1..].to_vec() } else { strong });
    let points: Vec<(f64, f64)> = harm.iter().take(16).enumerate().map(|(i, h)| (((i + 1) as f64).log2(), *h)).collect();
    let (pn, mx) = (points.len() as f64, points.iter().map(|p| p.0).sum::<f64>() / points.len() as f64);
    let my = points.iter().map(|p| p.1).sum::<f64>() / pn;
    let tilt = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / points.iter().map(|p| (p.0 - mx).powi(2)).sum::<f64>();
    let lin: Vec<f64> = harm.iter().map(|h| 10f64.powf(h / 10.0)).collect();
    let centroid = lin.iter().enumerate().map(|(i, l)| (i + 1) as f64 * l).sum::<f64>() / lin.iter().sum::<f64>();
    // How much the second partial's level wanders: frames of 80 ms every 10 ms.
    let frame = (0.08 * rate) as usize;
    let levels: Vec<f64> = (0..seg.len().saturating_sub(frame))
        .step_by(hop)
        .map(|s| {
            let sp = spectrum(&seg[s..s + frame], 8192);
            let fh = rate / 8192.0;
            sp[((1.7 * f0) / fh) as usize..((2.3 * f0 / fh) as usize).min(sp.len())].iter().sum::<f64>().sqrt()
        })
        .collect();
    let mean = levels.iter().sum::<f64>() / levels.len().max(1) as f64;
    let wander = (levels.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / levels.len().max(1) as f64).sqrt() / mean;
    let (l, r) = (&a.left[start..end], &a.right[start..end]);
    let dot = |x: &[f32], y: &[f32]| x.iter().zip(y).map(|(a, b)| f64::from(*a) * f64::from(*b)).sum::<f64>();
    let corr = dot(l, r) / (dot(l, l) * dot(r, r)).sqrt();
    Measures { f0, harm, tilt, centroid, noise: 10.0 * ((total - tonal).max(1e-30) / total).log10(), spread, wander, corr }
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("a directory with the Sonatina recordings"));
    let find = |stem: String| ["wav", "flac"].iter().map(|e| dir.join(format!("{stem}.{e}"))).find(|p| p.exists());
    type Instrument = (&'static str, u8, [u8; 4], u8, &'static str, &'static str, &'static [&'static str]);
    let instruments: [Instrument; 4] = [
        ("violin", 40, [76, 69, 62, 55], 16, "violin-{}", "1st-violins-sus-{}", &["g3", "c#4", "e4", "a#4", "c#5", "e5", "g5", "a#5"]),
        ("viola", 41, [69, 62, 55, 48], 12, "viola-sus-{}", "violas-sus-{}", &["c3", "d#3", "f#3", "a3", "c4", "d#4", "f#4", "a4"]),
        ("cello", 42, [57, 50, 43, 36], 10, "cello-{}", "celli-sus-{}", &["c2", "d#2", "f#2", "a2", "c3", "d#3", "f#3", "a3", "c4"]),
        ("bass", 43, [43, 38, 33, 28], 8, "bass-sus-{}", "basses-sus-{}", &["f#1", "a1", "c2", "d#2", "f#2", "a2", "c3"]),
    ];
    let edges: Vec<f64> = (0..20).map(|i| 42.0 * 2f64.powf(f64::from(i) / 2.0)).collect();
    for (name, program, tuning, section, solo, sect, notes) in instruments {
        let mut bands: Vec<Vec<f64>> = vec![Vec::new(); edges.len()];
        let mut pairs: [Vec<(Measures, Measures)>; 2] = [Vec::new(), Vec::new()];
        for note in notes {
            let m = midi(note);
            let nominal = 440.0 * 2f64.powf((f64::from(m) - 69.0) / 12.0);
            for (side, (pattern, players)) in [(solo, 1), (sect, section)].into_iter().enumerate() {
                let Some(path) = find(pattern.replace("{}", note)) else {
                    eprintln!("{name} {note}: no recording {}", pattern.replace("{}", note));
                    continue;
                };
                let Some(real) = read(&path) else { continue };
                let (real, ours) = (measure(&real, nominal), measure(&render(program, &tuning, m, players), nominal));
                let k = real.harm.len().min(ours.harm.len());
                let d: Vec<f64> = (0..k).map(|i| real.harm[i] - ours.harm[i]).collect();
                let below: Vec<f64> = (0..k).filter(|i| (*i + 1) as f64 * ours.f0 < 500f64.max(2.5 * ours.f0)).map(|i| d[i]).collect();
                let offset = below.iter().sum::<f64>() / below.len().max(1) as f64;
                for (i, v) in d.iter().enumerate() {
                    let f = (i + 1) as f64 * ours.f0;
                    if let Some(b) = edges.windows(2).position(|w| f >= w[0] && f < w[1]) {
                        bands[b].push(v - offset);
                    }
                }
                pairs[side].push((real, ours));
            }
        }
        let row: Vec<String> = edges.iter().zip(&bands).filter(|(_, v)| v.len() >= 3).map(|(e, v)| format!("{e:5.0}:{:+5.1}", median(v.clone()))).collect();
        println!("{name:7} real - model, dB  {}", row.join(" "));
        for (side, label) in ["solo", "section"].iter().enumerate() {
            let p = &pairs[side];
            if p.is_empty() {
                continue;
            }
            let both = |f: fn(&Measures) -> f64| (median(p.iter().map(|x| f(&x.0)).collect()), median(p.iter().map(|x| f(&x.1)).collect()));
            let rows = [
                ("brightness (harmonic)", both(|m| m.centroid)),
                ("fall (dB/octave)", both(|m| m.tilt)),
                ("noise between harmonics (dB)", both(|m| m.noise)),
                ("pitch spread (cents)", both(|m| m.spread)),
                ("2nd partial's wander", both(|m| m.wander)),
                ("left-right correlation", both(|m| m.corr)),
            ];
            let text: Vec<String> = rows.iter().map(|(n, (r, o))| format!("{n} {r:.2} / {o:.2}")).collect();
            println!("   {label:7} real / model: {}", text.join(" | "));
        }
    }
}
