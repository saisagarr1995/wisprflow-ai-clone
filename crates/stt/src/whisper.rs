use std::path::Path;

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::{SttEngine, SttError, TranscribeOpts, Transcript};

/// whisper.cpp backend. CPU-only for now; GPU feature flags land with the
/// performance pass (design doc §11) so early phases stay easy to build.
pub struct WhisperEngine {
    /// Decode state (KV cache + compute buffers, ~100 MB for base) is
    /// allocated once and reused for every decode. Allocating it per call
    /// churned that much memory several times a second across the partial /
    /// chunk / tail decoders, which on a RAM-starved machine evicted the
    /// model weights between decodes and turned every pass into a page-fault
    /// storm (observed as 20 s+ finals on a 7 GB laptop). The state keeps
    /// the model context alive via its internal Arc.
    state: WhisperState,
    model_name: String,
}

impl WhisperEngine {
    pub fn from_file(model_path: &Path) -> Result<Self, SttError> {
        let params = WhisperContextParameters::default();
        let context = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| SttError::ModelLoad("model path is not valid UTF-8".into()))?,
            params,
        )
        .map_err(|e| SttError::ModelLoad(e.to_string()))?;
        let state = context
            .create_state()
            .map_err(|e| SttError::ModelLoad(e.to_string()))?;
        let model_name = model_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".into());
        drop(context); // state's internal Arc keeps the model alive
        Ok(Self { state, model_name })
    }
}

impl SttEngine for WhisperEngine {
    fn transcribe(&mut self, audio: &[f32], opts: &TranscribeOpts) -> Result<Transcript, SttError> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        // whisper.cpp defaults to min(4, cores), leaving half of an 8-core
        // CPU idle for the latency-critical final pass.
        let threads = opts.threads.unwrap_or_else(|| {
            (std::thread::available_parallelism().map_or(4, |n| n.get() as i32) - 1).max(2)
        });
        params.set_n_threads(threads);
        params.set_translate(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        // Utterances are a single speaker pressing a hotkey: one segment
        // context, no need for the token history of previous calls.
        params.set_no_context(true);
        // Never re-decode at higher temperatures on low confidence: the
        // fallback ladder can multiply latency ~5x on hard audio, and
        // dictation always prefers the fast greedy pass.
        params.set_temperature_inc(0.0);
        if let Some(lang) = opts.language.as_deref() {
            params.set_language(Some(lang));
        }
        if let Some(prompt) = opts.initial_prompt.as_deref() {
            params.set_initial_prompt(prompt);
        }

        self.state
            .full(params, audio)
            .map_err(|e| SttError::Inference(e.to_string()))?;

        let mut text = String::new();
        for segment in self.state.as_iter() {
            let piece = segment
                .to_str_lossy()
                .map_err(|e| SttError::Inference(e.to_string()))?;
            text.push_str(&piece);
        }

        Ok(Transcript {
            text: strip_annotations(&text),
            language: opts.language.clone(),
        })
    }

    fn describe(&self) -> String {
        format!("whisper.cpp {} (CPU)", self.model_name)
    }

    /// A real decode over half a second of silence pages in the weights and
    /// initializes mel/compute scratch, so the first dictation is not cold.
    fn warm_up(&mut self) {
        let silence = vec![0.0f32; 8_000]; // 0.5 s at 16 kHz
        let opts = TranscribeOpts {
            threads: Some(2),
            ..TranscribeOpts::default()
        };
        if let Err(e) = self.transcribe(&silence, &opts) {
            tracing::debug!(error = %e, "warm-up decode failed");
        }
    }
}

/// Whisper emits non-speech annotations as ordinary segment text —
/// "[BLANK_AUDIO]", "[ Silence ]", "(sighs)", "♪". For dictation they are
/// noise: they must never reach the HUD partial bubble or be injected.
/// Square-bracket chunks and music notes are always annotations and are
/// removed anywhere; parenthesised/starred text is only dropped when it is
/// the *entire* transcript, so legitimate mid-sentence parentheses survive.
fn strip_annotations(text: &str) -> String {
    let t = text.trim();
    let whole = (t.starts_with('(') && t.ends_with(')'))
        || (t.starts_with('*') && t.ends_with('*') && t.len() > 1);
    if whole {
        return String::new();
    }
    let mut out = String::with_capacity(t.len());
    let mut depth = 0usize;
    for c in t.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '♪' => {}
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::strip_annotations;

    #[test]
    fn drops_blank_audio_and_bracket_annotations() {
        assert_eq!(strip_annotations("[BLANK_AUDIO]"), "");
        assert_eq!(strip_annotations(" [ Silence ] "), "");
        assert_eq!(
            strip_annotations("hello [BLANK_AUDIO] world"),
            "hello world"
        );
        assert_eq!(strip_annotations("♪"), "");
    }

    #[test]
    fn drops_whole_string_sound_events_only() {
        assert_eq!(strip_annotations("(sighs)"), "");
        assert_eq!(strip_annotations("*cough*"), "");
        assert_eq!(
            strip_annotations("keep (this) inline"),
            "keep (this) inline"
        );
    }

    #[test]
    fn passes_plain_speech_through() {
        assert_eq!(
            strip_annotations("  send the report today  "),
            "send the report today"
        );
    }
}
