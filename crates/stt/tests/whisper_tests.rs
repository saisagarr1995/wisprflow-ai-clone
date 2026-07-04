//! Real-model test. Ignored by default because it needs a downloaded Whisper
//! model. Run manually:
//!   $env:OPENFLOW_WHISPER_MODEL = "path\to\ggml-tiny.en.bin"
//!   cargo test -p openflow-stt -- --ignored

use std::path::{Path, PathBuf};

use openflow_stt::{SttEngine, TranscribeOpts, WhisperEngine};

fn load_jfk() -> Vec<f32> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../audio/tests/fixtures/jfk.wav");
    let mut reader = hound::WavReader::open(path).unwrap();
    reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect()
}

fn model_path() -> Option<PathBuf> {
    std::env::var_os("OPENFLOW_WHISPER_MODEL").map(PathBuf::from)
}

#[test]
#[ignore = "requires a downloaded whisper model (set OPENFLOW_WHISPER_MODEL)"]
fn transcribes_jfk_with_reasonable_accuracy() {
    let model = model_path().expect("set OPENFLOW_WHISPER_MODEL to run this test");
    let mut engine = WhisperEngine::from_file(&model).unwrap();
    let transcript = engine
        .transcribe(&load_jfk(), &TranscribeOpts { language: Some("en".into()), ..Default::default() })
        .unwrap();

    let text = transcript.text.to_lowercase();
    println!("transcript: {text}");
    // The famous line; require the load-bearing words.
    for word in ["ask", "country", "americans"] {
        assert!(text.contains(word), "missing '{word}' in: {text}");
    }
}
