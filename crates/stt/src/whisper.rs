use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::{SttEngine, SttError, TranscribeOpts, Transcript};

/// whisper.cpp backend. CPU-only for now; GPU feature flags land with the
/// performance pass (design doc §11) so early phases stay easy to build.
pub struct WhisperEngine {
    context: WhisperContext,
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
        let model_name = model_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".into());
        Ok(Self {
            context,
            model_name,
        })
    }
}

impl SttEngine for WhisperEngine {
    fn transcribe(&mut self, audio: &[f32], opts: &TranscribeOpts) -> Result<Transcript, SttError> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_translate(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        // Utterances are a single speaker pressing a hotkey: one segment
        // context, no need for the token history of previous calls.
        params.set_no_context(true);
        if let Some(lang) = opts.language.as_deref() {
            params.set_language(Some(lang));
        }
        if let Some(prompt) = opts.initial_prompt.as_deref() {
            params.set_initial_prompt(prompt);
        }

        let mut state = self
            .context
            .create_state()
            .map_err(|e| SttError::Inference(e.to_string()))?;
        state
            .full(params, audio)
            .map_err(|e| SttError::Inference(e.to_string()))?;

        let mut text = String::new();
        for segment in state.as_iter() {
            let piece = segment
                .to_str_lossy()
                .map_err(|e| SttError::Inference(e.to_string()))?;
            text.push_str(&piece);
        }

        Ok(Transcript {
            text: text.trim().to_string(),
            language: opts.language.clone(),
        })
    }

    fn describe(&self) -> String {
        format!("whisper.cpp {} (CPU)", self.model_name)
    }
}
