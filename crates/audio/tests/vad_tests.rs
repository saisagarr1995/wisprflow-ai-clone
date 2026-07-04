//! Integration tests against the real Silero model (committed in
//! `models/silero_vad.onnx`) and a real speech fixture (JFK clip from
//! whisper.cpp, public domain).

use std::path::{Path, PathBuf};

use openflow_audio::{
    trim_silence, SileroVad, Vad, SPEECH_THRESHOLD, TARGET_SAMPLE_RATE, VAD_FRAME_SAMPLES,
};

fn model_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("models/silero_vad.onnx")
}

fn load_jfk() -> Vec<f32> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jfk.wav");
    let mut reader = hound::WavReader::open(path).unwrap();
    assert_eq!(reader.spec().sample_rate, TARGET_SAMPLE_RATE);
    assert_eq!(reader.spec().channels, 1);
    reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect()
}

#[test]
fn silence_has_low_probability() {
    let mut vad = SileroVad::from_file(&model_path()).unwrap();
    let silence = vec![0.0f32; VAD_FRAME_SAMPLES];
    for _ in 0..5 {
        let p = vad.speech_prob(&silence).unwrap();
        assert!(p < 0.1, "silence scored {p}");
    }
}

#[test]
fn speech_has_high_probability() {
    let mut vad = SileroVad::from_file(&model_path()).unwrap();
    let audio = load_jfk();
    // Probe frames from the middle of the clip, where speech is ongoing.
    let mid = audio.len() / 2;
    let mut max_p: f32 = 0.0;
    for frame in audio[mid..mid + 20 * VAD_FRAME_SAMPLES].chunks_exact(VAD_FRAME_SAMPLES) {
        max_p = max_p.max(vad.speech_prob(frame).unwrap());
    }
    assert!(max_p > 0.5, "peak speech prob was only {max_p}");
}

#[test]
fn trim_removes_padded_silence() {
    let mut vad = SileroVad::from_file(&model_path()).unwrap();
    let speech = load_jfk();
    // Wrap the clip in 2 s of silence on each side.
    let pad = vec![0.0f32; 2 * TARGET_SAMPLE_RATE as usize];
    let mut audio = pad.clone();
    audio.extend_from_slice(&speech);
    audio.extend_from_slice(&pad);

    let range = trim_silence(&mut vad, &audio, SPEECH_THRESHOLD, 100).unwrap();
    assert!(!range.is_empty(), "speech should be detected");
    // >90% of each 2 s silence pad must be outside the kept range.
    let pad_len = pad.len();
    assert!(
        range.start > pad_len * 9 / 10,
        "leading silence kept: starts at {}",
        range.start
    );
    assert!(
        range.end < audio.len() - pad_len * 9 / 10,
        "trailing silence kept: ends at {} of {}",
        range.end,
        audio.len()
    );
    // And the speech itself survives.
    assert!(range.end - range.start >= speech.len() * 8 / 10);
}

#[test]
fn pure_silence_yields_empty_range() {
    let mut vad = SileroVad::from_file(&model_path()).unwrap();
    let silence = vec![0.0f32; 3 * TARGET_SAMPLE_RATE as usize];
    let range = trim_silence(&mut vad, &silence, SPEECH_THRESHOLD, 100).unwrap();
    assert!(range.is_empty());
}
