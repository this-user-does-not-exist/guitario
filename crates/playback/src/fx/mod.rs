//! Effects on a track's bus and on the master bus: original models of a compressor, noise gate,
//! overdrive, distortion, amp, cabinet, graphic and mixer equalizers, chorus, tremolo, delay and
//! reverb, all stereo. A [`Chain`] runs a list of [`guitario_score::Effect`]s in order and takes new
//! settings while it plays without losing its state (a reverb tail survives a knob move).

pub(crate) mod filter;
mod units;

use guitario_score::{Effect, EffectType};
pub(crate) use units::Unit;

/// A list of effects run in order.
pub(crate) struct Chain {
    sr: f32,
    slots: Vec<(EffectType, bool, Unit)>,
    /// The settings the slots were last built from.
    config: Vec<Effect>,
}

impl Chain {
    pub(crate) fn new(sr: f32) -> Chain {
        Chain { sr, slots: Vec::new(), config: Vec::new() }
    }

    /// Takes new settings: a slot of the same type keeps its state and only changes its
    /// parameters; a slot of a new type starts fresh. Nothing happens when nothing changed.
    pub(crate) fn configure(&mut self, effects: &[Effect]) {
        if self.config.as_slice() == effects {
            return;
        }
        self.slots.truncate(effects.len());
        for (i, e) in effects.iter().enumerate() {
            match self.slots.get_mut(i) {
                Some((kind, on, unit)) if *kind == e.kind => {
                    *on = e.on;
                    unit.set(e, self.sr);
                }
                Some(slot) => *slot = (e.kind, e.on, Unit::new(e, self.sr)),
                None => self.slots.push((e.kind, e.on, Unit::new(e, self.sr))),
            }
        }
        self.config = effects.to_vec();
    }

    /// Whether an effect that is on turns a mono source into stereo.
    pub(crate) fn widens(&self) -> bool {
        self.slots.iter().any(|(_, on, unit)| *on && unit.widens())
    }

    /// Whether any effect would change the sound.
    pub(crate) fn active(&self) -> bool {
        self.slots.iter().any(|(_, on, unit)| *on && !unit.is_transparent())
    }

