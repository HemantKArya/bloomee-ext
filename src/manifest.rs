use crate::types::Manifest;
use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn current_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

pub fn resolve_dir(dir: Option<&str>) -> Result<PathBuf> {
    let base = dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if !base.exists() {
        anyhow::bail!("Directory not found: {}", base.display());
    }
    base.canonicalize()
        .with_context(|| format!("Cannot resolve directory: {}", base.display()))
}

pub fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("manifest.json")
}

pub fn cargo_manifest_path(dir: &Path) -> PathBuf {
    dir.join("Cargo.toml")
}

pub fn load_manifest(dir: &Path) -> Result<Manifest> {
    let path = manifest_path(dir);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("{} is malformed", path.display()))
}

pub fn update_manifest_last_updated(dir: Option<&str>) -> Result<String> {
    let dir = resolve_dir(dir)?;
    let path = manifest_path(&dir);
    if !path.exists() {
        anyhow::bail!("No manifest.json found in {}", dir.display());
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    let mut value: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is malformed", path.display()))?;
    let object = value
        .as_object_mut()
        .context("manifest.json must contain a top-level JSON object")?;

    let timestamp = current_timestamp();
    object.insert("last_updated".to_string(), Value::String(timestamp.clone()));

    let formatted = format!("{}\n", serde_json::to_string_pretty(&value)?);
    fs::write(&path, formatted)
        .with_context(|| format!("Cannot write {}", path.display()))?;

    Ok(timestamp)
}