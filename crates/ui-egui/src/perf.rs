//! Performance budgets (gap G9). `cargo xtask perf` runs [`tests::budgets`] in release: a long
//! band score (demo 10 tiled to 400 bars, its guitars doubled to six tracks) goes through every
//! stage a user waits on, each timed as the median of several runs and checked against
//! `perf/budgets.toml`. Debug builds and the normal test run skip it (it is `#[ignore]`).

#[cfg(test)]
mod tests {
    use crate::layout::{LayoutMode, Metrics, Part, layout_parts};
    use guitario_engine::Session;
    use guitario_score::Score;
    use std::collections::BTreeMap;
    use std::time::Instant;

    const BARS: usize = 400;

    /// Demo 10 tiled to `BARS` bars, with its two guitar tracks doubled: six tracks.
    fn long_score() -> Score {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/10-ashes-to-distortion.gt");
        let demo = guitario_format::open(&root).unwrap();
        let mut s = demo.clone();
        let n = demo.headers.len();
        s.headers = (0..BARS).map(|i| demo.headers[i % n].clone()).collect();
        // Repeats and endings would replay the piece many times over; keep the bars plain.
        for h in &mut s.headers {
            h.repeat_open = false;
            h.repeat_close = 0;
            h.endings = 0;
            h.targets.clear();
            h.jump = None;
        }
        for (t, dt) in s.tracks.iter_mut().zip(&demo.tracks) {
            t.measures = (0..BARS).map(|i| dt.measures[i % n].clone()).collect();
        }
        let extra: Vec<_> = s.tracks.iter().filter(|t| !t.is_drums() && t.string_count() == 6).cloned().collect();
        for mut t in extra.into_iter().take(2) {
            t.name = format!("{} 2", t.name);
            s.tracks.push(t);
        }
        s.validate().unwrap();
        s
    }