    /// One stereo sample.
    pub(crate) fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (mut l, mut r) = (filter::flush(l), filter::flush(r));
        for (_, on, unit) in &mut self.slots {
            if *on {
                (l, r) = unit.tick(l, r);
            }
        }
        (l, r)
    }

    /// One sample of a mono source (the synth): each effect runs once, through its left
    /// channel, until one widens the sound to stereo. A source must keep to one of `tick` and
    /// `tick_mono`, so every channel's state stays in step.
    pub(crate) fn tick_mono(&mut self, x: f32) -> (f32, f32) {
        let (mut l, mut r) = (filter::flush(x), filter::flush(x));
        let mut mono = true;
        for (_, on, unit) in &mut self.slots {
            if !*on {
                continue;
            }
            if mono && !unit.widens() {
                l = unit.mono(l);
                r = l;
            } else {
                mono = false;
                (l, r) = unit.tick(l, r);
            }
        }
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::{Master, default_chain};

    const SR: f32 = 48_000.0;

    fn sine(freq: f32, i: usize) -> f32 {
        (2.0 * std::f32::consts::PI * freq * i as f32 / SR).sin()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    /// Runs `chain` over `n` samples of `input`, returning the left channel.
    fn run(chain: &mut Chain, n: usize, input: impl Fn(usize) -> f32) -> Vec<f32> {
        (0..n).map(|i| chain.tick(input(i), input(i)).0).collect()
    }

    #[test]
    fn every_effect_is_finite_and_bounded_on_loud_and_hostile_input() {
        for kind in EffectType::CHAIN.into_iter().chain([EffectType::MixerEq]) {
            let presets = kind.presets().iter().filter_map(|(n, _)| Effect::preset(kind, n)).chain([Effect::new(kind)]);
            for e in presets {
                let mut c = Chain::new(SR);
                c.configure(std::slice::from_ref(&e));
                let out = run(&mut c, 24_000, |i| 2.0 * sine(220.0, i) + if i == 500 { f32::NAN } else { 0.0 });
                assert!(out.iter().all(|x| x.is_finite() && x.abs() < 16.0), "{kind:?} {:?}", e.preset_name());
                // The tail dies to silence.
                let tail = run(&mut c, 48_000 * 16, |_| 0.0);
                assert!(rms(&tail[tail.len() - 4800..]) < 1e-3, "{kind:?} rings on");
            }
        }
    }

    #[test]
    fn the_amp_distorts_more_with_gain_and_the_cabinet_rolls_off_the_highs() {
        let thd = |gain: f32| {
            let mut amp = Effect::new(EffectType::Amp);
            amp.set("model", 2.0).unwrap();
            amp.set("gain", gain).unwrap();
            let mut c = Chain::new(SR);
            c.configure(&[amp]);
            let out = run(&mut c, 9600, |i| 0.1 * sine(200.0, i));
            // Energy at the third harmonic against the fundamental.
            let at = |f: f32| {
                let (mut s, mut co) = (0.0, 0.0);
                for (i, x) in out.iter().enumerate().skip(4800) {
                    s += x * sine(f, i);
                    co += x * (2.0 * std::f32::consts::PI * f * i as f32 / SR).cos();
                }
                (s * s + co * co).sqrt()
            };
            at(600.0) / at(200.0)
        };
        assert!(thd(9.0) > 3.0 * thd(0.0), "{} vs {}", thd(9.0), thd(0.0));
        let mut cab = Chain::new(SR);
        cab.configure(&[Effect::new(EffectType::Cabinet)]);
        let high = rms(&run(&mut cab, 9600, |i| sine(9000.0, i))[4800..]);
        let mut cab = Chain::new(SR);
        cab.configure(&[Effect::new(EffectType::Cabinet)]);
        let mid = rms(&run(&mut cab, 9600, |i| sine(1000.0, i))[4800..]);
        assert!(high < 0.25 * mid, "{high} vs {mid}");
    }

    #[test]
    fn delay_repeats_after_its_time_and_reverb_rings_after_an_impulse() {
        let mut delay = Effect::new(EffectType::Delay);
        delay.set("time", 100.0).unwrap();
        delay.set("mix", 100.0).unwrap();
        let mut c = Chain::new(SR);
        c.configure(&[delay]);
        let out = run(&mut c, 9600, |i| if i == 0 { 1.0 } else { 0.0 });
        let peak = out.iter().enumerate().skip(10).max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).map(|(i, _)| i).unwrap();
        assert!((4790..=4810).contains(&peak), "first repeat at {peak}");
        let mut c = Chain::new(SR);
        c.configure(&[Effect::preset(EffectType::Reverb, "Hall").unwrap()]);
        let out = run(&mut c, 48_000, |i| if i == 0 { 1.0 } else { 0.0 });
        assert!(rms(&out[24_000..30_000]) > 1e-4, "a hall still rings after half a second");
    }

    #[test]
    fn the_compressor_evens_out_and_the_gate_closes() {
        let mut comp = Effect::preset(EffectType::Compressor, "Squash").unwrap();
        comp.set("gain", 0.0).unwrap();
        let level = |amp: f32| {
            let mut c = Chain::new(SR);
            c.configure(std::slice::from_ref(&comp));
            rms(&run(&mut c, 24_000, |i| amp * sine(200.0, i))[12_000..])
        };
        let (quiet, loud) = (level(0.05), level(0.8));
        assert!(loud / quiet < 0.5 * (0.8 / 0.05), "{quiet} {loud}");
        let mut c = Chain::new(SR);
        c.configure(&[Effect::preset(EffectType::NoiseGate, "Tight").unwrap()]);
        let hiss = run(&mut c, 24_000, |i| 0.001 * sine(3000.0, i));
        assert!(rms(&hiss[12_000..]) < 1e-4);
        let note = run(&mut c, 24_000, |i| 0.5 * sine(200.0, i));
        assert!(rms(&note[12_000..]) > 0.3);
    }

    #[test]
    fn equalizers_boost_their_bands_and_a_flat_one_is_transparent() {
        let mut eq = guitario_score::mixer_eq();
        let mut c = Chain::new(SR);
        c.configure(std::slice::from_ref(&eq));
        assert!(!c.active());
        eq.set("mid", 12.0).unwrap();
        c.configure(std::slice::from_ref(&eq));
        assert!(c.active());
        let at1k = rms(&run(&mut c, 9600, |i| sine(1000.0, i))[4800..]);
        assert!((at1k / std::f32::consts::FRAC_1_SQRT_2 - 3.98).abs() < 0.2, "{at1k}");
        let mut g = Effect::new(EffectType::Equalizer);
        g.set("b125", -12.0).unwrap();
        c.configure(&[g]);
        let at125 = rms(&run(&mut c, 9600, |i| sine(125.0, i))[4800..]);
        assert!(at125 < 0.3, "{at125}");
    }

    #[test]
    fn a_mono_source_sounds_the_same_through_the_mono_path() {
        let chain = || {
            let mut c = Chain::new(SR);
            c.configure(&[
                Effect::new(EffectType::Compressor),
                Effect::new(EffectType::Overdrive),
                Effect::preset(EffectType::Amp, "Lead Stack").unwrap(),
                Effect::new(EffectType::Cabinet),
                Effect::new(EffectType::Delay),
                Effect::new(EffectType::Tremolo),
                Effect::new(EffectType::Reverb),
            ]);
            c
        };
        let (mut mono, mut stereo) = (chain(), chain());
        for i in 0..9600 {
            let x = 0.3 * sine(110.0, i);
            let a = mono.tick_mono(x);
            let b = stereo.tick(x, x);
            assert!((a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4, "{i}: {a:?} vs {b:?}");
        }
        assert!((super::units::fast_tanh(0.5) - 0.5f32.tanh()).abs() < 0.02 && super::units::fast_tanh(1e9) == 1.0);
    }

    #[test]
    fn new_settings_keep_the_state_of_an_effect_of_the_same_type() {
        let mut c = Chain::new(SR);
        let reverb = Effect::preset(EffectType::Reverb, "Hall").unwrap();
        c.configure(std::slice::from_ref(&reverb));
        run(&mut c, 4800, |i| sine(440.0, i));
        let mut wetter = reverb.clone();
        wetter.set("mix", 60.0).unwrap();
        c.configure(&[wetter]);
        assert!(rms(&run(&mut c, 4800, |_| 0.0)) > 1e-3, "the tail survives a knob move");
        c.configure(&[Effect::new(EffectType::Delay)]);
        assert!(rms(&run(&mut c, 100, |_| 0.0)) < 1e-6, "a new type starts fresh");
        let mut off = Effect::new(EffectType::Distortion);
        off.on = false;
        c.configure(&[off]);
        assert!(!c.active());
        c.configure(&default_chain(30));
        assert!(c.active());
        c.configure(&Master::default().slots().map(Clone::clone));
        assert!(!c.active(), "the master's slots are off by default");
        c.configure(&[]);
        assert_eq!(c.tick(0.25, -0.5), (0.25, -0.5));
    }
}
