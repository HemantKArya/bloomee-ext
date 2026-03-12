use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── Managed-block markers ──────────────────────────────────────────────────────
const GITIGNORE_START: &str = "# --- bex factory managed ---";
const GITIGNORE_END: &str = "# --- /bex factory managed ---";

const GITIGNORE_BLOCK: &str = include_str!("../assets/factory/gitignore-block.txt");
const WORKFLOW_YAML: &str = include_str!("../assets/factory/bex-factory.yml");

// ── `bex factory init` ─────────────────────────────────────────────────────────

pub fn run_factory_init(dir: Option<&str>, force: bool, git_init: bool) -> Result<()> {
    let root = resolve_or_create_root(dir)?;
    ensure_directory(&root)?;
    ensure_directory(&root.join(".github"))?;
    ensure_directory(&root.join(".github/workflows"))?;

    let workflow_path = root.join(".github/workflows/bex-factory.yml");
    let workflow_status = write_managed_file(&workflow_path, WORKFLOW_YAML, force)?;
    update_gitignore(&root.join(".gitignore"))?;

    if git_init {
        maybe_git_init(&root)?;
    }

    println!("Initialized BEX factory in {}", root.display());
    println!(
        "  {} {}",
        workflow_status,
        workflow_path
            .strip_prefix(&root)
            .unwrap_or(&workflow_path)
            .display()
    );
    println!("  .gitignore (managed block updated)");
    println!();
    println!("Next steps:");
    println!("  1. Add plugin directories to this repository.");
    println!("  2. Trigger the GitHub Actions workflow manually.");
    println!(
        "  3. The workflow runs `bex pack-all`, finalizes `bex-factory.json`, and publishes the release assets."
    );
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn write_managed_file(path: &Path, content: &str, force: bool) -> Result<&'static str> {
    let existed_before = path.exists();

    if existed_before {
        if path.is_dir() {
            anyhow::bail!("Expected a file but found a directory at {}", path.display());
        }
        let existing = fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        if existing == content {
            return Ok("unchanged");
        }
        if !force {
            anyhow::bail!(
                "Refusing to overwrite {}. Re-run with --force to overwrite.",
                path.display()
            );
        }
    }
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    Ok(if existed_before { "updated" } else { "created" })
}

fn update_gitignore(path: &Path) -> Result<()> {
    if path.exists() && path.is_dir() {
        anyhow::bail!("Expected a file but found a directory at {}", path.display());
    }
    let content = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
    } else {
        String::new()
    };

    let updated = if let (Some(s), Some(e)) =
        (content.find(GITIGNORE_START), content.find(GITIGNORE_END))
    {
        let mut out = String::new();
        let before = content[..s].trim_end();
        if !before.is_empty() {
            out.push_str(before);
            out.push_str("\n\n");
        }
        out.push_str(GITIGNORE_BLOCK.trim_end());
        let after = content[e + GITIGNORE_END.len()..].trim();
        if !after.is_empty() {
            out.push_str("\n\n");
            out.push_str(after);
        }
        out.push('\n');
        out
    } else if content.trim().is_empty() {
        format!("{}\n", GITIGNORE_BLOCK.trim_end())
    } else {
        format!("{}\n\n{}\n", content.trim_end(), GITIGNORE_BLOCK.trim_end())
    };

    fs::write(path, updated).with_context(|| format!("writing {}", path.display()))
}

fn maybe_git_init(root: &Path) -> Result<()> {
    if root.join(".git").exists() {
        return Ok(());
    }
    match Command::new("git").arg("init").current_dir(root).status() {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => {
            eprintln!("warning: `git init` failed in {}", root.display());
            Ok(())
        }
        Err(e) => {
            eprintln!("warning: could not run `git init`: {e}");
            Ok(())
        }
    }
}

fn resolve_or_create_root(dir: Option<&str>) -> Result<PathBuf> {
    let raw = dir.unwrap_or(".");
    let path = PathBuf::from(raw);

    if path.exists() {
        if !path.is_dir() {
            anyhow::bail!("Factory target is not a directory: {}", path.display());
        }
    } else {
        fs::create_dir_all(&path)
            .with_context(|| format!("creating directory {}", path.display()))?;
    }

    path.canonicalize()
        .with_context(|| format!("resolving directory {}", path.display()))
}

fn ensure_directory(path: &Path) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            anyhow::bail!("Expected a directory but found a file at {}", path.display());
        }
        return Ok(());
    }

    fs::create_dir_all(path).with_context(|| format!("creating directory {}", path.display()))
}
