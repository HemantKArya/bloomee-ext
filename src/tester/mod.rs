mod host_chart;
mod host_importer;
mod host_lyrics;
mod host_resolver;
mod host_suggestion;

use crate::types::{Manifest, PluginArchetype};
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

/// Run `bex test` in the current directory.
///
/// Reads `manifest.json`, resolves the WASM path, then dispatches to the
/// appropriate embedded host (same UX as the standalone host binaries).
pub fn run_test(wasm_override: Option<&str>) -> Result<()> {
    // ── Read manifest ─────────────────────────────────────────────────────
    let manifest_str = fs::read_to_string("manifest.json")
        .context("No manifest.json found. Run `bex build` first or cd into a plugin folder.")?;
    let manifest: Manifest =
        serde_json::from_str(&manifest_str).context("manifest.json is malformed")?;

    // ── Resolve WASM path ─────────────────────────────────────────────────
    let wasm_path: PathBuf = if let Some(p) = wasm_override {
        PathBuf::from(p)
    } else {
        PathBuf::from("target/bex/plugin.wasm")
    };

    if !wasm_path.exists() {
        anyhow::bail!(
            "Plugin WASM not found at {}.\n\
             Run `bex build` first, or pass --wasm <path>.",
            wasm_path.display()
        );
    }

    println!("=== bex test ===");
    println!("  Plugin : {}", manifest.name);
    println!("  Type   : {}", manifest.plugin_type);
    println!("  WASM   : {}", wasm_path.display());
    println!();

    // ── Dispatch to the appropriate embedded host ─────────────────────────
    match manifest.plugin_type {
        PluginArchetype::ChartProvider => host_chart::run(&wasm_path),
        PluginArchetype::LyricsProvider => host_lyrics::run(&wasm_path),
        PluginArchetype::ContentResolver => host_resolver::run(&wasm_path),
        PluginArchetype::SearchSuggestionProvider => host_suggestion::run(&wasm_path),
        PluginArchetype::ContentImporter => host_importer::run(&wasm_path),
    }
}
