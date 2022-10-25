use serde::{Deserialize, Serialize};
use std::fmt;

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
