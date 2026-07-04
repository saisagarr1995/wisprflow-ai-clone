//! Deterministic post-processing that runs before/around the LLM.
//!
//! Pipeline order (design doc §2 stage 5) — later stages see earlier output:
//! 1. `snippets::match_snippet`  — trigger phrases short-circuit everything
//! 2. `dictionary::apply`        — fuzzy-replace ASR near-misses
//! 3. (coding mode: `casing`)    — Phase 5
//! 4. (intent gate)              — Phase 6
//! The LLM cleanup runs after all of these.

pub mod dictionary;
pub mod snippets;

pub use dictionary::apply as apply_dictionary;
pub use snippets::match_snippet;
