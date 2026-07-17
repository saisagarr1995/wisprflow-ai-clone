use std::sync::mpsc;
use std::time::{Duration, Instant};

use openflow_audio::{trim_silence, AudioEngine, Vad, SPEECH_THRESHOLD, TARGET_SAMPLE_RATE};
use openflow_inject::Injector;
use openflow_settings::{Config, HotkeyMode};
use openflow_stt::{SttEngine, TranscribeOpts};

use crate::finalizer::{SharedStt, StreamFinalizer};
use crate::partial::PartialWorker;

/// Events posted into the orchestrator from hotkey/audio/UI threads.
/// `Config` is boxed to keep the enum small (it travels through channels).
#[derive(Debug, Clone)]
pub enum SessionEvent {
    HotkeyPressed,
    HotkeyReleased,
    /// HUD pill clicked while idle: start dictating (design: Wispr's
    /// "Click or hold <chord> to start dictating").
    HudStart,
    /// HUD stop button: finish the session and paste, exactly like a hotkey
    /// release in Hold mode.
    HudFinish,
    /// HUD cancel button: throw the session away — no transcription, no
    /// injection.
    HudCancel,
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

/// How often a hands-free recording checks for trailing silence.
const HANDS_FREE_TICK: Duration = Duration::from_millis(200);
/// Hands-free: this much continuous silence after speech ends the session.
const HANDS_FREE_SILENCE_MS: u64 = 2_000;
/// RMS analysis window for the silence detector (100 ms).
const HANDS_FREE_WIN_MS: u64 = 100;
/// A session must have heard at least this loud a window to count as speech;
/// below it, hands-free keeps listening instead of auto-stopping on nothing.
const HANDS_FREE_MIN_PEAK: f32 = 0.015;

/// Rebuild hooks so config changes can swap hardware-bound services without
/// tearing down the orchestrator. Factories live in the desktop shell (which
/// knows about cpal devices and model files); `None` disables hot-swap.
pub type SttFactory = Box<dyn Fn(&str) -> Result<Box<dyn SttEngine>, String> + Send>;
pub type AudioFactory = Box<dyn Fn(Option<String>) -> AudioEngine + Send>;

pub struct Services {
    pub audio: AudioEngine,
    /// The final engine, shared with the streaming finalizer so chunks can be
    /// committed while the user is still speaking.
    pub stt: SharedStt,
    pub vad: Option<Box<dyn Vad>>,
    pub injector: Box<dyn Injector>,
    pub partials: Option<PartialWorker>,
    /// Commits chunks of the utterance with the final engine while the user
    /// is still speaking, so release only decodes the tail. `None` = decode
    /// everything at release (tests, or when spawning the worker failed).
    pub finalizer: Option<StreamFinalizer>,
    pub stt_factory: Option<SttFactory>,
    pub audio_factory: Option<AudioFactory>,
    /// LLM cleanup; `None` = deterministic transcripts only.
    pub cleanup: Option<openflow_llm::CleanupService>,
    /// LLM backend + commands for voice-command transforms (design doc §10).
    /// `None` disables command handling (dictation only).
    pub commands: Option<CommandRuntime>,
    /// Dictionary/snippets/history; `None` = stateless dictation.
    pub store: Option<std::sync::Arc<openflow_store::Store>>,
    /// Focused-app awareness; `None` = generic tone, no focus-abort check.
    pub context: Option<Box<dyn openflow_context::ContextProvider>>,
    /// Used instead of `injector` when the Terminal profile is active
    /// (keystrokes, newline-stripped — never paste into a shell prompt).
    pub terminal_injector: Option<Box<dyn Injector>>,
}

pub struct Callbacks {
    pub on_status: Box<dyn Fn(Status) + Send>,
    pub on_timing: Option<Box<dyn Fn(DictationTiming) + Send>>,
    /// Assistant-command answer to show in a popover (design doc §10).
    pub on_assistant: Option<Box<dyn Fn(String) + Send>>,
    /// A short HUD flash after a command transform ("Rewrote 42 words").
    pub on_flash: Option<Box<dyn Fn(String) + Send>>,
}

/// Owns the dictation pipeline. Construct with `new`, then call `run` on a
/// dedicated thread; interact via the `OrchestratorHandle`.
pub struct Orchestrator {
    services: Services,
    config: Config,
    callbacks: Callbacks,
    rx: mpsc::Receiver<SessionEvent>,
    state: State,
    /// Absolute sample index where the current session began.
    session_start: u64,
    /// Decode options built at key-down; chunk and tail passes must agree.
    session_opts: TranscribeOpts,
    /// Context captured at key-down (design doc §7: parallel to speaking).
    session_context: Option<SessionContext>,
    /// Last text injected, used as the operand for commands when nothing is
    /// selected ("...and we'll ship Friday" then "make that professional").
    last_injection: Option<String>,
    /// Loudest 100 ms RMS window heard this session; hands-free auto-stop
    /// measures trailing silence relative to this.
    session_peak: f32,
}

struct SessionContext {
    app: openflow_context::AppInfo,
    profile: openflow_context::AppProfile,
    text: openflow_context::TextContext,
}

/// LLM backend + the registered command set, for voice-command handling.
pub struct CommandRuntime {
    pub backend: Box<dyn openflow_llm::LlmBackend>,
    pub commands: Vec<openflow_llm::Command>,
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
                session_start: 0,
                session_opts: TranscribeOpts::default(),
                session_context: None,
                last_injection: None,
                session_peak: 0.0,
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

