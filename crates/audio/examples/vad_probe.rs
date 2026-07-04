//! Diagnostic: print Silero speech probabilities across the JFK fixture.
//! Run: cargo run -p openflow-audio --example vad_probe

use std::path::Path;

use openflow_audio::{SileroVad, Vad, VAD_FRAME_SAMPLES};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut vad = SileroVad::from_file(&root.join("models/silero_vad.onnx")).unwrap();
    let mut reader = hound::WavReader::open(root.join("tests/fixtures/jfk.wav")).unwrap();
    let audio: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect();

    let probs: Vec<f32> = audio
        .chunks_exact(VAD_FRAME_SAMPLES)
        .map(|f| vad.speech_prob(f).unwrap())
        .collect();

    let peak = probs.iter().cloned().fold(0.0f32, f32::max);
    let above = probs.iter().filter(|&&p| p > 0.5).count();
    println!("frames: {}, peak: {peak:.3}, >0.5: {above}", probs.len());
    for (i, chunk) in probs.chunks(10).enumerate() {
        let line: String = chunk.iter().map(|p| format!("{:>5.2}", p)).collect();
        println!("{:>5.1}s {line}", i as f32 * 10.0 * 0.032);
    }
}
