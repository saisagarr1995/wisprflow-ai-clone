use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Stt,
    Vad,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub kind: ModelKind,
    /// File name inside the models dir.
    pub file: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub license: String,
    pub languages: String,
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    model: Vec<ModelEntry>,
}

/// The compiled-in registry. Parsing happens once; the TOML is validated by
/// a unit test so a bad edit fails CI, not first use.
pub fn registry() -> &'static [ModelEntry] {
    static REGISTRY: OnceLock<Vec<ModelEntry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let raw = include_str!("../registry.toml");
        toml::from_str::<RegistryFile>(raw)
            .expect("registry.toml is invalid — fix the file, tests cover this")
            .model
    })
}

pub fn find(id: &str) -> Option<&'static ModelEntry> {
    registry().iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parses_and_is_sane() {
        let models = registry();
        assert!(models.len() >= 3);
        for m in models {
            assert_eq!(m.sha256.len(), 64, "{} sha256 malformed", m.id);
            assert!(m.url.starts_with("https://"), "{} url must be https", m.id);
            assert!(m.size_bytes > 0);
            assert!(
                !m.file.contains(['/', '\\']),
                "{} file must be a bare name",
                m.id
            );
        }
    }

    #[test]
    fn known_ids_resolve() {
        assert!(find("whisper-tiny").is_some());
        assert!(find("silero-vad-v5").is_some());
        assert!(find("nope").is_none());
    }
}
