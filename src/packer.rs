use crate::manifest;
use crate::types::Manifest;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use std::fs;
use std::fs::File;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use zstd::stream::write::Encoder as ZstdEncoder;

/// Pack the plugin at `dir` (or current directory when `dir` is None).
pub fn run_pack(dir: Option<&str>) -> Result<()> {
    let dir = manifest::resolve_dir(dir)?;

    println!("Packaging BEX Plugin...\n");

    let manifest_path = manifest::manifest_path(&dir);
    if !manifest_path.exists() {
        anyhow::bail!("No manifest.json found.");
    }

    let plugin_manifest = manifest::load_manifest(&dir)?;
    let archive_name = archive_name_for_manifest(&plugin_manifest);
    let archive_path = dir.join(&archive_name);

    println!("Creating archive {}...", archive_name);
    pack_plugin_dir(&dir, &archive_path)?;
    println!("Success! Packed to: {}", archive_path.display());

    Ok(())
}

/// Build all plugins found under `root` in release mode, pack each, and move
/// the resulting `.bex` files into `<root>/plugins/`.
pub fn run_pack_all(root: Option<&str>, output_dir: Option<&str>, skip_failures: bool) -> Result<()> {
    let root_dir = manifest::resolve_dir(root)?;
    let plugin_dirs = find_plugin_dirs(&root_dir)?;
    if plugin_dirs.is_empty() {
        println!("No plugin directories found in {}.", root_dir.display());
        return Ok(());
    }

    println!("Found {} plugin(s):", plugin_dirs.len());
    for dir in &plugin_dirs {
        let rel = dir.strip_prefix(&root_dir).unwrap_or(dir);
        println!("  - {}", rel.display());
    }
    println!();

    let out_dir = resolve_output_dir(&root_dir, output_dir);
    fs::create_dir_all(&out_dir)?;

    // Accumulates manifest + archive_name for each successfully packed plugin.
    // Written as bex-factory.json — the index without download_url.
    let mut packed_index: Vec<(Manifest, String)> = Vec::new();

    let mut report = PackAllReport {
        generated_at: manifest::current_timestamp(),
        root_dir: root_dir.display().to_string(),
        output_dir: out_dir.display().to_string(),
        succeeded: 0,
        failed: 0,
        plugins: Vec::new(),
    };

    for dir in &plugin_dirs {
        let rel = dir.strip_prefix(&root_dir).unwrap_or(dir);
        println!("── {} ─────────────────────────────────────────────────────", rel.display());

        let plugin_manifest = match manifest::load_manifest(dir) {
            Ok(plugin_manifest) => plugin_manifest,
            Err(error) => {
                record_failure(&mut report, rel, None, "manifest_failed", error.to_string());
                eprintln!("  ✗ Manifest failed: {error}");
                if !skip_failures {
                    break;
                }
                continue;
            }
        };

        let archive_name = archive_name_for_manifest(&plugin_manifest);
        let archive_path = out_dir.join(&archive_name);

        if let Err(error) = crate::builder::run_build_at(false, dir) {
            record_failure(
                &mut report,
                rel,
                Some((&plugin_manifest, archive_name.as_str())),
                "build_failed",
                error.to_string(),
            );
            eprintln!("  ✗ Build failed: {error}");
            if !skip_failures {
                break;
            }
            continue;
        }

        match pack_plugin_dir(dir, &archive_path) {
            Ok(()) => {
                println!("  ✓ → {}", archive_path.strip_prefix(&root_dir).unwrap_or(&archive_path).display());
                report.succeeded += 1;
                report.plugins.push(PackAllPluginReport {
                    directory: rel.display().to_string(),
                    id: Some(plugin_manifest.id.clone()),
                    name: Some(plugin_manifest.name.clone()),
                    version: Some(plugin_manifest.version.clone()),
                    archive_name: archive_name.clone(),
                    status: "packed".to_string(),
                    error: None,
                });
                packed_index.push((plugin_manifest.clone(), archive_name.clone()));
            }
            Err(error) => {
                record_failure(
                    &mut report,
                    rel,
                    Some((&plugin_manifest, archive_name.as_str())),
                    "pack_failed",
                    error.to_string(),
                );
                eprintln!("  ✗ Pack failed: {error}");
                if !skip_failures {
                    break;
                }
            }
        }
    }

    report.generated_at = manifest::current_timestamp();
    write_pack_report(&out_dir, &report)?;
    write_factory_index(&out_dir, &packed_index, &report.generated_at)?;

    println!();
    println!("── pack_all complete ─────────────────────────────────────────────");
    println!("  {} succeeded, {} failed", report.succeeded, report.failed);
    println!("  Output: {}", out_dir.display());
    println!("  Index:  {}", out_dir.join("bex-factory.json").display());
    println!("  Report: {}", out_dir.join("bex-pack-report.json").display());

    if report.failed > 0 && !skip_failures {
        anyhow::bail!(
            "pack-all stopped after {} failure(s). Re-run with --skip-failures for CI-style partial releases.",
            report.failed
        );
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct PackAllReport {
    generated_at: String,
    root_dir: String,
    output_dir: String,
    succeeded: usize,
    failed: usize,
    plugins: Vec<PackAllPluginReport>,
}

#[derive(Debug, Serialize)]
struct PackAllPluginReport {
    directory: String,
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    archive_name: String,
    status: String,
    error: Option<String>,
}

fn pack_plugin_dir(dir: &Path, archive_path: &Path) -> Result<()> {
    let manifest_path = manifest::manifest_path(dir);
    let wasm_path = dir.join("target/bex/plugin.wasm");
    if !wasm_path.exists() {
        anyhow::bail!(
            "Compiled component not found at {}. Run `bex build` first.",
            wasm_path.display()
        );
    }

    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if archive_path.exists() {
        fs::remove_file(archive_path)?;
    }

    let file = File::create(archive_path).context("Creating archive file")?;
    let mut encoder = ZstdEncoder::new(file, 22).context("Initializing Zstd encoder")?;

    {
        let mut builder = tar::Builder::new(&mut encoder);
        builder.append_path_with_name(&manifest_path, "manifest.json")?;
        builder.append_path_with_name(&wasm_path, "plugin.wasm")?;
        builder.finish()?;
    }

    encoder.finish()?;
    Ok(())
}

fn archive_name_for_manifest(plugin_manifest: &Manifest) -> String {
    let last = plugin_manifest.id.split('.').last().unwrap_or(&plugin_manifest.id);
    format!("{}.bex", last)
}

fn resolve_output_dir(root: &Path, output_dir: Option<&str>) -> PathBuf {
    match output_dir {
        Some(path) => {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        }
        None => root.join("plugins"),
    }
}

fn write_pack_report(out_dir: &Path, report: &PackAllReport) -> Result<()> {
    let report_path = out_dir.join("bex-pack-report.json");
    let json = format!("{}\n", serde_json::to_string_pretty(report)?);
    fs::write(&report_path, json)
        .with_context(|| format!("Cannot write {}", report_path.display()))
}

fn record_failure(
    report: &mut PackAllReport,
    rel: &Path,
    plugin_manifest: Option<(&Manifest, &str)>,
    status: &str,
    error: String,
) {
    report.failed += 1;
    report.plugins.push(PackAllPluginReport {
        directory: rel.display().to_string(),
        id: plugin_manifest.map(|(manifest, _)| manifest.id.clone()),
        name: plugin_manifest.map(|(manifest, _)| manifest.name.clone()),
        version: plugin_manifest.map(|(manifest, _)| manifest.version.clone()),
        archive_name: plugin_manifest
            .map(|(_, archive_name)| archive_name.to_string())
            .unwrap_or_else(|| format!("{}.bex", rel.file_name().unwrap_or_default().to_string_lossy())),
        status: status.to_string(),
        error: Some(error),
    });
}

/// Write `bex-factory.json` — the public plugin index — into `out_dir`.
/// Contains all manifest fields (minus internal ones) plus `asset_name`.
/// Intentionally has NO `download_url`; the CI workflow injects that.
fn write_factory_index(
    out_dir: &Path,
    packed: &[(Manifest, String)],
    generated_at: &str,
) -> Result<()> {
    let plugins: Vec<Value> = packed
        .iter()
        .map(|(m, archive_name)| {
            let mut entry = serde_json::to_value(m).unwrap_or(Value::Null);
            if let Some(obj) = entry.as_object_mut() {
                // Strip fields that are only meaningful at runtime, not in the index.
                obj.remove("resolver");
                obj.insert("asset_name".into(), json!(archive_name));
            }
            entry
        })
        .collect();

    let index = json!({
        "schema_version": "1.0",
        "generated_at": generated_at,
        "plugin_count": plugins.len(),
        "plugins": plugins
    });

    let index_path = out_dir.join("bex-factory.json");
    let text = format!("{}\n", serde_json::to_string_pretty(&index)?);
    fs::write(&index_path, text)
        .with_context(|| format!("writing {}", index_path.display()))
}

/// Return all directories under `root` that look like BEX plugin projects
/// (contain both `manifest.json` and `Cargo.toml`).
fn find_plugin_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::from([root.to_path_buf()]);

    while let Some(current) = queue.pop_front() {
        for entry in fs::read_dir(&current)?.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = entry.file_name();
            let name = name.to_string_lossy();
            if matches!(name.as_ref(), ".git" | "target" | "plugins" | ".venv" | "node_modules") {
                continue;
            }

            if path.join("manifest.json").exists() && path.join("Cargo.toml").exists() {
                dirs.push(path);
                // A plugin folder can contain large nested build trees (target/, vendored bex-core).
                // Avoid descending further once the plugin root is found.
                continue;
            }

            queue.push_back(path);
        }
    }

    dirs.sort();
    Ok(dirs)
}