        loop {
            // Hands-free recording needs a heartbeat to notice trailing
            // silence; every other state just blocks on the next event.
            let event = if self.state == State::Recording
                && self.config.hotkey.mode == HotkeyMode::HandsFree
            {
                match self.rx.recv_timeout(HANDS_FREE_TICK) {
                    Ok(event) => event,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if self.hands_free_should_stop() {
                            tracing::debug!("hands-free: trailing silence; finishing");
                            self.finish_recording();
                        }
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match self.rx.recv() {
                    Ok(event) => event,
                    Err(_) => break,
                }
            };
            match event {
                SessionEvent::HotkeyPressed => self.on_hotkey_pressed(),
                SessionEvent::HotkeyReleased => self.on_hotkey_released(),
                SessionEvent::HudStart => {
                    if self.state == State::Idle {
                        self.begin_recording();
                    }
                }
                SessionEvent::HudFinish => {
                    if self.state == State::Recording {
                        self.finish_recording();
                    }
                }
                SessionEvent::HudCancel => {
                    if self.state == State::Recording {
                        self.cancel_recording();
                    }
                }
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
                        *self.services.stt.lock().expect("stt lock") = engine;
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
            // Toggle/hands-free: second press ends the session.
            (State::Recording, HotkeyMode::Toggle | HotkeyMode::HandsFree) => {
                self.finish_recording()
            }
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
            // Captions must decode in the session language (build_opts runs
            // later, after the context snapshot, so derive it here).
            let language = match self.config.language.as_str() {
                "auto" => None,
                lang => Some(lang.to_string()),
            };
            partials.start(self.services.audio.history_handle(), start, language);
        }
        // Snapshot context at key-down, in parallel with the user speaking.
        self.session_context = self.services.context.as_ref().and_then(|provider| {
            let app = provider.focused_app()?;
            let profile = openflow_context::classify(&app);
            let text = provider.text_context();
            tracing::debug!(process = app.process, ?profile, "session context captured");
            Some(SessionContext { app, profile, text })
        });
        // Build decode options now (bias needs the captured context) and hand
        // the finalizer the session so it can commit chunks while we record.
        self.session_start = start;
        self.session_opts = self.build_opts();
        if let Some(finalizer) = self.services.finalizer.as_ref() {
            finalizer.start(
                self.services.audio.history_handle(),
                start,
                self.session_opts.clone(),
            );
        }
        self.session_peak = 0.0;
        self.state = State::Recording;
        (self.callbacks.on_status)(Status::Recording);
        tracing::debug!("recording started");
    }

    /// Abort the current session: nothing is transcribed or injected.
    fn cancel_recording(&mut self) {
        self.state = State::Idle;
        if let Some(partials) = self.services.partials.as_ref() {
            partials.stop();
        }
        if let Some(finalizer) = self.services.finalizer.as_ref() {
            let _ = finalizer.finish(); // discard committed text
        }
        let _ = self.services.audio.end_session();
        self.session_context = None;
        (self.callbacks.on_status)(Status::Idle);
        tracing::debug!("recording cancelled");
    }

    /// Hands-free endpointing: true once the session has heard real speech
    /// and the trailing `HANDS_FREE_SILENCE_MS` were quiet relative to the
    /// session's loudest window. Level-based (not VAD) so it stays cheap on
    /// every tick and immune to Silero's recurrent-state ordering rules.
    fn hands_free_should_stop(&mut self) -> bool {
        let sr = TARGET_SAMPLE_RATE as u64;
        let win = (sr * HANDS_FREE_WIN_MS / 1000) as usize;
        let tail_windows = (HANDS_FREE_SILENCE_MS / HANDS_FREE_WIN_MS) as usize;
        let tail = {
            let history = self.services.audio.history_handle();
            let h = history.lock().expect("history lock");
            let head = h.head();
            // Too little audio yet to have both speech and a full silence run.
            if head.saturating_sub(self.session_start) < sr * (HANDS_FREE_SILENCE_MS + 500) / 1000 {
                return false;
            }
            let from = self
                .session_start
                .max(head.saturating_sub(win as u64 * (tail_windows as u64 + 5)));
            h.snapshot(from, head)
        };
        let levels: Vec<f32> = tail
            .chunks(win)
            .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len().max(1) as f32).sqrt())
            .collect();
        if let Some(max) = levels.iter().cloned().reduce(f32::max) {
            self.session_peak = self.session_peak.max(max);
        }
        if self.session_peak < HANDS_FREE_MIN_PEAK {
            return false; // nothing said yet — keep listening
        }
        let threshold = (self.session_peak * 0.15).max(0.006);
        levels.len() >= tail_windows
            && levels[levels.len() - tail_windows..]
                .iter()
                .all(|&l| l < threshold)
    }

