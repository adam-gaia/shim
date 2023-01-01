use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

pub struct ShimWithMetaInfo {
    pub shim: Shim,
    file: PathBuf,
}

impl ShimWithMetaInfo {
    pub fn new(shim: Shim, file: PathBuf) -> Self {
        ShimWithMetaInfo { shim, file }
    }
}

impl ShimWithMetaInfo {
    pub fn shell_function(&self, timestamp: &str, this_program_path: &Path) -> String {
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
    shim exec -- {} "$@"
}}"#,
            self.shim.program(),
            comment,
            self.shim.program()
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct EnvVar {
    key: String,
    value: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubcommandShim {
    pub on_subcommand: Option<String>,
    pub env: Option<Vec<String>>,
    pub run: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Shim {
    program: String,
    pre: Option<Vec<SubcommandShim>>,
    r#override: Option<Vec<SubcommandShim>>,
    post: Option<Vec<SubcommandShim>>,
    env: Option<Vec<String>>,
}

impl Shim {
    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn pre_hooks(&self) -> &Option<Vec<SubcommandShim>> {
        &self.pre
    }

    pub fn post_hooks(&self) -> &Option<Vec<SubcommandShim>> {
        &self.post
    }

    pub fn overrides(&self) -> &Option<Vec<SubcommandShim>> {
        &self.r#override
    }

    pub fn env(&self) -> &Option<Vec<String>> {
        &self.env
    }
}

impl fmt::Display for Shim {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", serde_yaml::to_string(&self).unwrap())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShimFile {
    shims: Vec<Shim>,
}

impl ShimFile {
    pub fn shims(self) -> Vec<Shim> {
        self.shims
    }
}
