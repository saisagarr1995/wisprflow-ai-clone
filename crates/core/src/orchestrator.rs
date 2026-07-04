use std::sync::mpsc;
use std::time::Instant;

use openflow_audio::{trim_silence, AudioEngine, Vad, SPEECH_THRESHOLD, TARGET_SAMPLE_RATE};
use openflow_inject::Injector;
use openflow_settings::{Config, HotkeyMode};
use openflow_stt::{SttEngine, TranscribeOpts};

/// Events posted into the orchestrator from hotkey/audio/UI threads.
/// `Config` is boxed to keep the enum small (it travels through channels).
#[derive(Debug, Clone)]
pub enum SessionEvent {
    HotkeyPressed,
    HotkeyReleased,
    ConfigChanged(Box<Config>),
    Shutdown,
}

/// What the outside world (tray icon, HUD) sees of the state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    Recording,
    Processing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Recording,
}

/// Owns the dictation pipeline. Construct with `new`, then call `run` on a
/// dedicated thread; interact via the `OrchestratorHandle`.
pub struct Orchestrator {
    audio: AudioEngine,
    stt: Box<dyn SttEngine>,
    vad: Option<Box<dyn Vad>>,
    injector: Box<dyn Injector>,
    config: Config,
    on_status: Box<dyn Fn(Status) + Send>,
    rx: mpsc::Receiver<SessionEvent>,
    state: State,
}

/// Cloneable sender for posting events from any thread.
#[derive(Clone)]
pub struct OrchestratorHandle {
    tx: mpsc::Sender<SessionEvent>,
}

impl OrchestratorHandle {
    pub fn post(&self, event: SessionEvent) {
        let _ = self.tx.send(event);
    }
}

