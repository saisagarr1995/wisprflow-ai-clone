//! Streaming finalization: while the user is still holding the hotkey,
//! finished chunks of the utterance are decoded with the *final* engine so
//! that on release only the last few seconds remain to transcribe. Final
//! latency stays flat no matter how long the dictation ran, instead of
//! growing with utterance length (design doc §2 stage 4).
//!
//! Chunks are cut at the quietest moment near the boundary so we never split
//! a word, and each chunk sees the tail of the already-committed text as its
//! decoder prompt for continuity across cuts.

use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use openflow_audio::{RollingBuffer, TARGET_SAMPLE_RATE};
use openflow_stt::{SttEngine, TranscribeOpts};

/// The final STT engine, shared between the orchestrator (tail decode on
/// release) and the finalizer worker (chunk decodes while recording). The
/// lock is only held for the duration of a single decode.
pub type SharedStt = Arc<Mutex<Box<dyn SttEngine>>>;

/// Text already decoded with the final engine, plus the absolute sample
/// index the release-time tail decode must start from.
#[derive(Debug, Clone, Default)]
pub struct Committed {
    pub text: String,
    pub until: u64,
}

const SR: u64 = TARGET_SAMPLE_RATE as u64;
/// Commit a chunk once at least this much undecoded audio is pending.
const COMMIT_AFTER: u64 = 5 * SR;
/// Never commit the freshest audio — the user is mid-word there.
const HOLDBACK: u64 = SR * 12 / 10;
/// The cut lands on the quietest 80 ms window inside the last second of the
/// candidate chunk, so chunks end between words rather than through them.
const CUT_SEARCH: u64 = SR;
/// Chunk decodes share the machine with live captions and the foreground
/// app; the release-time tail decode is the only pass that gets all cores.
const CHUNK_THREADS: i32 = 4;

struct Session {
    history: Arc<Mutex<RollingBuffer>>,
    opts: TranscribeOpts,
    committed: Committed,
}

enum Cmd {
    Start {
        history: Arc<Mutex<RollingBuffer>>,
        from: u64,
        opts: TranscribeOpts,
    },
    Finish(mpsc::Sender<Committed>),
    Shutdown,
}

