use serde::{Deserialize, Serialize};

/// Bump when the config layout changes incompatibly, and add a step in
/// `migrate.rs`. Version 0 means "pre-versioning" (no `version` field).
pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    /// "auto" or a BCP-47 tag like "en", "hi", "de".
    pub language: String,
    pub hotkey: HotkeyConfig,
    pub audio: AudioConfig,
    pub stt: SttConfig,
    pub llm: LlmConfig,
    pub injection: InjectionConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            language: "auto".into(),
            hotkey: HotkeyConfig::default(),
            audio: AudioConfig::default(),
            stt: SttConfig::default(),
            llm: LlmConfig::default(),
            injection: InjectionConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    /// Chord string, e.g. "RCtrl" or "Ctrl+Shift+Space". Parsed by the
    /// `openflow-hotkey` crate; stored here as the user-facing source of truth.
    pub chord: String,
    pub mode: HotkeyMode,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            chord: "RCtrl".into(),
            mode: HotkeyMode::Hold,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyMode {
    /// Push-to-talk: key down starts recording, key up finalizes.
    #[default]
    Hold,
    /// Press once to start, again to stop.
    Toggle,
    /// Hands-free: press once (or click the HUD pill) to start; recording
    /// finishes on its own after a couple of seconds of silence. A second
    /// press also stops it, like Toggle.
    HandsFree,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// `None` = system default input device.
    pub input_device: Option<String>,
    /// Audio retained from before the hotkey press, so the first syllable
    /// isn't clipped.
    pub preroll_ms: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            input_device: None,
            preroll_ms: 250,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SttConfig {
    pub engine: SttEngineKind,
    /// Model id from the `openflow-models` registry.
    pub model_id: String,
}

impl Default for SttConfig {
    fn default() -> Self {
        // base.en balances accuracy and speed on CPU. tiny is noticeably worse
        // on short/real-world phrases; small.en (in the registry) is more
        // accurate but slower — users can upgrade in Settings → Models.
        Self {
            engine: SttEngineKind::Auto,
            model_id: "whisper-base".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SttEngineKind {
    /// Route per detected language (design doc §4).
    #[default]
    Auto,
    Parakeet,
    Whisper,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    /// LLM cleanup can be disabled entirely; dictation then injects the
    /// deterministic transcript.
    pub enabled: bool,
    pub base_url: String,
    /// Hot-path model: must be small and kept resident (design doc §3).
    pub cleanup_model: String,
    /// Quality model for rewrites/commands; loaded on demand.
    pub rewrite_model: String,
    /// Optional coding-mode model (e.g. "qwen3-coder:30b").
    pub coding_model: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: "http://localhost:11434".into(),
            // A small, fast, non-thinking instruct model powers both the
            // hot-path cleanup and rewrites out of the box (one download,
            // usable on CPU-only machines). Thinking models like qwen3 are
            // far slower on CPU; users with a GPU can point these at bigger
            // models for higher quality.
            cleanup_model: "llama3.2:3b".into(),
            rewrite_model: "llama3.2:3b".into(),
            coding_model: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InjectionConfig {
    pub strategy: InjectionStrategy,
    /// Delay before restoring the saved clipboard after a paste, so slow
    /// targets (Teams, Outlook) have read the data (design doc §8).
    pub clipboard_restore_delay_ms: u64,
}

impl Default for InjectionConfig {
    fn default() -> Self {
        Self {
            strategy: InjectionStrategy::Auto,
            clipboard_restore_delay_ms: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionStrategy {
    /// Per-app table decides (paste for most apps, keystrokes for terminals).
    #[default]
    Auto,
    ClipboardPaste,
    Keystrokes,
}
