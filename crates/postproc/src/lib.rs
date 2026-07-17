//! Deterministic post-processing that runs before/around the LLM.
//!
//! Pipeline order (design doc §2 stage 5) — later stages see earlier output:
//!
//! 1. `snippets::match_snippet` — trigger phrases short-circuit everything.
//! 2. `dictionary::apply` — fuzzy-replace ASR near-misses.
//! 3. Coding mode: `casing` (Phase 5); prose: `tidy` (stutters, fillers,
//!    capitalization, terminal punctuation — always on, LLM or not).
//! 4. Intent gate (Phase 6).
//!
//! The LLM cleanup runs after all of these.

pub mod casing;
pub mod dictionary;
pub mod snippets;
pub mod tidy;

pub use casing::{apply_casing_command, apply_coding_grammar};
pub use dictionary::apply as apply_dictionary;
pub use snippets::match_snippet;
pub use tidy::tidy_transcript;
