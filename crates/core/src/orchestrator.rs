use std::sync::mpsc;
use std::time::Instant;

use openflow_audio::{trim_silence, AudioEngine, Vad, SPEECH_THRESHOLD, TARGET_SAMPLE_RATE};
use openflow_inject::Injector;
use openflow_settings::{Config, HotkeyMode};
use openflow_stt::{SttEngine, TranscribeOpts};

use crate::partial::PartialWorker;

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

/// Per-dictation stage timings, for the latency monitor and history.
#[derive(Debug, Clone, Default)]
pub struct DictationTiming {
    pub audio_ms: u64,
    pub stt_ms: u64,
    pub llm_ms: u64,
    pub inject_ms: u64,
    pub total_ms: u64,
    pub chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Recording,
}

/// Rebuild hooks so config changes can swap hardware-bound services without
/// tearing down the orchestrator. Factories live in the desktop shell (which
/// knows about cpal devices and model files); `None` disables hot-swap.
pub type SttFactory = Box<dyn Fn(&str) -> Result<Box<dyn SttEngine>, String> + Send>;
pub type AudioFactory = Box<dyn Fn(Option<String>) -> AudioEngine + Send>;

pub struct Services {
    pub audio: AudioEngine,
    pub stt: Box<dyn SttEngine>,
    pub vad: Option<Box<dyn Vad>>,
    pub injector: Box<dyn Injector>,
    pub partials: Option<PartialWorker>,
    pub stt_factory: Option<SttFactory>,
    pub audio_factory: Option<AudioFactory>,
    /// LLM cleanup; `None` = deterministic transcripts only.
    pub cleanup: Option<openflow_llm::CleanupService>,
    /// Dictionary/snippets/history; `None` = stateless dictation.
    pub store: Option<std::sync::Arc<openflow_store::Store>>,
    /// Focused-app awareness; `None` = generic tone, no focus-abort check.
    pub context: Option<Box<dyn openflow_context::ContextProvider>>,
}

pub struct Callbacks {
    pub on_status: Box<dyn Fn(Status) + Send>,
    pub on_timing: Option<Box<dyn Fn(DictationTiming) + Send>>,
}

/// Owns the dictation pipeline. Construct with `new`, then call `run` on a
/// dedicated thread; interact via the `OrchestratorHandle`.
pub struct Orchestrator {
    services: Services,
    config: Config,
    callbacks: Callbacks,
    rx: mpsc::Receiver<SessionEvent>,
    state: State,
    /// Context captured at key-down (design doc §7: parallel to speaking).
    session_context: Option<SessionContext>,
}

struct SessionContext {
    app: openflow_context::AppInfo,
    profile: openflow_context::AppProfile,
    text: openflow_context::TextContext,
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
    pub fn new(
        services: Services,
        config: Config,
        callbacks: Callbacks,
    ) -> (Self, OrchestratorHandle) {
        let (tx, rx) = mpsc::channel();
        (
            Self {
                services,
                config,
                callbacks,
                rx,
                state: State::Idle,
                session_context: None,
            },
            OrchestratorHandle { tx },
        )
    }

    /// Blocking event loop; returns on `Shutdown` or when all handles drop.
    pub fn run(mut self) {
        if let Err(e) = self.services.audio.start() {
            tracing::error!(error = %e, "audio engine failed to start; dictation disabled");
            return;
        }
        (self.callbacks.on_status)(Status::Idle);

        while let Ok(event) = self.rx.recv() {
            match event {
                SessionEvent::HotkeyPressed => self.on_hotkey_pressed(),
                SessionEvent::HotkeyReleased => self.on_hotkey_released(),
                SessionEvent::ConfigChanged(config) => self.apply_config(*config),
                SessionEvent::Shutdown => break,
            }
        }
        self.services.audio.stop();
    }