    fn finish_recording(&mut self) {
        self.state = State::Idle;
        if let Some(partials) = self.services.partials.as_ref() {
            partials.stop();
        }
        (self.callbacks.on_status)(Status::Processing);
        let started = Instant::now();
        let mut timing = DictationTiming::default();

        // Everything the finalizer committed while the user was speaking;
        // only the audio after `committed.until` still needs a decode.
        let committed = self
            .services
            .finalizer
            .as_ref()
            .map(|f| f.finish())
            .unwrap_or_default();

        let outcome = self.services.audio.end_session().and_then(|audio| {
            timing.audio_ms = (audio.len() as u64 * 1000) / TARGET_SAMPLE_RATE as u64;
            let audio = if committed.text.is_empty() {
                self.trimmed(audio)
            } else {
                let done = committed.until.saturating_sub(self.session_start) as usize;
                self.trimmed(audio.get(done..).map(<[f32]>::to_vec).unwrap_or_default())
            };
            // Sub-300 ms utterances are hotkey fumbles, and Whisper
            // hallucinates on near-silence — skip them.
            if audio.len() < (TARGET_SAMPLE_RATE as usize) * 3 / 10 {
                tracing::debug!(samples = audio.len(), "utterance/tail too short; skipped");
                return None;
            }
            Some(audio)
        });

        let session_context = self.session_context.take();
        {
            // Personalization inputs (design doc §2 stage 5).
            let (dict_terms, snippet_defs) = self.personalization();

            // Decode the remaining tail with the options built at key-down
            // (same bias the chunk passes saw). When chunks already
            // committed, reuse their pinned language (skips whisper's
            // detection pass — a large slice of tail latency on auto-detect)
            // and continue from their text instead of the bias prompt.
            let mut opts = self.session_opts.clone();
            if opts.language.is_none() {
                opts.language = committed.language.clone();
            }
            if !committed.text.is_empty() {
                opts.initial_prompt = Some(crate::finalizer::tail_chars(&committed.text, 120));
            }
            let stt_started = Instant::now();
            let tail = match outcome {
                Some(audio) => {
                    let result = self
                        .services
                        .stt
                        .lock()
                        .expect("stt lock")
                        .transcribe(&audio, &opts);
                    match result {
                        Ok(t) => t.text.trim().to_string(),
                        Err(e) => {
                            tracing::error!(error = %e, "transcription failed");
                            String::new()
                        }
                    }
                }
                None => String::new(),
            };
            timing.stt_ms = stt_started.elapsed().as_millis() as u64;

            let mut joined = committed.text;
            if !tail.is_empty() {
                if !joined.is_empty() {
                    joined.push(' ');
                }
                joined.push_str(&tail);
            }
            match joined.trim() {
                "" => tracing::debug!("empty transcript; nothing to inject"),
                r => {
                    let raw = r.to_string();

                    // Voice command? (design doc §10). Operand = selection, or
                    // the last dictation. Runs before the dictation path.
                    if self.try_run_command(&raw, session_context.as_ref()) {
                        (self.callbacks.on_status)(Status::Idle);
                        return;
                    }

                    let profile = session_context.as_ref().map(|c| c.profile);
                    let coding_mode = matches!(
                        profile,
                        Some(
                            openflow_context::AppProfile::Coding
                                | openflow_context::AppProfile::Terminal
                        )
                    );
                    let terminal_mode =
                        matches!(profile, Some(openflow_context::AppProfile::Terminal));

                    // 1. Dictionary fuzzy-replace. 2. Coding grammar / casing
                    // commands; prose gets the deterministic tidy pass
                    // (stutters, fillers, punctuation) so text is Wispr-clean
                    // even with the LLM off. 3. Snippet short-circuit. 4. LLM
                    // cleanup with guard + fallback (skipped in terminals:
                    // literal only).
                    let after_dict = openflow_postproc::apply_dictionary(&raw, &dict_terms);
                    let mut identifier_result = false;
                    let after_dict = if coding_mode {
                        if let Some(ident) = openflow_postproc::apply_casing_command(&after_dict) {
                            identifier_result = true;
                            ident
                        } else {
                            openflow_postproc::apply_coding_grammar(&after_dict)
                        }
                    } else {
                        openflow_postproc::tidy_transcript(&after_dict)
                    };
                    let final_text = if let Some(expansion) =
                        openflow_postproc::match_snippet(&after_dict, &snippet_defs)
                    {
                        expansion.to_string()
                    } else if identifier_result || terminal_mode {
                        // Identifiers and terminal input stay literal.
                        after_dict.clone()
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

                    // Trailing space so consecutive dictations chain naturally
                    // (but not after identifiers — code continues immediately).
                    let text = if identifier_result {
                        final_text.trim_end().to_string()
                    } else {
                        format!("{} ", final_text.trim_end())
                    };
                    timing.chars = text.chars().count();
                    let inject_started = Instant::now();
                    let injector: &mut dyn Injector = if terminal_mode {
                        match self.services.terminal_injector.as_deref_mut() {
                            Some(t) => t,
                            None => self.services.injector.as_mut(),
                        }
                    } else {
                        self.services.injector.as_mut()
                    };
                    match injector.inject(&text) {
                        Ok(()) => {
                            self.last_injection = Some(text.trim().to_string());
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
            }
        }
        (self.callbacks.on_status)(Status::Idle);
    }

    /// Decode options for the current session, built at key-down so the
    /// chunk finalizer and the release-time tail pass agree. STT bias:
    /// dictionary words + entities from on-screen text (§7).
    fn build_opts(&self) -> TranscribeOpts {
        let (dict_terms, _) = self.personalization();
        let mut bias_parts: Vec<String> = Vec::new();
        if let Some(bias) = openflow_postproc::dictionary::bias_prompt(&dict_terms) {
            bias_parts.push(bias);
        }
        if let Some(ctx) = self.session_context.as_ref() {
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
        TranscribeOpts {
            language: match self.config.language.as_str() {
                "auto" => None,
                lang => Some(lang.to_string()),
            },
            initial_prompt: if bias_parts.is_empty() {
                None
            } else {
                Some(bias_parts.join(", "))
            },
            threads: None,
        }
    }

    /// Try to interpret the utterance as a voice command (design doc §10).
    /// Returns true if it was a command (handled), false to fall through to
    /// normal dictation.
    fn try_run_command(&mut self, raw: &str, ctx: Option<&SessionContext>) -> bool {
        let Some(runtime) = self.services.commands.as_ref() else {
            return false;
        };
        let selection = ctx.and_then(|c| c.text.selection.clone());
        let operand = selection.clone().or_else(|| self.last_injection.clone());
        let has_operand = operand.is_some();

        let Some(m) = openflow_llm::match_command(raw, &runtime.commands, has_operand) else {
            return false;
        };

        // Assistant commands use the spoken arg as the question; transforms use
        // the operand text.
        let input = if m.command.output == openflow_llm::OutputMode::Popover {
            m.arg
                .clone()
                .unwrap_or_else(|| operand.clone().unwrap_or_default())
        } else {
            operand.clone().unwrap_or_default()
        };

        let output =
            match openflow_llm::run_command(runtime.backend.as_ref(), &m, &input, &self.config.llm)
            {
                Ok(o) => o,
                Err(e) => {
                    tracing::warn!(error = %e, command = m.command.name, "command failed");
                    if let Some(flash) = self.callbacks.on_flash.as_ref() {
                        flash("Command failed — is Ollama running?".into());
                    }
                    return true; // still a command; don't fall through to dictation
                }
            };

        match m.command.output {
            openflow_llm::OutputMode::Popover => {
                if let Some(on_assistant) = self.callbacks.on_assistant.as_ref() {
                    on_assistant(output);
                }
            }
            openflow_llm::OutputMode::Replace | openflow_llm::OutputMode::Insert => {
                // Replace: overwrite the selection by pasting over it (the
                // selection is still active in the target app).
                if self.services.injector.inject(&output).is_ok() {
                    self.last_injection = Some(output.clone());
                    if let Some(flash) = self.callbacks.on_flash.as_ref() {
                        let n = input.split_whitespace().count();
                        flash(format!("{} · {n} words", m.command.name));
                    }
                }
            }
        }
        true
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
                stt: Arc::new(Mutex::new(Box::new(stt) as Box<dyn SttEngine>)),
                vad: None,
                injector: Box::new(injector.clone()),
                partials: None,
                finalizer: None,
                stt_factory: None,
                audio_factory: None,
                cleanup,
                store,
                context: None,
                terminal_injector: None,
                commands: None,
            },
            config,
            Callbacks {
                on_status: Box::new(move |s| statuses2.lock().unwrap().push(s)),
                on_timing: Some(Box::new(move |t| timings2.lock().unwrap().push(t))),
                on_assistant: None,
                on_flash: None,
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

        // Prose passes through the deterministic tidy: capitalized + closed.
        assert_eq!(h.injected(), vec!["Hello world. ".to_string()]);
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
        assert_eq!(timings[0].chars, "Hello world. ".chars().count());
    }

    #[test]
    fn toggle_mode_uses_two_presses_and_ignores_releases() {
        let mut h = start_harness(HotkeyMode::Toggle, FakeStt::with_responses(["toggled"]));
        h.handle.post(SessionEvent::HotkeyPressed);
        h.handle.post(SessionEvent::HotkeyReleased); // must be ignored
        std::thread::sleep(Duration::from_millis(400));
        h.handle.post(SessionEvent::HotkeyPressed);
        h.shutdown();

        assert_eq!(h.injected(), vec!["Toggled. ".to_string()]);
    }

    #[test]
    fn mode_change_at_runtime_takes_effect_immediately() {
        // Regression guard for "changing Mode in Settings does nothing":
        // start in Hold, hot-swap the config to Toggle, and verify the very
        // next session follows toggle semantics (release ignored, second
        // press finishes).
        let mut h = start_harness(HotkeyMode::Hold, FakeStt::with_responses(["switched"]));
        let mut toggled = Config::default();
        toggled.hotkey.mode = HotkeyMode::Toggle;
        toggled.audio.preroll_ms = 0;
        h.handle
            .post(SessionEvent::ConfigChanged(Box::new(toggled)));

        h.handle.post(SessionEvent::HotkeyPressed);
        h.handle.post(SessionEvent::HotkeyReleased); // toggle: must be ignored
        std::thread::sleep(Duration::from_millis(400));
        h.handle.post(SessionEvent::HotkeyPressed); // toggle: finishes
        h.shutdown();

        assert_eq!(h.injected(), vec!["Switched. ".to_string()]);
    }

    #[test]
    fn hands_free_auto_stops_after_trailing_silence() {
        // One second of loud speech, then endless silence, paced at real
        // time so the speech lands *inside* the session (a CPU-speed flood
        // would exhaust the noise before the hotkey press arrives). In
        // hands-free mode the orchestrator must finish on its own — no
        // release, no second press.
        let mut config = Config::default();
        config.hotkey.mode = HotkeyMode::HandsFree;
        config.audio.preroll_ms = 0;
        let audio = AudioEngine::new(Box::new(
            FakeAudioSource::new(one_second_of_noise()).realtime(true),
        ));
        let injector = FakeInjector::default();
        let statuses = Arc::new(Mutex::new(Vec::new()));
        let statuses2 = statuses.clone();
        let (orch, handle) = Orchestrator::new(
            Services {
                audio,
                stt: Arc::new(Mutex::new(
                    Box::new(FakeStt::with_responses(["hands free works"])) as Box<dyn SttEngine>,
                )),
                vad: None,
                injector: Box::new(injector.clone()),
                partials: None,
                finalizer: None,
                stt_factory: None,
                audio_factory: None,
                cleanup: None,
                store: None,
                context: None,
                terminal_injector: None,
                commands: None,
            },
            config,
            Callbacks {
                on_status: Box::new(move |s| statuses2.lock().unwrap().push(s)),
                on_timing: None,
                on_assistant: None,
                on_flash: None,
            },
        );
        let thread = std::thread::spawn(move || orch.run());
        handle.post(SessionEvent::HotkeyPressed);

        // ~1 s of speech + 2 s silence hold + tick cadence: allow up to 10 s.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while injector.injected.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        handle.post(SessionEvent::Shutdown);
        thread.join().unwrap();

        assert_eq!(
            injector.injected.lock().unwrap().clone(),
            vec!["Hands free works. ".to_string()]
        );
        assert_eq!(*statuses.lock().unwrap().last().unwrap(), Status::Idle);
    }

    #[test]
    fn hud_start_and_finish_dictate_like_a_hotkey() {
        let mut h = start_harness(HotkeyMode::Hold, FakeStt::with_responses(["from the pill"]));
        h.handle.post(SessionEvent::HudStart);
        std::thread::sleep(Duration::from_millis(400));
        h.handle.post(SessionEvent::HudFinish);
        h.shutdown();

        assert_eq!(h.injected(), vec!["From the pill. ".to_string()]);
    }

    #[test]
    fn hud_cancel_discards_the_session() {
        let mut h = start_harness(HotkeyMode::Hold, FakeStt::with_responses(["after cancel"]));
        h.handle.post(SessionEvent::HudStart);
        std::thread::sleep(Duration::from_millis(400));
        h.handle.post(SessionEvent::HudCancel);
        // The cancelled session reaches STT/injection zero times; a later
        // dictation still works.
        std::thread::sleep(Duration::from_millis(100));
        h.handle.post(SessionEvent::HotkeyPressed);
        std::thread::sleep(Duration::from_millis(400));
        h.handle.post(SessionEvent::HotkeyReleased);
        h.shutdown();

        assert_eq!(h.injected(), vec!["After cancel. ".to_string()]);
        assert!(h.timings.lock().unwrap().len() == 1);
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

        assert_eq!(h.injected(), vec!["Run kubectl apply. ".to_string()]);
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
    fn voice_command_transforms_selection_and_skips_dictation() {
        // Selection present + "make it shorter" → command runs, its output
        // replaces the selection; the raw utterance is NOT dictated.
        let command_llm = openflow_llm::FakeLlm::with_responses(["Ship Monday."]);
        let context = openflow_context::FakeContext {
            app: Some(openflow_context::AppInfo {
                process: "notepad.exe".into(),
                title: "Untitled".into(),
            }),
            text: openflow_context::TextContext {
                preceding_text: None,
                selection: Some("We should probably ship it on Monday I think".into()),
            },
        };
        let injector = FakeInjector::default();
        let assistant_calls = Arc::new(Mutex::new(Vec::<String>::new()));

        let mut config = Config::default();
        config.audio.preroll_ms = 0;
        let audio = AudioEngine::new(Box::new(FakeAudioSource::new(one_second_of_noise())));
        let (orch, handle) = Orchestrator::new(
            Services {
                audio,
                stt: Arc::new(Mutex::new(
                    Box::new(FakeStt::with_responses(["make it shorter"])) as Box<dyn SttEngine>,
                )),
                vad: None,
                injector: Box::new(injector.clone()),
                partials: None,
                finalizer: None,
                stt_factory: None,
                audio_factory: None,
                cleanup: None,
                store: None,
                context: Some(Box::new(context)),
                terminal_injector: None,
                commands: Some(CommandRuntime {
                    backend: Box::new(command_llm.clone()),
                    commands: openflow_llm::builtin_commands(),
                }),
            },
            config,
            Callbacks {
                on_status: Box::new(|_| {}),
                on_timing: None,
                on_assistant: None,
                on_flash: None,
            },
        );
        let thread = std::thread::spawn(move || orch.run());
        handle.post(SessionEvent::HotkeyPressed);
        std::thread::sleep(Duration::from_millis(400));
        handle.post(SessionEvent::HotkeyReleased);
        handle.post(SessionEvent::Shutdown);
        thread.join().unwrap();

        let _ = assistant_calls;
        // The command's output replaced the selection (no trailing space added
        // by dictation path — command injects verbatim).
        assert_eq!(
            injector.injected.lock().unwrap().clone(),
            vec!["Ship Monday.".to_string()]
        );
        // The LLM saw the selection, not the trigger phrase.
        let calls = command_llm.calls.lock().unwrap();
        assert!(calls[0].1[0].content.contains("ship it on Monday"));
    }

    #[test]
    fn coding_profile_applies_casing_and_terminal_skips_llm() {
        // Terminal context: casing command → identifier, no LLM, terminal
        // injector used, no trailing space.
        let fake_llm = openflow_llm::FakeLlm::with_responses(["LLM MUST NOT RUN"]);
        let cleanup = openflow_llm::CleanupService::new(Box::new(fake_llm.clone()));
        let terminal_injector = FakeInjector::default();
        let context = openflow_context::FakeContext {
            app: Some(openflow_context::AppInfo {
                process: "windowsterminal.exe".into(),
                title: "pwsh".into(),
            }),
            text: openflow_context::TextContext::default(),
        };

        let mut config = Config::default();
        config.audio.preroll_ms = 0;
        let audio = AudioEngine::new(Box::new(FakeAudioSource::new(one_second_of_noise())));
        let (orch, handle) = Orchestrator::new(
            Services {
                audio,
                stt: Arc::new(Mutex::new(Box::new(FakeStt::with_responses([
                    "get user by id camel case",
                ])) as Box<dyn SttEngine>)),
                vad: None,
                injector: Box::new(FakeInjector::default()),
                partials: None,
                finalizer: None,
                stt_factory: None,
                audio_factory: None,
                cleanup: Some(cleanup),
                store: None,
                context: Some(Box::new(context)),
                terminal_injector: Some(Box::new(terminal_injector.clone())),
                commands: None,
            },
            config,
            Callbacks {
                on_status: Box::new(|_| {}),
                on_timing: None,
                on_assistant: None,
                on_flash: None,
            },
        );
        let thread = std::thread::spawn(move || orch.run());
        handle.post(SessionEvent::HotkeyPressed);
        std::thread::sleep(Duration::from_millis(400));
        handle.post(SessionEvent::HotkeyReleased);
        handle.post(SessionEvent::Shutdown);
        thread.join().unwrap();

        assert!(
            fake_llm.calls.lock().unwrap().is_empty(),
            "LLM ran in terminal mode"
        );
        assert_eq!(
            terminal_injector.injected.lock().unwrap().clone(),
            vec!["getUserById".to_string()]
        );
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
                stt: Arc::new(Mutex::new(Box::new(FakeStt::with_responses([
                    "um hey ship it monday",
                ])) as Box<dyn SttEngine>)),
                vad: None,
                injector: Box::new(injector.clone()),
                partials: None,
                finalizer: None,
                stt_factory: None,
                audio_factory: None,
                cleanup: Some(cleanup),
                store: None,
                context: Some(Box::new(context)),
                terminal_injector: None,
                commands: None,
            },
            config,
            Callbacks {
                on_status: Box::new(|_| {}),
                on_timing: None,
                on_assistant: None,
                on_flash: None,
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
            match (seed >> 33) % 6 {
                0 => h.handle.post(SessionEvent::HotkeyPressed),
                1 => h.handle.post(SessionEvent::HotkeyReleased),
                2 => h.handle.post(SessionEvent::HudStart),
                3 => h.handle.post(SessionEvent::HudFinish),
                4 => h.handle.post(SessionEvent::HudCancel),
                _ => h.handle.post(SessionEvent::ConfigChanged(Box::default())),
            }
        }
        h.shutdown(); // hangs (test timeout) if the loop wedged
        let last = *h.statuses.lock().unwrap().last().unwrap();
        assert_eq!(last, Status::Idle);
    }
}