impl Orchestrator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        audio: AudioEngine,
        stt: Box<dyn SttEngine>,
        vad: Option<Box<dyn Vad>>,
        injector: Box<dyn Injector>,
        config: Config,
        on_status: Box<dyn Fn(Status) + Send>,
    ) -> (Self, OrchestratorHandle) {
        let (tx, rx) = mpsc::channel();
        (
            Self {
                audio,
                stt,
                vad,
                injector,
                config,
                on_status,
                rx,
                state: State::Idle,
            },
            OrchestratorHandle { tx },
        )
    }

    /// Blocking event loop; returns on `Shutdown` or when all handles drop.
    pub fn run(mut self) {
        if let Err(e) = self.audio.start() {
            tracing::error!(error = %e, "audio engine failed to start; dictation disabled");
            return;
        }
        (self.on_status)(Status::Idle);

        while let Ok(event) = self.rx.recv() {
            match event {
                SessionEvent::HotkeyPressed => self.on_hotkey_pressed(),
                SessionEvent::HotkeyReleased => self.on_hotkey_released(),
                SessionEvent::ConfigChanged(config) => {
                    // Device/model swaps need service rebuilds (Prompt 9);
                    // behavioral settings apply immediately.
                    self.config = *config;
                }
                SessionEvent::Shutdown => break,
            }
        }
        self.audio.stop();
    }

    fn on_hotkey_pressed(&mut self) {
        match (self.state, self.config.hotkey.mode) {
            (State::Idle, _) => self.begin_recording(),
            // Toggle mode: second press ends the session.
            (State::Recording, HotkeyMode::Toggle) => self.finish_recording(),
            // Hold mode: pressed while recording = autorepeat echo; ignore.
            (State::Recording, HotkeyMode::Hold) => {}
        }
    }

    fn on_hotkey_released(&mut self) {
        // Toggle mode ignores releases; Idle releases are stray.
        if self.state == State::Recording && self.config.hotkey.mode == HotkeyMode::Hold {
            self.finish_recording();
        }
    }

    fn begin_recording(&mut self) {
        self.audio.begin_session(self.config.audio.preroll_ms);
        self.state = State::Recording;
        (self.on_status)(Status::Recording);
        tracing::debug!("recording started");
    }

    fn finish_recording(&mut self) {
        self.state = State::Idle;
        (self.on_status)(Status::Processing);
        let started = Instant::now();

        let outcome = self.audio.end_session().and_then(|audio| {
            let audio = self.trimmed(audio);
            // Sub-300 ms utterances are hotkey fumbles, and Whisper
            // hallucinates on near-silence — skip them.
            if audio.len() < (TARGET_SAMPLE_RATE as usize) * 3 / 10 {
                tracing::debug!(samples = audio.len(), "utterance too short; skipped");
                return None;
            }
            Some(audio)
        });

        if let Some(audio) = outcome {
            let secs = audio.len() as f32 / TARGET_SAMPLE_RATE as f32;
            let opts = TranscribeOpts {
                language: match self.config.language.as_str() {
                    "auto" => None,
                    lang => Some(lang.to_string()),
                },
                initial_prompt: None, // dictionary biasing lands in Prompt 15
            };
            match self.stt.transcribe(&audio, &opts) {
                Ok(t) if !t.text.trim().is_empty() => {
                    // Trailing space so consecutive dictations chain naturally.
                    let text = format!("{} ", t.text.trim());
                    match self.injector.inject(&text) {
                        Ok(()) => tracing::info!(
                            audio_secs = secs,
                            total_ms = started.elapsed().as_millis() as u64,
                            chars = text.len(),
                            "dictation injected"
                        ),
                        Err(e) => tracing::error!(error = %e, "injection failed"),
                    }
                }
                Ok(_) => tracing::debug!("empty transcript; nothing to inject"),
                Err(e) => tracing::error!(error = %e, "transcription failed"),
            }
        }
        (self.on_status)(Status::Idle);
    }

    fn trimmed(&mut self, audio: Vec<f32>) -> Vec<f32> {
        let Some(vad) = self.vad.as_mut() else {
            return audio;
        };
        match trim_silence(vad.as_mut(), &audio, SPEECH_THRESHOLD, 150) {
            Ok(range) if range.is_empty() => Vec::new(),
            Ok(range) => audio[range].to_vec(),
            Err(e) => {
                tracing::warn!(error = %e, "vad trim failed; using untrimmed audio");
                audio
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openflow_audio::FakeAudioSource;
    use openflow_inject::FakeInjector;
    use openflow_stt::FakeStt;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn one_second_of_noise() -> Vec<f32> {
        // Loud enough to pass the min-length gate without a VAD (vad: None).
        (0..TARGET_SAMPLE_RATE as usize)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect()
    }

    struct Harness {
        handle: OrchestratorHandle,
        injector: FakeInjector,
        statuses: Arc<Mutex<Vec<Status>>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    fn start_harness(mode: HotkeyMode, stt: FakeStt) -> Harness {
        let mut config = Config::default();
        config.hotkey.mode = mode;
        config.audio.preroll_ms = 0;

        let audio = AudioEngine::new(Box::new(FakeAudioSource::new(one_second_of_noise())));
        let injector = FakeInjector::default();
        let statuses = Arc::new(Mutex::new(Vec::new()));
        let statuses2 = statuses.clone();

        let (orch, handle) = Orchestrator::new(
            audio,
            Box::new(stt),
            None,
            Box::new(injector.clone()),
            config,
            Box::new(move |s| statuses2.lock().unwrap().push(s)),
        );
        let thread = std::thread::spawn(move || orch.run());
        Harness {
            handle,
            injector,
            statuses,
            thread: Some(thread),
        }
    }

    impl Harness {
        fn shutdown(&mut self) {
            self.handle.post(SessionEvent::Shutdown);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
        fn injected(&self) -> Vec<String> {
            self.injector.injected.lock().unwrap().clone()
        }
    }

    #[test]
    fn hold_mode_dictates_once_per_press_release() {
        let mut h = start_harness(HotkeyMode::Hold, FakeStt::with_responses(["hello world"]));
        h.handle.post(SessionEvent::HotkeyPressed);
        std::thread::sleep(Duration::from_millis(400)); // let fake audio flow
        h.handle.post(SessionEvent::HotkeyReleased);
        h.shutdown();

        assert_eq!(h.injected(), vec!["hello world ".to_string()]);
        let statuses = h.statuses.lock().unwrap().clone();
        assert_eq!(
            statuses,
            vec![
                Status::Idle,
                Status::Recording,
                Status::Processing,
                Status::Idle
            ]
        );
    }

    #[test]
    fn toggle_mode_uses_two_presses_and_ignores_releases() {
        let mut h = start_harness(HotkeyMode::Toggle, FakeStt::with_responses(["toggled"]));
        h.handle.post(SessionEvent::HotkeyPressed);
        h.handle.post(SessionEvent::HotkeyReleased); // must be ignored
        std::thread::sleep(Duration::from_millis(400));
        h.handle.post(SessionEvent::HotkeyPressed);
        h.shutdown();

        assert_eq!(h.injected(), vec!["toggled ".to_string()]);
    }

    #[test]
    fn too_short_utterance_is_skipped() {
        let mut h = start_harness(HotkeyMode::Hold, FakeStt::default());
        h.handle.post(SessionEvent::HotkeyPressed);
        // Release immediately: near-zero audio captured.
        h.handle.post(SessionEvent::HotkeyReleased);
        h.shutdown();
        assert!(h.injected().is_empty(), "short fumble must not inject");
    }

    #[test]
    fn random_event_sequences_never_wedge_the_loop() {
        // Cheap property test: an LCG drives arbitrary event storms; the loop
        // must survive and still end at Idle status via clean shutdown.
        let mut h = start_harness(HotkeyMode::Hold, FakeStt::default());
        let mut seed: u64 = 0x00F1_0E5E_ED15_C0DE;
        for _ in 0..500 {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            match (seed >> 33) % 3 {
                0 => h.handle.post(SessionEvent::HotkeyPressed),
                1 => h.handle.post(SessionEvent::HotkeyReleased),
                _ => h.handle.post(SessionEvent::ConfigChanged(Box::default())),
            }
        }
        h.shutdown(); // hangs (test timeout) if the loop wedged
        let last = *h.statuses.lock().unwrap().last().unwrap();
        assert_eq!(last, Status::Idle);
    }
}