    fn apply_config(&mut self, new: Config) {
        // Mic device changed: rebuild the audio engine (never mid-recording;
        // a recording session keeps the old engine until it finishes).
        if new.audio.input_device != self.config.audio.input_device && self.state == State::Idle {
            if let Some(factory) = self.services.audio_factory.as_ref() {
                self.services.audio.stop();
                self.services.audio = factory(new.audio.input_device.clone());
                if let Err(e) = self.services.audio.start() {
                    tracing::error!(error = %e, "audio engine rebuild failed");
                }
                tracing::info!(device = ?new.audio.input_device, "audio engine rebuilt");
            }
        }
        // STT model changed: rebuild the engine (downloads if needed).
        if new.stt.model_id != self.config.stt.model_id {
            if let Some(factory) = self.services.stt_factory.as_ref() {
                match factory(&new.stt.model_id) {
                    Ok(engine) => {
                        self.services.stt = engine;
                        tracing::info!(model = new.stt.model_id, "stt engine rebuilt");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "stt rebuild failed; keeping previous model")
                    }
                }
            }
        }
        self.config = new;
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
        let start = self
            .services
            .audio
            .begin_session(self.config.audio.preroll_ms);
        if let Some(partials) = self.services.partials.as_ref() {
            partials.start(self.services.audio.history_handle(), start);
        }
        // Snapshot context at key-down, in parallel with the user speaking.
        self.session_context = self.services.context.as_ref().and_then(|provider| {
            let app = provider.focused_app()?;
            let profile = openflow_context::classify(&app);
            let text = provider.text_context();
            tracing::debug!(process = app.process, ?profile, "session context captured");
            Some(SessionContext { app, profile, text })
        });
        self.state = State::Recording;
        (self.callbacks.on_status)(Status::Recording);
        tracing::debug!("recording started");
    }

    fn finish_recording(&mut self) {
        self.state = State::Idle;
        if let Some(partials) = self.services.partials.as_ref() {
            partials.stop();
        }
        (self.callbacks.on_status)(Status::Processing);
        let started = Instant::now();
        let mut timing = DictationTiming::default();

        let outcome = self.services.audio.end_session().and_then(|audio| {
            timing.audio_ms = (audio.len() as u64 * 1000) / TARGET_SAMPLE_RATE as u64;
            let audio = self.trimmed(audio);
            // Sub-300 ms utterances are hotkey fumbles, and Whisper
            // hallucinates on near-silence — skip them.
            if audio.len() < (TARGET_SAMPLE_RATE as usize) * 3 / 10 {
                tracing::debug!(samples = audio.len(), "utterance too short; skipped");
                return None;
            }
            Some(audio)
        });

        let session_context = self.session_context.take();
        if let Some(audio) = outcome {
            // Personalization inputs (design doc §2 stage 5).
            let (dict_terms, snippet_defs) = self.personalization();

            // STT bias: dictionary words + entities from on-screen text (§7).
            let mut bias_parts: Vec<String> = Vec::new();
            if let Some(bias) = openflow_postproc::dictionary::bias_prompt(&dict_terms) {
                bias_parts.push(bias);
            }
            if let Some(ctx) = session_context.as_ref() {
                let mut screen_text = ctx.text.preceding_text.clone().unwrap_or_default();
                if let Some(sel) = &ctx.text.selection {
                    screen_text.push(' ');
                    screen_text.push_str(sel);
                }
                let entities = openflow_context::extract_entities(&screen_text, 12);
                if !entities.is_empty() {
                    bias_parts.push(entities.join(", "));
                }
            }
            let opts = TranscribeOpts {
                language: match self.config.language.as_str() {
                    "auto" => None,
                    lang => Some(lang.to_string()),
                },
                initial_prompt: if bias_parts.is_empty() {
                    None
                } else {
                    Some(bias_parts.join(", "))
                },
            };
            let stt_started = Instant::now();
            match self.services.stt.transcribe(&audio, &opts) {
                Ok(t) if !t.text.trim().is_empty() => {
                    timing.stt_ms = stt_started.elapsed().as_millis() as u64;
                    let raw = t.text.trim().to_string();

                    // 1. Dictionary fuzzy-replace. 2. Snippet short-circuit.
                    // 3. LLM cleanup with guard + fallback.
                    let after_dict = openflow_postproc::apply_dictionary(&raw, &dict_terms);
                    let final_text = if let Some(expansion) =
                        openflow_postproc::match_snippet(&after_dict, &snippet_defs)
                    {
                        expansion.to_string()
                    } else if let Some(cleanup) = self.services.cleanup.as_ref() {
                        let ctx = openflow_llm::CleanupContext {
                            tone: session_context.as_ref().and_then(|c| {
                                openflow_context::tone_for(c.profile).map(String::from)
                            }),
                            dictionary: dict_terms.iter().map(|t| t.word.clone()).collect(),
                            preceding_text: session_context
                                .as_ref()
                                .and_then(|c| c.text.preceding_text.clone()),
                        };
                        let result = cleanup.cleanup(&after_dict, &self.config.llm, &ctx);
                        timing.llm_ms = result.llm_ms;
                        result.text
                    } else {
                        after_dict
                    };

                    // Abort if focus moved to a different app while we were
                    // transcribing — never paste into the wrong window (§7).
                    if let (Some(captured), Some(provider)) =
                        (session_context.as_ref(), self.services.context.as_ref())
                    {
                        if let Some(now) = provider.focused_app() {
                            if now.process != captured.app.process {
                                tracing::warn!(
                                    was = captured.app.process,
                                    now = now.process,
                                    "focus changed during dictation; injection aborted"
                                );
                                (self.callbacks.on_status)(Status::Idle);
                                return;
                            }
                        }
                    }

                    // Trailing space so consecutive dictations chain naturally.
                    let text = format!("{} ", final_text.trim_end());
                    timing.chars = text.chars().count();
                    let inject_started = Instant::now();
                    match self.services.injector.inject(&text) {
                        Ok(()) => {
                            timing.inject_ms = inject_started.elapsed().as_millis() as u64;
                            timing.total_ms = started.elapsed().as_millis() as u64;
                            tracing::info!(
                                stt_ms = timing.stt_ms,
                                llm_ms = timing.llm_ms,
                                total_ms = timing.total_ms,
                                chars = timing.chars,
                                "dictation injected"
                            );
                            if let Some(store) = self.services.store.as_ref() {
                                if let Err(e) = store.history_insert(
                                    &raw,
                                    text.trim_end(),
                                    session_context.as_ref().map(|c| c.app.process.as_str()),
                                    timing.total_ms as i64,
                                    timing.stt_ms as i64,
                                    timing.llm_ms as i64,
                                    timing.chars as i64,
                                ) {
                                    tracing::warn!(error = %e, "history insert failed");
                                }
                            }
                            if let Some(on_timing) = self.callbacks.on_timing.as_ref() {
                                on_timing(timing.clone());
                            }
                        }
                        Err(e) => tracing::error!(error = %e, "injection failed"),
                    }
                }
                Ok(_) => tracing::debug!("empty transcript; nothing to inject"),
                Err(e) => tracing::error!(error = %e, "transcription failed"),
            }
        }
        (self.callbacks.on_status)(Status::Idle);
    }

    fn personalization(
        &self,
    ) -> (
        Vec<openflow_postproc::dictionary::Term>,
        Vec<openflow_postproc::snippets::SnippetDef>,
    ) {
        let Some(store) = self.services.store.as_ref() else {
            return (Vec::new(), Vec::new());
        };
        let terms = store
            .dictionary_list()
            .unwrap_or_default()
            .into_iter()
            .map(|e| openflow_postproc::dictionary::Term {
                word: e.word,
                hint: e.hint,
            })
            .collect();
        let snippets = store
            .snippets_list()
            .unwrap_or_default()
            .into_iter()
            .map(|s| openflow_postproc::snippets::SnippetDef {
                trigger: s.trigger,
                expansion: s.expansion,
            })
            .collect();
        (terms, snippets)
    }

    fn trimmed(&mut self, audio: Vec<f32>) -> Vec<f32> {
        let Some(vad) = self.services.vad.as_mut() else {
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
        timings: Arc<Mutex<Vec<DictationTiming>>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    fn start_harness(mode: HotkeyMode, stt: FakeStt) -> Harness {
        start_harness_full(mode, stt, None, None)
    }

    fn start_harness_full(
        mode: HotkeyMode,
        stt: FakeStt,
        cleanup: Option<openflow_llm::CleanupService>,
        store: Option<Arc<openflow_store::Store>>,
    ) -> Harness {
        let mut config = Config::default();
        config.hotkey.mode = mode;
        config.audio.preroll_ms = 0;

        let audio = AudioEngine::new(Box::new(FakeAudioSource::new(one_second_of_noise())));
        let injector = FakeInjector::default();
        let statuses = Arc::new(Mutex::new(Vec::new()));
        let statuses2 = statuses.clone();
        let timings = Arc::new(Mutex::new(Vec::new()));
        let timings2 = timings.clone();

        let (orch, handle) = Orchestrator::new(
            Services {
                audio,
                stt: Box::new(stt),
                vad: None,
                injector: Box::new(injector.clone()),
                partials: None,
                stt_factory: None,
                audio_factory: None,
                cleanup,
                store,
                context: None,
            },
            config,
            Callbacks {
                on_status: Box::new(move |s| statuses2.lock().unwrap().push(s)),
                on_timing: Some(Box::new(move |t| timings2.lock().unwrap().push(t))),
            },
        );
        let thread = std::thread::spawn(move || orch.run());
        Harness {
            handle,
            injector,
            statuses,
            timings,
            thread: Some(thread),
        }
    }

    fn dictate(h: &Harness) {
        h.handle.post(SessionEvent::HotkeyPressed);
        std::thread::sleep(Duration::from_millis(400));
        h.handle.post(SessionEvent::HotkeyReleased);
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
        let timings = h.timings.lock().unwrap();
        assert_eq!(timings.len(), 1);
        assert_eq!(timings[0].chars, "hello world ".chars().count());
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
        assert!(h.timings.lock().unwrap().is_empty());
    }

    #[test]
    fn dictionary_corrects_transcript_and_biases_stt() {
        let store = Arc::new(openflow_store::Store::open_in_memory().unwrap());
        store
            .dictionary_add("kubectl", Some("cube control"))
            .unwrap();
        let stt = FakeStt::with_responses(["run cube control apply"]);
        let mut h = start_harness_full(HotkeyMode::Hold, stt.clone(), None, Some(store));
        dictate(&h);
        h.shutdown();

        assert_eq!(h.injected(), vec!["run kubectl apply ".to_string()]);
        let calls = stt.calls.lock().unwrap();
        assert_eq!(calls[0].opts.initial_prompt.as_deref(), Some("kubectl"));
    }

    #[test]
    fn snippet_short_circuits_the_llm() {
        let store = Arc::new(openflow_store::Store::open_in_memory().unwrap());
        store.snippets_add("my address", "42 Example Lane").unwrap();
        // If the LLM ran, this canned response would be injected instead.
        let cleanup =
            openflow_llm::CleanupService::new(Box::new(openflow_llm::FakeLlm::with_responses([
                "LLM MUST NOT RUN",
            ])));
        let stt = FakeStt::with_responses(["my address"]);
        let mut h = start_harness_full(HotkeyMode::Hold, stt, Some(cleanup), Some(store.clone()));
        dictate(&h);
        h.shutdown();

        assert_eq!(h.injected(), vec!["42 Example Lane ".to_string()]);
        // History records the exchange.
        assert_eq!(store.history_list(None, 10).unwrap().len(), 1);
    }

    #[test]
    fn llm_cleanup_formats_and_history_records_raw_and_formatted() {
        let store = Arc::new(openflow_store::Store::open_in_memory().unwrap());
        let cleanup =
            openflow_llm::CleanupService::new(Box::new(openflow_llm::FakeLlm::with_responses([
                "We ship on Monday.",
            ])));
        let stt = FakeStt::with_responses(["um we ship on friday no wait monday"]);
        let mut h = start_harness_full(HotkeyMode::Hold, stt, Some(cleanup), Some(store.clone()));
        dictate(&h);
        h.shutdown();

        assert_eq!(h.injected(), vec!["We ship on Monday. ".to_string()]);
        let history = store.history_list(None, 10).unwrap();
        assert_eq!(history[0].raw, "um we ship on friday no wait monday");
        assert_eq!(history[0].formatted, "We ship on Monday.");
    }

    #[test]
    fn app_profile_tone_reaches_the_cleanup_prompt() {
        // Slack context → cleanup system prompt must carry the casual tone.
        let fake_llm = openflow_llm::FakeLlm::with_responses(["hey, ship it monday"]);
        let cleanup = openflow_llm::CleanupService::new(Box::new(fake_llm.clone()));
        let context = openflow_context::FakeContext {
            app: Some(openflow_context::AppInfo {
                process: "slack.exe".into(),
                title: "#general - Slack".into(),
            }),
            text: openflow_context::TextContext::default(),
        };

        let mut config = Config::default();
        config.hotkey.mode = HotkeyMode::Hold;
        config.audio.preroll_ms = 0;
        let audio = AudioEngine::new(Box::new(FakeAudioSource::new(one_second_of_noise())));
        let injector = FakeInjector::default();
        let (orch, handle) = Orchestrator::new(
            Services {
                audio,
                stt: Box::new(FakeStt::with_responses(["um hey ship it monday"])),
                vad: None,
                injector: Box::new(injector.clone()),
                partials: None,
                stt_factory: None,
                audio_factory: None,
                cleanup: Some(cleanup),
                store: None,
                context: Some(Box::new(context)),
            },
            config,
            Callbacks {
                on_status: Box::new(|_| {}),
                on_timing: None,
            },
        );
        let thread = std::thread::spawn(move || orch.run());
        handle.post(SessionEvent::HotkeyPressed);
        std::thread::sleep(Duration::from_millis(400));
        handle.post(SessionEvent::HotkeyReleased);
        handle.post(SessionEvent::Shutdown);
        thread.join().unwrap();

        let calls = fake_llm.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].1[0].content.contains("casual"),
            "tone block missing from system prompt"
        );
        assert_eq!(injector.injected.lock().unwrap()[0], "hey, ship it monday ");
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
