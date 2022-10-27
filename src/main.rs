#![allow(unused)] // TODO: remove
use anyhow::{bail, Result};
use chrono::prelude::*;
use clap::{CommandFactory, Parser, Subcommand};
use directories_next::ProjectDirs;
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::ExitStatus;
use std::process::Stdio;
use tokio::io::AsyncRead;
use tokio::io::Lines;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::{Stream, StreamExt, StreamMap};
use which::which;

mod shim;
use shim::{Shim, ShimFile, SubcommandShim};

// TODO: do we allow recursion where an override invokes a command that has another shim? - be sure
// not to infinite loop - we could detect that if the program to exec is already on a "stack" that
// we could use to keep track with

struct ShimWithMetaInfo {
    shim: Shim,
    file: PathBuf,
}

impl ShimWithMetaInfo {
    fn new(shim: Shim, file: PathBuf) -> Self {
        ShimWithMetaInfo { shim, file }
    }
}

impl ShimWithMetaInfo {
    fn shell_function(&self, timestamp: &str, this_program_path: &Path) -> String {
        let comment = format!(
            r#"    # Shim for {}
    # Created automatically by {}
    #    from config file {}
    #    at {}"#,
            self.shim.program(),
            this_program_path.display(),
            self.file.display(),
            timestamp
        );
        format!(
            r#"function {}(){{
{}
    shim exec "$@"
}}"#,
            self.shim.program(),
            comment
        )
    }
}

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

async fn run_command(exec: &str, args: &[String]) -> Result<(ExitStatus)> {
    if let Err(..) = which(exec) {
        bail!("Unable to find '{}' on the system path", exec);
    }
    let mut child = Command::new(exec)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // TODO: handle None case instead of unwrapping
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_reader = LinesStream::new(BufReader::new(stdout).lines());
    let mut stderr_reader = LinesStream::new(BufReader::new(stderr).lines());

    let a = Box::pin(async_stream::stream! {
        while let Some(item) = stdout_reader.next().await {
            yield item;
        }
    })
        as Pin<Box<dyn Stream<Item = std::result::Result<String, std::io::Error>> + Send>>;
    let b = Box::pin(async_stream::stream! {
        while let Some(item) = stderr_reader.next().await {
            yield item;
        }
    })
        as Pin<Box<dyn Stream<Item = std::result::Result<String, std::io::Error>> + Send>>;

    let mut map = StreamMap::with_capacity(2);
    map.insert("stdout", a);
    map.insert("stderr", b);

    let handle: tokio::task::JoinHandle<Result<ExitStatus, std::io::Error>> =
        tokio::spawn(async move { child.wait().await });

    // Stream output to terminal as it runs
    while let Some((source, line)) = map.next().await {
        let line = line?;
        // TODO: 'match source {}' would allow use to display streams sepreatly
        println!("{}", line);
    }

    let child_status = handle.await??;
    debug!("Child process exited with status {}", child_status);
    if !child_status.success() {
        // TODO figure out rust's ExitStatusExt to check if a
        // signal killed our process and remove the unwrap
        // TODO: better error handeling to report if a pre/post/override/originalcmd failed
        bail!(
            "'{} {:?}' returned non-zero exit code {}",
            exec,
            args,
            child_status.code().unwrap()
        );
    }
    Ok(child_status)
}

struct App {
    shims: HashMap<String, ShimWithMetaInfo>,
}
impl App {
    fn new(files_to_read: Vec<PathBuf>) -> Result<Self> {
        let mut shims: HashMap<String, ShimWithMetaInfo> = HashMap::new();
        for path in files_to_read {
            if let Ok(f) = std::fs::File::open(&path) {
                debug!("Reading shims from {}", path.display());
                let shim_file: ShimFile = serde_yaml::from_reader(f)?;
                for shim in shim_file.shims() {
                    shims.insert(
                        shim.program().to_string(),
                        ShimWithMetaInfo::new(shim, path.clone()),
                    );
                }
            } else {
                error!("Unable to open {}", path.display());
                continue;
            }
        }
        let app = App { shims };
        Ok(app)
    }

    fn list(&self) -> Result<()> {
        for (program, meta_info_shim) in &self.shims {
            let shim = &meta_info_shim.shim;

            println!("> {}", program);
            if let Some(pre_hooks) = shim.pre_hooks() {
                println!("  * Pre-hooks:");
                for hook in pre_hooks {
                    println!("    - {:?}", hook);
                }
            }
            if let Some(overrides) = shim.overrides() {
                println!("  * Overrides:");
                for hook in overrides {
                    println!("    - {:?}", hook);
                }
            }
            if let Some(pre_hooks) = shim.post_hooks() {
                println!("  * Post-hooks:");
                for hook in pre_hooks {
                    println!("    - {:?}", hook);
                }
            }
        }
        Ok(())
    }

