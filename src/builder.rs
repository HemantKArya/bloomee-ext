use crate::manifest;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Run `bex build` in the given directory (or the current directory if `dir` is None).
///
/// Steps:
///   1. (Optionally) chdir to the plugin directory
///   2. Check for manifest.json + Cargo.toml
///   3. Ensure `cargo-component` is installed (auto-install if missing)
///   4. `cargo component build --target wasm32-unknown-unknown [--release]`
///   5. Copy the resulting .wasm to `target/bex/plugin.wasm`
pub fn run_build(debug: bool, dir: Option<&str>) -> Result<()> {
    let dir = manifest::resolve_dir(dir)?;
    run_build_at(debug, &dir)
}

pub fn run_build_at(debug: bool, dir: &Path) -> Result<()> {
    println!("Building BEX Plugin...\n");

    // ── Validate project structure ────────────────────────────────────────
    let manifest_path = manifest::manifest_path(dir);
    if !manifest_path.exists() {
        anyhow::bail!(
            "No manifest.json found in the current directory.\n\
             Run `bex create` to scaffold a new plugin project."
        );
    }
    let manifest = manifest::load_manifest(dir)?;

    let cargo_path = manifest::cargo_manifest_path(dir);
    if !cargo_path.exists() {
        anyhow::bail!("No Cargo.toml found. Are you inside a Rust plugin project?");
    }

    // ── Ensure cargo-component is available ───────────────────────────────
    println!("[1/3] Checking build toolchain...");
    ensure_cargo_component()?;

    // ── Compile to WASM component ─────────────────────────────────────────
    let profile = if debug { "debug" } else { "release" };
    println!("[2/3] Compiling {} ({}) — {}",
        manifest.name, manifest.plugin_type, profile);

    let mut cargo_args = vec!["component", "build", "--target", "wasm32-unknown-unknown"];
    if !debug {
        cargo_args.push("--release");
    }

    let status = Command::new("cargo")
        .args(&cargo_args)
        .current_dir(dir)
        .status()
        .context("Failed to run `cargo component build`")?;

    if !status.success() {
        anyhow::bail!(
            "`cargo component build` failed.\n\
             Tip: run `cargo component check` to see full errors."
        );
    }

    // ── Locate the compiled .wasm file ────────────────────────────────────
    let crate_name = crate_name_from_cargo_toml(&cargo_path, &manifest.name);
    let profile_dir = if debug { "debug" } else { "release" };
    let wasm_src = locate_wasm(dir, &crate_name, profile_dir)?;

    // ── Copy to target/bex/ ───────────────────────────────────────────────
    let out_dir = dir.join("target/bex");
    fs::create_dir_all(&out_dir)?;
    let out_file = out_dir.join("plugin.wasm");
    fs::copy(&wasm_src, &out_file)?;

    println!(
        "[3/3] ✓ Built: {}\n      Component → {}",
        wasm_src.display(),
        out_file.display()
    );
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Check whether `cargo component` is available. If not, install it.
fn ensure_cargo_component() -> Result<()> {
    let ok = Command::new("cargo")
        .args(["component", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if ok {
        return Ok(());
    }

    println!("  `cargo-component` not found — installing (one-time setup)…");
    let install = Command::new("cargo")
        .args(["install", "cargo-component"])
        .status()
        .context("Failed to run `cargo install cargo-component`")?;

    if !install.success() {
        anyhow::bail!(
            "Could not install `cargo-component`.\n\
             Please install Rust from https://rustup.rs/ and try again."
        );
    }
    println!("  `cargo-component` installed successfully.");
    Ok(())
}

/// Extract the crate name from Cargo.toml `name = "..."` line.
fn crate_name_from_cargo_toml(path: &Path, fallback: &str) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .find(|l| l.trim_start().starts_with("name") && l.contains('='))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().trim_matches('"').replace('-', "_"))
        .unwrap_or_else(|| fallback.replace('-', "_"))
}

/// Find the compiled .wasm in the given profile output directory.
fn locate_wasm(dir: &Path, crate_name: &str, profile: &str) -> Result<PathBuf> {
    let out_dir = dir.join("target/wasm32-unknown-unknown").join(profile);
    let candidate = out_dir.join(format!("{}.wasm", crate_name));
    if candidate.exists() {
        return Ok(candidate);
    }
    // Fallback: first .wasm that isn't a dep artifact
    if out_dir.exists() {
        for entry in fs::read_dir(&out_dir)?.flatten() {
            let p = entry.path();
            if p.extension().map_or(false, |e| e == "wasm")
                && !p.to_string_lossy().contains(".d.")
            {
                return Ok(p);
            }
        }
    }
    anyhow::bail!(
        "Compiled WASM not found.\n\
         Expected: {}/{}.wasm",
        out_dir.display(),
        crate_name
    )
}
