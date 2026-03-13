# Bex — Bloomee Extension SDK CLI

**Bex** (short for Bloomee Extension) is a CLI tool designed to streamline the lifecycle of developing, building, and distributing plugins for the Bloomee ecosystem.

## 🚀 Quick Start

### Installation
Ensure you have Rust installed. Clone this repository and run:
`cargo install --path .`

### Usage
```bash
# Scaffold a new plugin project
bex create

# Build your plugin to WASM
bex build

# Test your plugin in the interactive host
bex test

# Pack your plugin for distribution (.bex)
bex pack

# Build and pack everything in a factory repository
bex pack-all

# Update the plugin version by +1 and time stamp for last updated
bex update [path]
```

## 🛠️ CLI Commands

- `bex create`: Interactively scaffold a new plugin from a template.
- `bex build [PATH]`: Compile the plugin in the specified directory (defaults to current) to a WASM component.
- `bex test`: Launch an interactive test host to debug your plugin.
- `bex pack [PATH]`: Bundle the compiled code and manifest into a distributable `.bex` file.
- `bex pack-all`: (Alias: `pack_all`) Automated build/pack cycle for multiple plugins in a "factory" repository.
- `bex factory init`: Initialize a new plugin-factory with workflow files and gitignore rules.
- `bex update`: Updates the plugins version by +1 and time stamp to latest.

## 📦 GitHub Actions & Releases

This project includes a built-in release workflow. To trigger a manual release:
1. Navigate to the **Actions** tab in GitHub.
2. Select the **Release** workflow.
3. Click "Run workflow" and optionally specify a version tag (e.g., `v1.2.3`).
4. Automated binaries for Windows, Linux, and macOS (Intel & Apple Silicon) will be attached to the new release.

## ⚖️ License
Licensed under the [MIT License](LICENSE).

