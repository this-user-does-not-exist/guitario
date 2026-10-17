//! The sampled sound bank in the app: instruments decode on a worker thread whenever the score
//! needs notes the bank has not decoded, so pressing Play never freezes the window. A Play pressed
//! while they load starts when they are ready.

impl crate::GuitarioApp {
    /// Keeps the sampled instruments decoded for the score as it is, on a worker thread, and starts
    /// a Play that was waiting on them.
    pub(crate) fn sounds_tick(&mut self, ctx: &egui::Context) {
        let Some(bank) = self.sounds.clone() else { return };
        if let Some(done) = self.preload.take_if(|h| h.is_finished()) {
            let notes = done.join().unwrap_or_else(|_| vec!["loading the sound bank stopped".into()]);
            if let Some(first) = notes.first() {
                self.status = format!("Sound bank: {first}");
                self.status_is_error = true;
            }
            if self.play_when_ready {
                self.play_when_ready = false;
                self.toggle_play();
            }
        }
        if self.preload.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }
        let score = std::sync::Arc::clone(self.session.score());
        if self.preloaded.as_ref().is_some_and(|p| std::sync::Arc::ptr_eq(p, &score)) {
            return;
        }
        self.preloaded = Some(std::sync::Arc::clone(&score));
        if bank.ready(&score) {
            return;
        }
        let (b, sc) = (std::sync::Arc::clone(&bank), std::sync::Arc::clone(&score));
        match std::thread::Builder::new().name("sounds".into()).spawn(move || b.preload(&sc)) {
            Ok(h) => {
                self.preload = Some(h);
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
            // No thread to spare: decode here rather than never play.
            Err(_) => {
                bank.preload(&score);
                if self.play_when_ready {
                    self.play_when_ready = false;
                    self.toggle_play();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use guitario_engine::Session;
    use guitario_playback::SoundBank;
    use serde_json::json;

    fn bank(tag: &str) -> (std::path::PathBuf, SoundBank) {
        let dir = std::env::temp_dir().join(format!("gt-sounds-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let spec = hound::WavSpec { channels: 1, sample_rate: 8000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(dir.join("a.wav"), spec).unwrap();
        for n in 0..8000 {
            w.write_sample(((n as f32 * 0.1).sin() * 20_000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        std::fs::write(dir.join("k.sfz"), "<region> sample=a.wav lokey=0 hikey=127 pitch_keycenter=64").unwrap();
        std::fs::write(dir.join("bank.json"), r#"{"guitar_clean":"k.sfz"}"#).unwrap();
        let bank = SoundBank::open(&dir).unwrap();
        (dir, bank)
    }

    /// Frames until the worker is done.
    fn settle(app: &mut crate::GuitarioApp, ctx: &egui::Context) {
        for _ in 0..500 {
            app.sounds_tick(ctx);
            if app.preload.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the sound bank never finished loading");
    }

    #[test]
    fn instruments_load_in_the_background_and_play_waits_for_them() {
        let (dir, bank) = bank("bg");
        let mut s = Session::new();
        s.run("track.setProgram", &json!({"program": 25})).unwrap();
        s.run("note.setFret", &json!({"fret": 3})).unwrap();
        let mut app = crate::GuitarioApp::new(s, None).with_sounds(Some(bank));
        let ctx = egui::Context::default();
        // The score as opened is loaded without anyone pressing Play.
        app.sounds_tick(&ctx);
        settle(&mut app, &ctx);
        let bank = std::sync::Arc::clone(app.sounds.as_ref().unwrap());
        assert!(bank.ready(app.session.score()));
        // A new pitch: Play waits, says why, and Space again cancels; the load carries on.
        app.run("note.setFret", json!({"fret": 9}));
        assert!(!bank.ready(app.session.score()));
        app.toggle_play();
        assert!(app.play_when_ready && !app.transport.playing && app.status.contains("Loading sounds"));
        app.toggle_play();
        assert!(!app.play_when_ready);
        app.sounds_tick(&ctx);
        settle(&mut app, &ctx);
        assert!(bank.ready(app.session.score()) && !app.transport.playing);
        // Nothing new to load: no worker starts.
        app.sounds_tick(&ctx);
        assert!(app.preload.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
