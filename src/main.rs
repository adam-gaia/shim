use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use directories_next::ProjectDirs;
use log::{debug, warn};
use std::env;
use std::fs;
use std::path::PathBuf;
mod app;
mod shim;
use app::App;

// TODO: do we allow recursion where an override invokes a command that has another shim? - be sure
// not to infinite loop - we could detect that if the program to exec is already on a "stack" that
// we could use to keep track with

/// Create shims for existing executables
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input file to read shims from
    #[arg(short, long)]
    file: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate shims as shell functions
    Generate,

    /// Run a program with the shims
    Exec {
        #[arg(last = true)]
        trailing_args: Vec<String>,
    },

    /// Check that the environment is setup
    Check,

    /// Show all registered hooks
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    // TODO: KDL might be a better config language for this program https://kdl.dev/
    env_logger::init();
    let this_program_name = clap::crate_name!();
    let args = Args::parse();
    let mut files_to_read: Vec<PathBuf> = Vec::new();

    // Collect shim files specified by `--file` arg(s)
    if let Some(file) = args.file {
        if let Ok(file) = file.canonicalize() {
            files_to_read.push(file);
        } else {
            bail!("File not found: {}", file.display());
        }
    }

    // Collect shim files from the config dir (XDG_CONFIG_DIR/<program_name>/shims/*.yaml)
    if let Some(proj_dirs) = ProjectDirs::from("", "", this_program_name) {
        let mut shim_dir = proj_dirs.config_dir().to_owned();
        shim_dir.push("shims");
        if fs::create_dir_all(&shim_dir).is_err() {
            bail!("bad {}", shim_dir.display());
        }
        if let Ok(contents) = fs::read_dir(&shim_dir) {
            for file in contents.flatten() {
                let path = file.path();
                if let Some(ext) = path.extension() {
                    if ext.to_os_string() == "yaml" {
                        files_to_read.push(path);
                    }
                }
            }
        } else {
            warn!("Shim dir unaccessible or DNE ({})", shim_dir.display());
        }
    };
    debug!("Shim files: {:?}", files_to_read);

    let app = App::new(files_to_read)?;
    if let Some(command) = args.command {
        match &command {
            Commands::Generate => {
                app.generate_shims()?;
            }
            Commands::Exec { trailing_args } => {
                debug!("Exec {:?}", trailing_args);
                if trailing_args.len() > 0 {
                    app.run_shimmed_program(&trailing_args).await?;
                } else {
                    bail!("Nothing to exec");
                }
            }
            Commands::Check => {
                unimplemented!("todo");
            }
            Commands::List => {
                app.list()?;
            }
        }
    } else {
        app.list()?;
    }

    Ok(())
}
