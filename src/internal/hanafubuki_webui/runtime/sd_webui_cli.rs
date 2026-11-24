//! Pure argument-list builder for the `sd_webui_all_in_one` Python CLI.
//!
//! This module only constructs argument lists. It deliberately does **not**
//! execute anything, create terminal sessions, inject environment variables,
//! resolve working directories, check Python availability, or use any Tauri
//! types. It is an ordinary Rust helper independent from `config`, `instances`,
//! the interactive terminal, the command executor, and runtime state.
//!
/// The Python module name invoked with `python -m <module>`.
const CLI_MODULE: &str = "sd_webui_all_in_one";

/// Build the argument vector to invoke the `sd_webui_all_in_one` CLI:
///
/// ```text
/// ["<python interpreter path>", "-m", "sd_webui_all_in_one", ...cli_args]
/// ```
///
/// The `cli_args` are appended verbatim (no parsing, no quoting). Returns an
/// error if `python_path` is empty (or only whitespace).
pub fn build_sd_webui_all_in_one_cli_args(
    python_path: &str,
    cli_args: &[String],
) -> Result<Vec<String>, String> {
    if python_path.trim().is_empty() {
        return Err("python interpreter path must not be empty".to_string());
    }

    let mut args = Vec::with_capacity(cli_args.len() + 3);
    args.push(python_path.to_string());
    args.push("-m".to_string());
    args.push(CLI_MODULE.to_string());
    args.extend(cli_args.iter().cloned());
    Ok(args)
}

#[cfg(test)]
mod tests;
