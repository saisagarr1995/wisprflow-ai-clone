use openflow_models::{ensure_entry, ModelEntry, ModelError, ModelKind};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

// Re-exported test server lives in the crate; simplest is to duplicate the
// tiny blob server here (integration tests can't reach cfg(test) modules).
mod blob_server;
use blob_server::BlobServer;

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn entry_for(server_url: &str, blob: &[u8], sha256: String) -> ModelEntry {
    ModelEntry {
        id: "test-model".into(),
        kind: ModelKind::Vad,
        file: "test-model.bin".into(),
        url: server_url.to_string(),
        sha256,
        size_bytes: blob.len() as u64,
        license: "MIT".into(),
        languages: "any".into(),
    }
}

#[test]
fn downloads_and_verifies() {
    let blob: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
    let server = BlobServer::serve(blob.clone(), true);
    let dir = TempDir::new().unwrap();
    let entry = entry_for(&server.url, &blob, sha256_hex(&blob));

    let mut last = 0u64;
    let path = ensure_entry(&entry, dir.path(), |p| last = p.downloaded).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), blob);
    assert_eq!(last, blob.len() as u64);
    // No leftover .part file.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn checksum_mismatch_rejects_and_cleans_up() {
    let blob = vec![7u8; 5000];
    let server = BlobServer::serve(blob.clone(), true);
    let dir = TempDir::new().unwrap();
    let entry = entry_for(&server.url, &blob, "0".repeat(64));

    let err = ensure_entry(&entry, dir.path(), |_| {}).unwrap_err();
    assert!(matches!(err, ModelError::ChecksumMismatch { .. }), "got {err:?}");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "partial not cleaned");
}

#[test]
fn resumes_partial_download() {
    let blob: Vec<u8> = (0..50_000u32).map(|i| (i % 249) as u8).collect();
    let server = BlobServer::serve(blob.clone(), true);
    let dir = TempDir::new().unwrap();
    let entry = entry_for(&server.url, &blob, sha256_hex(&blob));

    // Simulate an interrupted download: first half already on disk.
    std::fs::write(dir.path().join("test-model.bin.part"), &blob[..25_000]).unwrap();

    let mut first_progress: Option<u64> = None;
    let path = ensure_entry(&entry, dir.path(), |p| {
        first_progress.get_or_insert(p.downloaded);
    })
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), blob);
    // Progress started beyond the existing 25k, proving the Range resume.
    assert!(first_progress.unwrap() > 25_000, "resume did not happen: {first_progress:?}");
}

#[test]
fn range_ignoring_server_restarts_cleanly() {
    let blob: Vec<u8> = (0..30_000u32).map(|i| (i % 241) as u8).collect();
    let server = BlobServer::serve(blob.clone(), false); // always 200 OK
    let dir = TempDir::new().unwrap();
    let entry = entry_for(&server.url, &blob, sha256_hex(&blob));

    std::fs::write(dir.path().join("test-model.bin.part"), &blob[..10_000]).unwrap();
    let path = ensure_entry(&entry, dir.path(), |_| {}).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), blob);
}

#[test]
fn existing_valid_file_short_circuits() {
    let blob = vec![1u8; 1000];
    // Server that would fail the test if contacted: point at a closed port.
    let dir = TempDir::new().unwrap();
    let entry = entry_for("http://127.0.0.1:1/nope", &blob, sha256_hex(&blob));
    std::fs::write(dir.path().join("test-model.bin"), &blob).unwrap();

    let path = ensure_entry(&entry, dir.path(), |_| {}).unwrap();
    assert!(path.ends_with("test-model.bin"));
}
