//! The tails of long samples, read from disk when a voice gets near them.
//!
//! A library's guitar notes ring for 15 seconds or more, and held whole a guitar takes about
//! 450 MB. Only the first [`HEAD_SECS`] of a long sample stay in memory (`Sample::left`/`right`);
//! the rest is a [`Stream`]. A voice asks for it ahead of need: on the audio thread without ever
//! waiting (the streamer thread decodes it), offline (export, tests) by loading it right there.
//! Tails stay in a cache of at most [`cache_cap`] bytes, the least recently used going first, but
//! never one a voice is playing; a tail whose instrument is gone is dropped.

use super::sample::{Sample, SfzError};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

/// Seconds at the start of a long sample that stay in memory.
pub const HEAD_SECS: f64 = 1.0;
/// Samples up to this long are held whole: streaming them would save little.
pub const STREAM_MIN_SECS: f64 = 2.0;
/// How far ahead of the head's end a voice asks for the tail, in seconds of the sample.
pub const LEAD_SECS: f64 = 0.6;
/// Tails unused for this long are dropped even under the cap, in milliseconds.
const IDLE_MS: u64 = 60_000;
/// The default size of the tail cache.
const DEFAULT_CACHE_MB: usize = 256;
/// Requests waiting for the streamer; when full, a voice asks again a little later.
const QUEUE: usize = 512;

/// Decodes the whole sample again (from its file), for its tail.
pub type Loader = Box<dyn Fn() -> Result<Sample, SfzError> + Send + Sync>;

/// The frames of a sample after its head.
#[derive(Debug)]
pub struct Tail {
    left: Vec<i16>,
    right: Option<Vec<i16>>,
}

impl Tail {
    /// Frame `i` of the tail, both channels, -1..1; silence past its end.
    #[inline]
    pub fn get(&self, i: usize) -> (f32, f32) {
        let l = self.left.get(i).map_or(0.0, |x| f32::from(*x) / 32_768.0);
        let r = self.right.as_ref().map_or(l, |r| r.get(i).map_or(0.0, |x| f32::from(*x) / 32_768.0));
        (l, r)
    }

    pub fn bytes(&self) -> usize {
        (self.left.len() + self.right.as_ref().map_or(0, Vec::len)) * std::mem::size_of::<i16>()
    }
}

/// The part of a sample that is read from disk when needed.
pub struct Stream {
    /// Frames in memory before the tail.
    head: usize,
    /// Frames in the tail.
    frames: usize,
    load: Loader,
    slot: Mutex<Option<Arc<Tail>>>,
    pub(super) queued: AtomicBool,
    failed: AtomicBool,
    used: AtomicU64,
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stream").field("head", &self.head).field("frames", &self.frames).finish_non_exhaustive()
    }
}

/// Milliseconds since the first call; there is no clock on wasm, where nothing streams.
fn now_ms() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        u64::try_from(START.get_or_init(std::time::Instant::now).elapsed().as_millis()).unwrap_or(u64::MAX)
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

impl Stream {
    pub fn new(head: usize, frames: usize, load: Loader) -> Stream {
        Stream { head, frames, load, slot: Mutex::new(None), queued: AtomicBool::new(false), failed: AtomicBool::new(false), used: AtomicU64::new(0) }
    }

    /// Frames in the tail.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// The tail if it is in memory, without waiting: for the audio thread.
    pub fn try_tail(&self) -> Option<Arc<Tail>> {
        let tail = self.slot.try_lock().ok()?.clone();
        if tail.is_some() {
            self.used.store(now_ms(), Ordering::Relaxed);
        }
        tail
    }

    /// Asks the streamer for the tail without waiting. Nothing happens if it is already asked
    /// for or cannot be read.
    pub fn request(self: &Arc<Self>) {
        if self.failed.load(Ordering::Relaxed) || self.queued.swap(true, Ordering::AcqRel) {
            return;
        }
        let sent = streamer().is_some_and(|q| q.try_send(Arc::clone(self)).is_ok());
        if !sent {
            self.queued.store(false, Ordering::Release);
        }
    }

