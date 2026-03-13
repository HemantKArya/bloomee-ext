# 📦 Bex CLI

**The official Bloomee extension CLI.** `bex` is your all-in-one tool for creating, building, testing, and packaging WebAssembly plugins for the Bloomee ecosystem.

---

## ⚡ Quick Start

Once installed, your plugin development loop is simple and intuitive:

```bash
# 1. Scaffold a new plugin
bex create

# 2. Compile to WebAssembly during development
bex build

# 3. Run and test in the embedded interactive host
bex test

# 4. Bump the version before publishing
bex update

# 5. Package the plugin into a deployable archive
bex pack

```

---

## 🛠 Installation

To install `bex` globally on your system, ensure you have Rust and Cargo installed, then run the following command from the root of this repository:

```bash
cargo install --path .

```

---

## 🛤 Repository Workflows

Depending on how you want to manage your plugins, choose the workflow that best fits your repository structure.

### Option A: The Plugin Factory (Multi-Plugin & CI/CD)

Use this flow when starting a new repository intended to house **multiple plugins** and leverage GitHub release automation.

1. **Initialize the Factory:** Set up automation files in an empty or new repository.
```bash
bex factory init

```


2. **Create Plugins:** Scaffold one or more plugins.
```bash
bex create

```


3. **Develop:** Move into the new plugin directory to build and test.
```bash
cd <plugin-dir>
bex build
bex test
bex pack

```


4. **Pack All:** Return to the repository root to build and package all plugins at once.
```bash
bex pack-all

```



### Option B: Existing Plugin Repository

Use this flow when you want to add a plugin to an **already established repository**.

1. **Create the Plugin:** Run this at the root of your existing repo.
```bash
bex create

```


2. **Develop:** Move into the directory to build, test, and package.
```bash
cd <plugin-dir>
bex build
bex test
bex pack

```



*(Note: If this repository later becomes a factory repo, you can run `bex pack-all` at the root).*

---

## 🧰 Command Reference

| Command | Flags / Args | Description |
| --- | --- | --- |
| **`bex create`** |  | Scaffolds a new plugin directory with `Cargo.toml`, `manifest.json`, `src/lib.rs`, and a local `bex-core` snapshot. |
| **`bex build`** | `[PATH]` | Compiles the plugin to WebAssembly and outputs to `target/bex/plugin.wasm`. |
| **`bex test`** | `[--wasm <path>]` | Runs the compiled plugin in an embedded interactive host for local testing. |
| **`bex update`** | `[PATH]` | Increments the version in `manifest.json` and refreshes the `last_updated` timestamp. |
| **`bex pack`** | `[PATH]` | Bundles `manifest.json` and `target/bex/plugin.wasm` into a compressed `.bex` archive. |
| **`bex pack-all`** | `--dir`, `--output-dir`, `--skip-failures` | Finds all plugin directories, builds/packs each one, and generates factory index outputs. |
| **`bex factory init`** | `--dir`, `--force`, `--no-git-init` | Initializes factory automation files (like GitHub Actions and `.gitignore` blocks). |

---

## 🏭 Factory Automation Outputs

When managing a multi-plugin factory, `bex` generates standardized files to make CI/CD and indexing effortless.

**Running `bex pack-all` generates:**

* 📦 `.bex` archives inside the `plugins/` folder (or your specified `--output-dir`).
* 📄 `bex-factory.json`: The master index of all compiled plugins.
* 📊 `bex-pack-report.json`: A detailed build and packaging report.

**Running `bex factory init` creates/updates:**

* 🤖 `.github/workflows/bex-factory.yml`: GitHub Actions workflow for automated releases.
* 🙈 A managed block inside your `.gitignore` file.


## 📄 License

This project is licensed under the [MIT License](https://www.google.com/search?q=LICENSE).