/// Owns a dedicated thread that shares the final engine with the
/// orchestrator. Start it at key-down; `finish` returns everything committed
/// so far (waiting out any in-flight chunk decode first).
pub struct StreamFinalizer {
    tx: mpsc::Sender<Cmd>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl StreamFinalizer {
    pub fn spawn(stt: SharedStt, interval: Duration) -> Self {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let thread = std::thread::Builder::new()
            .name("openflow-finalizer".into())
            .spawn(move || {
                let mut session: Option<Session> = None;
                loop {
                    let cmd = if session.is_none() {
                        rx.recv().ok()
                    } else {
                        match rx.recv_timeout(interval) {
                            Ok(c) => Some(c),
                            Err(mpsc::RecvTimeoutError::Timeout) => None,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    };
                    match cmd {
                        Some(Cmd::Start {
                            history,
                            from,
                            opts,
                        }) => {
                            session = Some(Session {
                                history,
                                opts,
                                committed: Committed {
                                    text: String::new(),
                                    until: from,
                                },
                            });
                            continue;
                        }
                        Some(Cmd::Finish(reply)) => {
                            let out = session.take().map(|s| s.committed).unwrap_or_default();
                            let _ = reply.send(out);
                            continue;
                        }
                        Some(Cmd::Shutdown) => break,
                        None => {} // timeout tick — maybe commit below
                    }
                    let Some(s) = session.as_mut() else { continue };
                    let (chunk, cut) = {
                        let h = s.history.lock().expect("history lock");
                        let head = h.head();
                        if head.saturating_sub(s.committed.until) < COMMIT_AFTER + HOLDBACK {
                            continue;
                        }
                        let end = head - HOLDBACK;
                        let search = h.snapshot(end - CUT_SEARCH, end);
                        let cut = end - CUT_SEARCH + quietest_offset(&search) as u64;
                        (h.snapshot(s.committed.until, cut), cut)
                    };
                    if chunk.is_empty() {
                        s.committed.until = cut;
                        continue;
                    }
                    // Continuity: after the first chunk the decoder's prompt
                    // is the tail of what it already wrote, not the bias text.
                    let prompt = if s.committed.text.is_empty() {
                        s.opts.initial_prompt.clone()
                    } else {
                        Some(tail_chars(&s.committed.text, 120))
                    };
                    let opts = TranscribeOpts {
                        language: s.opts.language.clone(),
                        initial_prompt: prompt,
                        threads: Some(CHUNK_THREADS),
                    };
                    let result = stt.lock().expect("stt lock").transcribe(&chunk, &opts);
                    match result {
                        Ok(t) => {
                            let text = t.text.trim();
                            if !text.is_empty() {
                                if !s.committed.text.is_empty() {
                                    s.committed.text.push(' ');
                                }
                                s.committed.text.push_str(text);
                            }
                            s.committed.until = cut;
                        }
                        // Keep `until` so the audio is retried next tick (or
                        // swept up by the release-time tail decode).
                        Err(e) => tracing::debug!(error = %e, "chunk finalize failed; will retry"),
                    }
                }
            })
            .expect("spawn finalizer worker");
        Self {
            tx,
            thread: Some(thread),
        }
    }

    pub fn start(&self, history: Arc<Mutex<RollingBuffer>>, from: u64, opts: TranscribeOpts) {
        let _ = self.tx.send(Cmd::Start {
            history,
            from,
            opts,
        });
    }

    /// Blocks until any in-flight chunk decode lands, then returns the
    /// committed text and where the tail decode should pick up.
    pub fn finish(&self) -> Committed {
        let (reply_tx, reply_rx) = mpsc::channel();
        if self.tx.send(Cmd::Finish(reply_tx)).is_err() {
            return Committed::default();
        }
        reply_rx.recv().unwrap_or_default()
    }
}

impl Drop for StreamFinalizer {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Offset of the centre of the quietest 80 ms window in `region`.
fn quietest_offset(region: &[f32]) -> usize {
    const WIN: usize = 1280; // 80 ms at 16 kHz
    if region.len() <= WIN {
        return region.len();
    }
    let mut best_energy = f32::MAX;
    let mut best_at = region.len();
    let mut i = 0;
    while i + WIN <= region.len() {
        let energy: f32 = region[i..i + WIN].iter().map(|s| s.abs()).sum();
        if energy < best_energy {
            best_energy = energy;
            best_at = i + WIN / 2;
        }
        i += WIN / 4;
    }
    best_at
}

fn tail_chars(text: &str, n: usize) -> String {
    let start = text
        .char_indices()
        .rev()
        .nth(n.saturating_sub(1))
        .map_or(0, |(i, _)| i);
    text[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openflow_stt::FakeStt;

    #[test]
    fn commits_chunks_while_recording_and_finishes_with_text() {
        // 8 s of loud "speech": enough pending audio for exactly one commit.
        let history = Arc::new(Mutex::new(RollingBuffer::new(30 * SR as usize)));
        history.lock().unwrap().push(&vec![0.4; 8 * SR as usize]);

        let stt: SharedStt = Arc::new(Mutex::new(Box::new(FakeStt::with_responses(["chunk one"]))));
        let worker = StreamFinalizer::spawn(stt, Duration::from_millis(20));
        worker.start(history.clone(), 0, TranscribeOpts::default());
        std::thread::sleep(Duration::from_millis(300));

        let committed = worker.finish();
        assert_eq!(committed.text, "chunk one");
        // The cut is behind the head by at least the holdback.
        assert!(committed.until >= COMMIT_AFTER);
        assert!(committed.until <= 8 * SR - HOLDBACK);
        drop(worker); // must join, not hang
    }

    #[test]
    fn finish_without_commits_returns_empty() {
        let history = Arc::new(Mutex::new(RollingBuffer::new(30 * SR as usize)));
        history.lock().unwrap().push(&vec![0.4; SR as usize]); // 1 s: below threshold

        let stt: SharedStt = Arc::new(Mutex::new(Box::new(FakeStt::with_responses(["nope"]))));
        let worker = StreamFinalizer::spawn(stt, Duration::from_millis(20));
        worker.start(history, 0, TranscribeOpts::default());
        std::thread::sleep(Duration::from_millis(120));

        let committed = worker.finish();
        assert_eq!(committed.text, "");
        assert_eq!(committed.until, 0);
    }

    #[test]
    fn quietest_offset_finds_the_gap() {
        let mut region = vec![0.5f32; 16_000];
        for s in region.iter_mut().skip(8_000).take(2_000) {
            *s = 0.0; // silence in the middle
        }
        let at = quietest_offset(&region);
        assert!((8_000..10_000).contains(&at), "cut at {at}");
    }
}