    /// Whether reading the tail failed (the file is gone or changed), so asking again is pointless.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// The tail, decoded here if it is not in memory: for offline rendering and the streamer.
    pub fn tail(self: &Arc<Self>) -> Option<Arc<Tail>> {
        let (tail, loaded) = {
            let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
            match slot.as_ref() {
                Some(t) => (Some(Arc::clone(t)), false),
                None if self.failed() => (None, false),
                None => match self.decode() {
                    Some(t) => {
                        let t = Arc::new(t);
                        *slot = Some(Arc::clone(&t));
                        (Some(t), true)
                    }
                    None => {
                        self.failed.store(true, Ordering::Relaxed);
                        (None, false)
                    }
                },
            }
        };
        self.used.store(now_ms(), Ordering::Relaxed);
        if loaded {
            let mut cache = cache().lock().unwrap_or_else(PoisonError::into_inner);
            cache.add(Arc::clone(self));
            cache.trim(cache_cap(), now_ms());
        }
        tail
    }

    /// The whole sample decoded again, cut after its head.
    fn decode(&self) -> Option<Tail> {
        let mut whole = (self.load)().ok()?;
        let head = self.head.min(whole.left.len());
        let left = whole.left.split_off(head);
        let right = whole.right.as_mut().map(|r| r.split_off(head.min(r.len())));
        Some(Tail { left, right })
    }

    /// Drops the tail if no voice holds it; returns the bytes freed.
    fn evict(&self) -> usize {
        let Ok(mut slot) = self.slot.try_lock() else { return 0 };
        // Voices take their copy under this lock, so a count of one cannot rise while it is held.
        match slot.as_ref() {
            Some(t) if Arc::strong_count(t) == 1 => slot.take().map_or(0, |t| t.bytes()),
            _ => 0,
        }
    }

    fn resident(&self) -> usize {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner).as_ref().map_or(0, |t| t.bytes())
    }
}

/// The tails in memory, for the cap.
#[derive(Default)]
struct Cache {
    entries: Vec<Arc<Stream>>,
}

impl Cache {
    fn add(&mut self, s: Arc<Stream>) {
        if !self.entries.iter().any(|e| Arc::ptr_eq(e, &s)) {
            self.entries.push(s);
        }
    }

    fn bytes(&self) -> usize {
        self.entries.iter().map(|e| e.resident()).sum()
    }

    /// Drops what is not needed: tails whose instrument is gone, tails idle for a minute, then
    /// the least recently used until the cache fits `cap`. Tails a voice is playing stay.
    fn trim(&mut self, cap: usize, now: u64) {
        // Only the cache holds it: no instrument or voice can play it again.
        self.entries.retain(|e| Arc::strong_count(e) > 1);
        for e in &self.entries {
            if now.saturating_sub(e.used.load(Ordering::Relaxed)) > IDLE_MS {
                e.evict();
            }
        }
        let mut bytes = self.bytes();
        if bytes > cap {
            self.entries.sort_by_key(|e| e.used.load(Ordering::Relaxed));
            for e in &self.entries {
                if bytes <= cap {
                    break;
                }
                bytes = bytes.saturating_sub(e.evict());
            }
        }
        self.entries.retain(|e| e.resident() > 0);
    }
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}

/// The most the tail cache holds while nothing plays the tails: `GUITARIO_STREAM_CACHE_MB`, or
/// 256 MB.
pub fn cache_cap() -> usize {
    static CAP: OnceLock<usize> = OnceLock::new();
    *CAP.get_or_init(|| {
        std::env::var("GUITARIO_STREAM_CACHE_MB").ok().and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(DEFAULT_CACHE_MB).saturating_mul(1 << 20)
    })
}

/// Bytes of tails in memory now.
pub fn resident_bytes() -> usize {
    cache().lock().unwrap_or_else(PoisonError::into_inner).bytes()
}

static UNDERRUNS: AtomicUsize = AtomicUsize::new(0);

/// How many times a voice reached a tail that was not in memory yet (and played silence until it
/// was): a sign the disk or the streamer is too slow.
pub fn underruns() -> usize {
    UNDERRUNS.load(Ordering::Relaxed)
}

pub(crate) fn note_underrun() {
    UNDERRUNS.fetch_add(1, Ordering::Relaxed);
}

