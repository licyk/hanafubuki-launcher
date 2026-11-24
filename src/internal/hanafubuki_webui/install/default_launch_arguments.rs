//! Compile-time built-in launch arguments for installed WebUI instances.
//!
//! These values are argv tokens, not a shell command. Each family's module
//! owns its table; runtime behavior has no dependency on the development notes
//! from which the initial tables were derived.

use super::WebUiInstallSpec;

pub fn default_launch_arguments(spec: &WebUiInstallSpec) -> Vec<String> {
    spec.default_launch_arguments()
        .iter()
        .map(|argument| (*argument).to_string())
        .collect()
}

#[cfg(test)]
mod tests;
