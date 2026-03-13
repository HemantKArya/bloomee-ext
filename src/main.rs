use anyhow::Result;
use clap::{Parser, Subcommand};

mod builder;
mod factory;
mod manifest;
mod packer;
mod scaffold;
mod tester;
mod types;

#[derive(Parser)]
#[command(name = "bex")]
#[command(version)]
#[command(about = "Bloomee Extensions (BEX) — plugin toolkit")]
#[command(long_about = "
Create, build, test, and pack Bloomee plugin extensions.

  bex create              scaffold a new typed plugin from a template
  bex build [PATH]        compile to WASM component (default: current dir)
  bex test                run the plugin against an embedded interactive host
  bex pack [PATH]         bundle into a distributable .bex archive
  bex pack-all [--dir D]  build + pack all plugins, collect into plugins/ dir
    bex update [PATH]       bump manifest version and refresh last_updated
    bex factory init        prepare a plugin-factory repo for GitHub releases
")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold a new BEX plugin project interactively
    Create,

    /// Compile a plugin to a WASM component
    Build {
        /// Build in debug mode (faster, larger binary)
        #[arg(long)]
        debug: bool,
        /// Path to the plugin directory (default: current directory)
        path: Option<String>,
    },

    /// Test the compiled plugin in an embedded interactive host
    Test {
        /// Path to the .wasm component to test (default: target/bex/plugin.wasm)
        #[arg(short, long)]
        wasm: Option<String>,
    },

    /// Pack the compiled plugin + manifest into a .bex archive
    Pack {
        /// Path to the plugin directory (default: current directory)
        path: Option<String>,
    },

    /// Build all plugins in release mode, pack them, and move to plugins/
    #[command(visible_alias = "pack_all")]
    PackAll {
        /// Root directory to scan for plugins (default: current directory)
        #[arg(short, long)]
        dir: Option<String>,

        /// Output directory for generated .bex archives (default: <root>/plugins)
        #[arg(long)]
        output_dir: Option<String>,

        /// Continue after build or pack failures and emit a report for CI workflows
        #[arg(long)]
        skip_failures: bool,
    },

    /// Bump manifest version and refresh last_updated timestamp
    Update {
        /// Path to the plugin directory (default: current directory)
        path: Option<String>,
    },

    /// Prepare a plugin-factory repository with workflow assets and ignore rules
    Factory {
        #[command(subcommand)]
        command: FactoryCommands,
    },
}

#[derive(Subcommand)]
enum FactoryCommands {
    /// Initialize GitHub Actions workflow and .gitignore for a plugin factory repository
    Init {
        /// Directory to initialize (default: current directory)
        #[arg(short, long)]
        dir: Option<String>,

        /// Overwrite generated files managed by bex
        #[arg(long)]
        force: bool,

        /// Do not run `git init` when the target directory is not already a git repository
        #[arg(long)]
        no_git_init: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Create => scaffold::run_create()?,
        Commands::Build { debug, path } => builder::run_build(*debug, path.as_deref())?,
        Commands::Pack { path } => packer::run_pack(path.as_deref())?,
        Commands::PackAll {
            dir,
            output_dir,
            skip_failures,
        } => packer::run_pack_all(dir.as_deref(), output_dir.as_deref(), *skip_failures)?,
        Commands::Update { path } => {
            let (version, timestamp) = manifest::bump_manifest_version(path.as_deref())?;
            println!("Updated manifest version -> {version}");
            println!("Updated manifest last_updated -> {timestamp}");
        }
        Commands::Test { wasm } => tester::run_test(wasm.as_deref())?,
        Commands::Factory { command } => match command {
            FactoryCommands::Init {
                dir,
                force,
                no_git_init,
            } => factory::run_factory_init(dir.as_deref(), *force, !*no_git_init)?,
        },
    }

    Ok(())
}