    /// Milliseconds per stage from `perf/budgets.toml` (`name = ms` lines, `#` comments).
    fn budgets() -> BTreeMap<String, f64> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../perf/budgets.toml");
        let text = std::fs::read_to_string(path).unwrap();
        text.lines()
            .filter_map(|l| {
                let l = l.split('#').next()?.trim();
                let (k, v) = l.split_once('=')?;
                Some((k.trim().to_string(), v.trim().parse().ok()?))
            })
            .collect()
    }

    /// The median of `runs` timings of `f`, in milliseconds.
    fn time(runs: usize, mut f: impl FnMut()) -> f64 {
        let mut ms: Vec<f64> = (0..runs)
            .map(|_| {
                let t = Instant::now();
                f();
                t.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        ms.sort_by(f64::total_cmp);
        ms[ms.len() / 2]
    }

    fn parts(score: &Score, multitrack: bool) -> (Vec<Part<'_>>, Metrics) {
        let indices: Vec<usize> = if multitrack { (0..score.tracks.len()).collect() } else { vec![0] };
        let parts: Vec<Part> = indices
            .iter()
            .enumerate()
            .map(|(k, &ti)| {
                let track = &score.tracks[ti];
                let m = Metrics::new(1.25, true, track.string_count())
                    .with_page(&score.page)
                    .with_annotations(track)
                    .with_sections(score)
                    .with_mode(LayoutMode::Page, 1200.0);
                Part { index: ti, track, metrics: if k == 0 { m } else { m.below() } }
            })
            .collect();
        let page = parts[0].metrics;
        (parts, page)
    }

    /// One app frame of the page view, averaged over frames after the first.
    fn frame_ms(score: &Score, multitrack: bool, bar: usize) -> f64 {
        let mut s = Session::with_score(score.clone());
        s.run("cursor.set", &serde_json::json!({"measure": bar})).unwrap();
        let mut app = crate::GuitarioApp::new(s, None);
        app.view.multitrack = multitrack;
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(move |ui| app.show(ui));
        harness.run_steps(4);
        time(9, || {
            harness.run_steps(1);
            // What the GPU backend does with the frame's shapes before drawing them.
            let shapes = harness.output().shapes.clone();
            drop(harness.ctx.tessellate(shapes, 2.0));
        })
    }

    #[test]
    #[ignore = "timed in release by `cargo xtask perf`"]
    fn budgets_hold() {
        let budget = budgets();
        let score = long_score();
        let mut results: Vec<(&str, f64)> = Vec::new();

        let bytes = guitario_format::to_bytes(&score).unwrap();
        results.push(("open_gt", time(5, || drop(guitario_format::from_bytes(&bytes).unwrap()))));
        results.push(("save_gt", time(5, || drop(guitario_format::to_bytes(&score).unwrap()))));
        let gp = guitario_gp::export_gp(&score).unwrap().bytes;
        results.push(("import_gp", time(5, || drop(guitario_gp::import(&gp).unwrap()))));

        let (one, page) = parts(&score, false);
        results.push(("layout_one_track", time(9, || drop(layout_parts(&score, &one, &page)))));
        let (all, page) = parts(&score, true);
        results.push(("layout_multitrack", time(9, || drop(layout_parts(&score, &all, &page)))));
        results.push(("frame_page_start", frame_ms(&score, false, 0)));
        results.push(("frame_page_middle", frame_ms(&score, false, BARS / 2)));
        results.push(("frame_multitrack", frame_ms(&score, true, BARS / 2)));

        results.push(("timeline", time(9, || drop(guitario_score::timeline(&score)))));
        results.push(("player_new", time(5, || drop(guitario_playback::Player::new(&score, 48_000, guitario_playback::Options::default())))));
        // Synth rendering, as milliseconds of work per second of sound (1000 would be real time).
        let mut player = guitario_playback::Player::new(&score, 48_000, guitario_playback::Options::default());
        player.play_from(10.0);
        let mut buf = vec![0f32; 512 * 2];
        let render = time(5, || {
            for _ in 0..(48_000 / 512) {
                player.render(&mut buf, 2);
            }
        });
        results.push(("render_per_second", render));
        // The modelled instruments: the same score with its pitched tracks as a grand piano, a
        // string orchestra (each note by the section for its range, eight to sixteen players, a
        // chord divided among them), a violin and a cello.
        let mut modelled = score.clone();
        for (t, program) in modelled.tracks.iter_mut().filter(|t| !t.is_drums()).zip([0u8, 48, 40, 42].into_iter().cycle()) {
            t.program = program;
        }
        let mut player = guitario_playback::Player::new(&modelled, 48_000, guitario_playback::Options::default());
        player.play_from(10.0);
        let render = time(5, || {
            for _ in 0..(48_000 / 512) {
                player.render(&mut buf, 2);
            }
        });
        results.push(("render_modelled_per_second", render));

        // With the sampled sound bank installed: decoding runs on a worker thread when a score is
        // opened or gains new notes; the window only checks whether it is done (after each edit) and,
        // on Play, hands the decoded instruments to the player.
        if let Some(bank) = guitario_playback::SoundBank::open_default() {
            let t = Instant::now();
            let notes = bank.preload(&score);
            results.push(("sounds_preload", t.elapsed().as_secs_f64() * 1000.0));
            results.push(("sounds_memory_mb", bank.memory() as f64 / 1_048_576.0));
            assert!(notes.is_empty(), "{notes:?}");
            results.push(("sounds_ready_check", time(9, || assert!(bank.ready(&score)))));
            let mut p = guitario_playback::Player::new(&score, 48_000, guitario_playback::Options::default());
            results.push(("sounds_play", time(5, || drop(bank.apply(&mut p, &score)))));
            // The audio thread's share: the tails of long samples are read by the streamer ahead
            // of need, which faster than real time it cannot be, so an offline pass over the
            // same seconds reads them first.
            let mut warm = guitario_playback::Player::new(&score, 48_000, guitario_playback::Options::default());
            bank.apply(&mut warm, &score);
            warm.play_from(10.0);
            for _ in 0..(6 * 48_000 / 512) {
                warm.render(&mut buf, 2);
            }
            p.set_live(true);
            p.play_from(10.0);
            let late = guitario_playback::sfz::stream::underruns();
            let sampled = time(5, || {
                for _ in 0..(48_000 / 512) {
                    p.render(&mut buf, 2);
                }
            });
            results.push(("render_sampled_per_second", sampled));
            results.push(("sounds_tails_mb", guitario_playback::sfz::stream::resident_bytes() as f64 / 1_048_576.0));
            results.push(("sounds_late_tails", (guitario_playback::sfz::stream::underruns() - late) as f64));
        }

        let mut failed = Vec::new();
        println!("{:<22} {:>10} {:>10}", "stage", "ms", "budget");
        for (name, ms) in &results {
            let b = budget.get(*name).copied();
            println!("{name:<22} {ms:>10.2} {:>10}", b.map_or("-".into(), |b| format!("{b:.0}")));
            if b.is_some_and(|b| *ms > b) {
                failed.push(format!("{name} took {ms:.1} ms, over its {:.0} ms budget", b.unwrap_or(0.0)));
            }
        }
        if std::env::var_os("GUITARIO_PERF_REPORT").is_none() {
            assert!(failed.is_empty(), "{}", failed.join("\n"));
        }
    }
}
