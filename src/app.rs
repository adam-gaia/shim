use crate::shim::{ShimFile, ShimWithMetaInfo, SubcommandShim};
use anyhow::{bail, Result};
use commandstream::{CommandStream, SimpleCommand};
use log::{debug, error};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use which::which;

async fn run_command(command: &[String]) -> Result<i32> {
    let cmd = SimpleCommand::new(command)?;
    let child_status = cmd.run().await?;
    Ok(child_status)
}

pub struct App {
    shims: HashMap<String, ShimWithMetaInfo>,
}
impl App {
    pub fn new(files_to_read: Vec<PathBuf>) -> Result<Self> {
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

    pub fn list(&self) -> Result<()> {
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

    pub fn generate_shims(&self) -> Result<()> {
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
        if processed_hook.contains("$@") || processed_hook.contains("${@}") {
            // TODO: replace subsets of args "$#", ${@[1:4]}, "$1", etc
            let mut join_orig_args = String::with_capacity(original_args.len());
            for x in original_args {
                join_orig_args.push_str(x);
                join_orig_args.push_str(" "); // Add whitespace to separate
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
            run_command(&parts).await?;
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
                    debug!("first: {}, subcommand: {}", first_arg, subcommand);
                    if first_arg != subcommand {
                        // Skip this hook since the subcommand doesnt match our arg
                        continue;
                    }
                } else {
                    // Hooks that do not specify an 'on_subcommand' are ran when no subcommand is
                    // specified. Thus, skip when the first arg is defined. TODO: we should use
                    // None instead of an empty string
                    if first_arg != "" {
                        // Skip
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

    pub async fn run_shimmed_program(&self, original_command: &[String]) -> Result<()> {
        debug!("Command: {:?}", original_command);
        let (original_exec, original_args) = original_command.split_first().unwrap(); // TODO: handle unwraps
        if let Ok(original_exec) = which(&original_exec) {
            let command_name = original_exec.file_name().unwrap();
            let first_arg = if original_args.is_empty() {
                // Consider the first argument an empty string, for matching against a subcommand later
                String::from("")
            } else {
                original_args[0].to_string()
            };

            // Look up a shim based on the command's name
            if let Some(shim) = self.shims.get(command_name.to_str().unwrap()) {
                debug!("Found a shim");
                let shim = &shim.shim; // TODO fix all the structs with similar names
                let env = shim.env(); // TODO: do something with the env

                // Run pre hooks
                self.process_shim_hooks(
                    shim.pre_hooks(),
                    &first_arg,
                    &original_exec,
                    original_args,
                )
                .await?;

                // Run any overrides
                let overrides = shim.overrides();
                if overrides.is_some() {
                    debug!("Overrides: {:?}", overrides);
                    self.process_shim_hooks(overrides, &first_arg, &original_exec, original_args)
                        .await?;
                } else {
                    debug!("No overrides, run the command itself");
                    // No overrides, run the program itself
                    // TODO: create a function for running with an env so that we can use it here
                    // too. When we do call the same function, make sure we do not redundantly call
                    // 'which'
                    // TODO handle the None case instead of unwrapping
                    run_command(original_command).await?;
                }

                // Run post hooks
                self.process_shim_hooks(
                    shim.post_hooks(),
                    &first_arg,
                    &original_exec,
                    original_args,
                )
                .await?;
            } else {
                error!("No registered shim for '{:?}'", original_command);
                todo!("run this command normally whithout any shims?");
            }
        } else {
            bail!("Unable to find '{}' on the system path", original_exec);
        }
        Ok(())
    }
}
