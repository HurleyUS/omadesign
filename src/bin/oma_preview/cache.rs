//! Content-addressed preview storage, including font dependencies.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Create the private preview cache.
///
/// Takes no arguments. Returns the user cache path, or a filesystem error.
pub fn root() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or("No user cache directory is available")?;
    let root = base.join("flea-omadesign");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let stale = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.elapsed().ok())
                .is_some_and(|age| age.as_secs() > 180);
            if path
                .extension()
                .is_some_and(|extension| extension == "part")
                && stale
            {
                let _ = fs::remove_file(path);
            }
        }
    }
    Ok(root)
}

/// Identify a preview by its document, renderer, settings, and fonts.
///
/// `input` names the native artwork, `mode` selects thumbnail or preview, and
/// `edge` sets resolution. Returns a hash that changes when any dependency changes.
pub fn key(input: &Path, mode: &str, edge: u32) -> Result<String, String> {
    let input = input.canonicalize().map_err(|e| e.to_string())?;
    let metadata = fs::metadata(&input).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
        return Err("Choose a regular document smaller than 128 MiB".into());
    }
    let mut digest = Sha256::new();
    digest.update(format!(
        "oma-preview-0.1.0:{mode}:{edge}:{}",
        input.display()
    ));
    let bytes = fs::read(&input).map_err(|e| e.to_string())?;
    if bytes.len() > 128 * 1024 * 1024 {
        return Err("The document exceeds 128 MiB".into());
    }
    digest.update(&bytes);
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    fonts(&value, &mut digest);
    if let Some(parent) = input.parent() {
        for ancestor in parent.ancestors() {
            let directory = ancestor.join(".omabrand/fonts");
            if directory.is_dir() {
                directory_hash(&directory, &mut digest, 0)?;
                break;
            }
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn fonts(value: &Value, digest: &mut Sha256) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "font" {
                    if let Some(path) = value.as_str() {
                        if let Ok(bytes) = fs::read(path) {
                            digest.update(&bytes);
                        }
                    }
                } else {
                    fonts(value, digest);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                fonts(value, digest);
            }
        }
        _ => {}
    }
}

fn directory_hash(path: &Path, digest: &mut Sha256, depth: u32) -> Result<(), String> {
    if depth > 8 {
        return Err("Font folders are nested too deeply".into());
    }
    let mut entries: Vec<_> = fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for entry in entries {
        let metadata = fs::symlink_metadata(&entry).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        digest.update(entry.to_string_lossy().as_bytes());
        if metadata.is_dir() {
            directory_hash(&entry, digest, depth + 1)?;
        } else if metadata.len() < 32 * 1024 * 1024 {
            digest.update(fs::read(&entry).map_err(|e| e.to_string())?);
        }
    }
    Ok(())
}

/// Reuse a complete cached render.
///
/// `manifest` is its JSON record. Returns that record only while its image exists.
pub fn answer(manifest: &Path) -> Option<String> {
    let answer = fs::read_to_string(manifest).ok()?;
    let value: Value = serde_json::from_str(&answer).ok()?;
    Path::new(value.get("file")?.as_str()?)
        .is_file()
        .then_some(answer)
}

/// Keep the preview cache within its disk allowance.
///
/// `root` holds the cache and `keep` protects the render just completed. Removes
/// the oldest images and their records until their total fits 512 MiB. Returns nothing.
pub fn trim(root: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let path = entry.path();
            matches!(path.extension()?.to_str()?, "png" | "mp4").then_some((path, metadata))
        })
        .collect();
    entries.sort_by_key(|(_, metadata)| metadata.modified().ok());
    let mut size: u64 = entries.iter().map(|(_, metadata)| metadata.len()).sum();
    for (path, metadata) in entries {
        if size <= 512 * 1024 * 1024 {
            break;
        }
        if path == keep {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            let _ = fs::remove_file(path.with_extension("json"));
            size = size.saturating_sub(metadata.len());
        }
    }
}
