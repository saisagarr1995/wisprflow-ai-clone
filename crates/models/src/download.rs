use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::registry::find;
use crate::{models_dir, ModelError};

#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
}

/// Ensure a registry model is present and checksum-verified in the models
/// dir; download (resuming any partial file) if not. Returns the final path.
///
/// Blocking; callers run it on a worker thread and forward `on_progress`
/// events to the UI.
pub fn ensure(
    model_id: &str,
    on_progress: impl FnMut(DownloadProgress),
) -> Result<PathBuf, ModelError> {
    let entry = find(model_id).ok_or_else(|| ModelError::UnknownModel(model_id.into()))?;
    ensure_entry(entry, &models_dir(), on_progress)
}

/// Registry-independent core, so tests can point entries at a local server.
pub fn ensure_entry(
    entry: &crate::ModelEntry,
    dir: &Path,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<PathBuf, ModelError> {
    fs::create_dir_all(dir)?;
    let target = dir.join(&entry.file);

    if target.exists() && file_sha256(&target)? == entry.sha256 {
        return Ok(target);
    }

    let part = target.with_extension(format!("{}.part", ext_of(&entry.file)));
    let mut start_at = part.metadata().map(|m| m.len()).unwrap_or(0);
    // A partial file larger than the artifact is garbage — start over.
    if start_at >= entry.size_bytes {
        let _ = fs::remove_file(&part);
        start_at = 0;
    }

    download_into(
        &entry.url,
        &part,
        start_at,
        entry.size_bytes,
        &mut on_progress,
    )?;

    let actual = file_sha256(&part)?;
    if actual != entry.sha256 {
        // Corrupt or upstream changed: remove so the next attempt is clean.
        let _ = fs::remove_file(&part);
        return Err(ModelError::ChecksumMismatch {
            file: entry.file.clone(),
            expected: entry.sha256.clone(),
            actual,
        });
    }
    fs::rename(&part, &target)?;
    tracing::info!(model = entry.id, path = %target.display(), "model ready");
    Ok(target)
}

fn ext_of(file: &str) -> &str {
    Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
}

fn download_into(
    url: &str,
    part: &Path,
    start_at: u64,
    total: u64,
    on_progress: &mut impl FnMut(DownloadProgress),
) -> Result<(), ModelError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None) // large files; progress callback is the liveness signal
        .build()
        .map_err(|e| ModelError::Download(e.to_string()))?;

    let mut req = client.get(url);
    if start_at > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={start_at}-"));
    }
    let mut resp = req
        .send()
        .map_err(|e| ModelError::Download(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(ModelError::Download(format!("{} for {url}", resp.status())));
    }
    // Server ignored the Range header (200 instead of 206): restart from zero.
    let mut downloaded = if start_at > 0 && resp.status() == reqwest::StatusCode::OK {
        0
    } else {
        start_at
    };

    // truncate(false): resuming appends after the seek; set_len below trims.
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(part)?;
    file.seek(SeekFrom::Start(downloaded))?;
    file.set_len(downloaded)?;

    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = resp
            .read(&mut buf)
            .map_err(|e| ModelError::Download(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        on_progress(DownloadProgress { downloaded, total });
    }
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String, ModelError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 128 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
