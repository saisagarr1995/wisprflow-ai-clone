//! Streaming partial transcripts via chunked re-decode (design doc §2 stage 4,
//! "chunked re-decode" strategy): while recording, periodically re-transcribe
//! the whole session-so-far with a small model and emit the volatile result.
//! Partials are UI-only — nothing downstream consumes them; the final pass on
//! hotkey release is authoritative.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use openflow_audio::{RollingBuffer, TARGET_SAMPLE_RATE};
use openflow_stt::{SttEngine, TranscribeOpts};

pub enum PartialCmd {
    Start {
        history: Arc<Mutex<RollingBuffer>>,
        from: u64,
        /// Session language ("ta", "te"...); `None` = auto-detect. Captions
        /// must decode in the same language as the final pass.
        language: Option<String>,
    },
    Stop,
    Shutdown,
}

/// Owns a dedicated thread + its own (small) STT engine so partial decodes
/// never block the orchestrator or the final pass.
pub struct PartialWorker {
    tx: mpsc::Sender<PartialCmd>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PartialWorker {
    /// `interval` is the re-decode cadence (~600 ms is a good CPU balance).
    pub fn spawn(
        mut stt: Box<dyn SttEngine>,
        on_partial: Box<dyn Fn(String) + Send>,
        interval: Duration,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<PartialCmd>();
        let thread = std::thread::Builder::new()
            .name("openflow-partials".into())
            .spawn(move || {
                let mut active: Option<(Arc<Mutex<RollingBuffer>>, u64, Option<String>)> = None;
                loop {
                    // Idle: block. Active: poll so we can decode between commands.
                    let cmd = if active.is_none() {
                        rx.recv().ok()
                    } else {
                        match rx.recv_timeout(interval) {
                            Ok(c) => Some(c),
                            Err(mpsc::RecvTimeoutError::Timeout) => None,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    };
                    match cmd {
                        Some(PartialCmd::Start {
                            history,
                            from,
                            language,
                        }) => {
                            active = Some((history, from, language));
                            continue;
                        }
                        Some(PartialCmd::Stop) => {
                            active = None;
                            continue;
                        }
                        Some(PartialCmd::Shutdown) => break,
                        None => {} // timeout tick — decode below
                    }
                    if let Some((history, from, language)) = active.as_mut() {
                        let audio = {
                            let h = history.lock().expect("history lock");
                            let head = h.head();
                            // Sliding window: decode only the most recent few
                            // seconds. Re-decoding the whole utterance grows
                            // O(n²) and lags far behind speech on slow CPUs;
                            // the final pass on release covers the full audio.
                            let window = TARGET_SAMPLE_RATE as u64 * 5;
                            let start = (*from).max(head.saturating_sub(window));
                            h.snapshot(start, head)
                        };
                        // Skip until there's half a second to say anything about.
                        if audio.len() < TARGET_SAMPLE_RATE as usize / 2 {
                            continue;
                        }
                        // Few threads: captions are a nicety; they must never
                        // starve the final pass, the UI, or other apps.
                        let opts = TranscribeOpts {
                            language: language.clone(),
                            threads: Some(3),
                            ..TranscribeOpts::default()
                        };
                        match stt.transcribe(&audio, &opts) {
                            Ok(t) => {
                                // Auto-detect runs an extra pass per decode;
                                // pin the first detection for the rest of the
                                // session so later ticks skip it.
                                if language.is_none() {
                                    *language = t.language.clone();
                                }
                                if !t.text.trim().is_empty() {
                                    on_partial(t.text);
                                }
                            }
                            Err(e) => tracing::debug!(error = %e, "partial decode failed"),
                        }
                    }
                }
            })
            .expect("spawn partial worker");
        Self {
            tx,
            thread: Some(thread),
        }
    }

    pub fn start(&self, history: Arc<Mutex<RollingBuffer>>, from: u64, language: Option<String>) {
        let _ = self.tx.send(PartialCmd::Start {
            history,
            from,
            language,
        });
    }

    pub fn stop(&self) {
        let _ = self.tx.send(PartialCmd::Stop);
    }
}

impl Drop for PartialWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(PartialCmd::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openflow_stt::FakeStt;

    #[test]
    fn emits_partials_while_active_and_stops_cleanly() {
        let history = Arc::new(Mutex::new(RollingBuffer::new(64_000)));
        history.lock().unwrap().push(&vec![0.1; 16_000]); // 1 s of "audio"

        let (ptx, prx) = mpsc::channel();
        let worker = PartialWorker::spawn(
            Box::new(FakeStt::with_responses(["partial one", "partial two"])),
            Box::new(move |t| {
                let _ = ptx.send(t);
            }),
            Duration::from_millis(30),
        );

        worker.start(history.clone(), 0, None);
        let first = prx
            .recv_timeout(Duration::from_secs(2))
            .expect("first partial");
        assert_eq!(first, "partial one");
        let second = prx
            .recv_timeout(Duration::from_secs(2))
            .expect("second partial");
        assert_eq!(second, "partial two");

        worker.stop();
        // Drain anything in flight, then confirm silence.
        while prx.recv_timeout(Duration::from_millis(120)).is_ok() {}
        assert!(prx.recv_timeout(Duration::from_millis(200)).is_err());
        drop(worker); // must join, not hang
    }
}