/// Starts the streamer thread if it is not running (the first live player does). Without
/// threads (wasm) nothing streams and offline loading still works.
pub fn start() {
    let _ = streamer();
}

fn streamer() -> Option<&'static SyncSender<Arc<Stream>>> {
    static QUEUE_TX: OnceLock<Option<SyncSender<Arc<Stream>>>> = OnceLock::new();
    QUEUE_TX
        .get_or_init(|| {
            let (tx, rx) = sync_channel(QUEUE);
            std::thread::Builder::new().name("sfz-stream".into()).spawn(move || serve(&rx)).ok().map(|_| tx)
        })
        .as_ref()
}

fn serve(rx: &Receiver<Arc<Stream>>) {
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(s) => {
                let _ = s.tail();
                s.queued.store(false, Ordering::Release);
            }
            Err(RecvTimeoutError::Timeout) => cache().lock().unwrap_or_else(PoisonError::into_inner).trim(cache_cap(), now_ms()),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(head: usize, total: usize) -> (Arc<Stream>, Arc<AtomicUsize>) {
        let loads = Arc::new(AtomicUsize::new(0));
        let n = Arc::clone(&loads);
        let s = Stream::new(
            head,
            total - head,
            Box::new(move || {
                n.fetch_add(1, Ordering::Relaxed);
                let x: Vec<f32> = (0..total).map(|i| (i % 1000) as f32 / 1000.0).collect();
                Ok(Sample::from_f32(1000, &x, None))
            }),
        );
        (Arc::new(s), loads)
    }

    #[test]
    fn the_tail_is_what_follows_the_head() {
        let (s, loads) = stream(100, 400);
        let t = s.tail().unwrap();
        assert_eq!(t.left.len(), 300);
        assert!((t.get(0).0 - 0.1).abs() < 1e-3 && t.get(300) == (0.0, 0.0));
        assert!(Arc::ptr_eq(&t, &s.tail().unwrap()), "decoded once");
        assert_eq!(loads.load(Ordering::Relaxed), 1);
        assert!(s.try_tail().is_some());
        let broken = Arc::new(Stream::new(1, 1, Box::new(|| Err(SfzError::Io("gone".into())))));
        assert!(broken.tail().is_none() && broken.failed());
        broken.request();
        assert!(!broken.queued.load(Ordering::Relaxed), "a failed tail is not asked for again");
    }

    #[test]
    fn the_cache_keeps_what_voices_play_and_drops_the_oldest() {
        let mut cache = Cache::default();
        let streams: Vec<_> = (0..4).map(|_| stream(10, 1010).0).collect();
        for (n, s) in streams.iter().enumerate() {
            *s.slot.lock().unwrap() = s.decode().map(Arc::new);
            s.used.store(n as u64 * 10, Ordering::Relaxed);
            cache.add(Arc::clone(s));
        }
        let one = streams[0].resident();
        assert_eq!(cache.bytes(), 4 * one);
        // A voice plays the oldest tail: the next oldest go instead.
        let playing = streams[0].try_tail().unwrap();
        streams[0].used.store(0, Ordering::Relaxed);
        cache.trim(2 * one, 40);
        assert!(streams[0].resident() > 0 && streams[1].resident() == 0 && streams[2].resident() == 0 && streams[3].resident() > 0);
        drop(playing);
        // Idle for a minute: gone even under the cap.
        cache.trim(usize::MAX, 40 + IDLE_MS + 1);
        assert_eq!(cache.bytes(), 0);
        assert!(cache.entries.is_empty());
        // A tail whose instrument is gone is dropped.
        let (s, _) = stream(10, 1010);
        *s.slot.lock().unwrap() = s.decode().map(Arc::new);
        let mut cache = Cache::default();
        cache.add(Arc::clone(&s));
        drop(s);
        cache.trim(usize::MAX, 0);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn the_streamer_fills_a_request() {
        let (s, _) = stream(10, 5000);
        s.request();
        let t0 = std::time::Instant::now();
        while s.try_tail().is_none() && t0.elapsed().as_secs() < 10 {
            std::thread::yield_now();
        }
        assert!(s.try_tail().is_some(), "the streamer thread decoded it");
    }
}