    fn generate_shims(&self) -> Result<()> {
        let this_program_path = env::current_exe()?;
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for shim in self.shims.values() {
            let function = shim.shell_function(&timestamp, &this_program_path);
            println!("{}", function);
        }
        Ok(())
    }

    async fn run_hook(
        &self,
        hook: &str,
        original_command: &Path,
        original_args: &[String],
    ) -> Result<()> {
        // TODO: this function needs to take the env as an input arg to assing to the command
        // Check if the hook refers to the original args and replace if needed
        let mut processed_hook = String::from(hook);
        if processed_hook.contains("$@") {
            // TODO: replace subsets of args "$#", ${@[1:4]}, "$1", etc
            let mut join_orig_args = String::with_capacity(original_args.len());
            for x in original_args {
                join_orig_args.push_str(x);
            }
            processed_hook = processed_hook.replace("$@", &join_orig_args)
        }
        // TODO: can we use the `just` tool's eval/run crate?
        // https://github.com/casey/just

        // Run the processed hook line by line
        for line in processed_hook.split('\n') {
            let line = line.trim();
            if line.starts_with('#') {
                // Skip commented lines
                debug!("Skiping comment {}", line);
                continue;
            }
            debug!("Running command: {}", line);

            let parts: Vec<String> = line.split_whitespace().map(|x| x.to_owned()).collect();
            let command = parts[0].as_str();
            let args = &parts[1..];
            run_command(command, args).await?;
        }

        Ok(())
    }

    async fn process_shim_hooks(
        &self,
        hooks: &Option<Vec<SubcommandShim>>,
        first_arg: &str,
        original_command: &Path,
        original_args: &[String],
    ) -> Result<()> {
        if let Some(hooks) = hooks {
            for hook in hooks {
                if let Some(subcommand) = &hook.on_subcommand {
                    // Only run when the first argument, the subcommand, matches
                    // Else, fall through to run the hook
                    if first_arg != subcommand {
                        // Skip this hook since the subcommand doesnt match our arg
                        continue;
                    }
                }
                // Run the command specified by this hook
                self.run_hook(&hook.run, original_command, original_args)
                    .await?;
            }
        }
        Ok(())
    }

    async fn run_shimmed_program(
        &self,
        original_command: String,
        original_args: &[String],
    ) -> Result<()> {
        debug!("command: {}, args: {:?}", original_command, original_args);
        if let Ok(original_command) = which(&original_command) {
            let command_name = original_command.file_name().unwrap(); // TODO: handle unwraps
            let first_arg = if !original_args.is_empty() {
                original_args[0].to_string()
            } else {
                // Consider the first argument an empty string, for matching against a subcommand later
                String::from("")
            };

            // Look up a shim based on the command's name
            if let Some(shim) = self.shims.get(command_name.to_str().unwrap()) {
                let shim = &shim.shim; // TODO fix all the structs with similar names
                let env = shim.env(); // TODO: do something with the env

                // Run pre hooks
                self.process_shim_hooks(
                    shim.pre_hooks(),
                    &first_arg,
                    &original_command,
                    original_args,
                )
                .await?;

                // Run any overrides
                let overrides = shim.overrides();
                if overrides.is_some() {
                    self.process_shim_hooks(
                        overrides,
                        &first_arg,
                        &original_command,
                        original_args,
                    )
                    .await?;
                } else {
                    // No overrides, run the program itself
                    // TODO: create a function for running with an env so that we can use it here
                    // too. When we do call the same function, make sure we do not redundantly call
                    // 'which'
                    // TODO handle the None case instead of unwrapping
                    let cmd_as_str = original_command.to_str().unwrap();
                    run_command(cmd_as_str, original_args).await?;
                }

                // Run post hooks
                self.process_shim_hooks(
                    shim.post_hooks(),
                    &first_arg,
                    &original_command,
                    original_args,
                )
                .await?;
            } else {
                error!("No registered shim for '{}'", original_command.display());
                todo!("run this command normally whithout any shims?");
            }
        } else {
            bail!("Unable to find '{}' on the system path", original_command);
        }
        Ok(())
    }
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
                    if ext.to_os_string() == ".yaml" {
                        debug!("Found shim file {}", path.display());
                        files_to_read.push(path);
                    }
                }
            }
        } else {
            warn!("Shim dir unaccessible or DNE ({})", shim_dir.display());
        }
    };

    let app = App::new(files_to_read)?;
    if let Some(command) = args.command {
        match &command {
            Commands::Generate => {
                app.generate_shims()?;
            }
            Commands::Exec { trailing_args } => {
                debug!("Exec {:?}", trailing_args);
                if let Some((first, rest)) = trailing_args.split_first() {
                    app.run_shimmed_program(first.to_string(), rest).await?;
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
