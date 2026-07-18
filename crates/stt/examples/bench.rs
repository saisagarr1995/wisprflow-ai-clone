//! Decode-speed harness: `cargo run -p openflow-stt --example bench -- <model.bin> <speech.wav>`
//!
//! Times two warm decodes of the same clip and prints ms per pass. Used to
//! compare toolchain builds of the bundled whisper.cpp (the CI-binary saga:
//! preview-MSVC output decoded 5-20x slower than v143/clang on Zen 2).

use std::path::Path;
use std::time::Instant;

use openflow_stt::{SttEngine, TranscribeOpts, WhisperEngine};

fn read_wav_16k_mono(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("read wav");
    let data_pos = bytes
        .windows(4)
        .position(|w| w == b"data")
        .expect("wav data chunk")
        + 8;
    bytes[data_pos..]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let model = args.next().expect("usage: bench <model.bin> <speech.wav>");
    let wav = args.next().expect("usage: bench <model.bin> <speech.wav>");
    let audio = read_wav_16k_mono(Path::new(&wav));
    println!("audio: {:.1} s", audio.len() as f32 / 16000.0);

    let mut engine = WhisperEngine::from_file(Path::new(&model)).expect("load model");
    engine.warm_up();

    let opts = TranscribeOpts::default();
    for pass in 1..=2 {
        let t = Instant::now();
        let out = engine.transcribe(&audio, &opts).expect("transcribe");
        println!(
            "pass {pass}: {} ms  text: {}",
            t.elapsed().as_millis(),
            out.text.trim()
        );
    }
}
