use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::{SttEngine, SttError, Transcript, TranscribeOpts};

/// Canned-response engine for headless pipeline tests. Returns queued
/// transcripts in order (repeating the last one when exhausted) and records
/// every call's audio length and options for assertions.
#[derive(Clone, Default)]
pub struct FakeStt {
    responses: Arc<Mutex<VecDeque<String>>>,
    last: Arc<Mutex<Option<String>>>,
    pub calls: Arc<Mutex<Vec<RecordedCall>>>,
}

#[derive(Debug, Clone)]
pub struct RecordedCall {
    pub audio_samples: usize,
    pub opts: TranscribeOpts,
}

impl FakeStt {
    pub fn with_responses<S: Into<String>>(responses: impl IntoIterator<Item = S>) -> Self {
        let fake = Self::default();
        {
            let mut q = fake.responses.lock().unwrap();
            for r in responses {
                q.push_back(r.into());
            }
        }
        fake
    }
}

impl SttEngine for FakeStt {
    fn transcribe(&mut self, audio: &[f32], opts: &TranscribeOpts) -> Result<Transcript, SttError> {
        self.calls.lock().unwrap().push(RecordedCall {
            audio_samples: audio.len(),
            opts: opts.clone(),
        });
        let mut q = self.responses.lock().unwrap();
        let text = match q.pop_front() {
            Some(t) => {
                *self.last.lock().unwrap() = Some(t.clone());
                t
            }
            None => self
                .last
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "hello world".into()),
        };
        Ok(Transcript {
            text,
            language: opts.language.clone().or_else(|| Some("en".into())),
        })
    }

    fn describe(&self) -> String {
        "fake stt".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_queued_responses_then_repeats_last() {
        let mut stt = FakeStt::with_responses(["one", "two"]);
        let opts = TranscribeOpts::default();
        assert_eq!(stt.transcribe(&[0.0; 10], &opts).unwrap().text, "one");
        assert_eq!(stt.transcribe(&[0.0; 10], &opts).unwrap().text, "two");
        assert_eq!(stt.transcribe(&[0.0; 10], &opts).unwrap().text, "two");
    }

    #[test]
    fn records_calls_with_options() {
        let mut stt = FakeStt::default();
        let opts = TranscribeOpts {
            language: Some("hi".into()),
            initial_prompt: Some("kubectl".into()),
        };
        stt.transcribe(&[0.0; 1600], &opts).unwrap();
        let calls = stt.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].audio_samples, 1600);
        assert_eq!(calls[0].opts.language.as_deref(), Some("hi"));
        assert_eq!(calls[0].opts.initial_prompt.as_deref(), Some("kubectl"));
    }
}
