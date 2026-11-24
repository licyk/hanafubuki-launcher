//! Composition of the environment passed directly to native child processes.
//!
//! These names are not shell identifiers. Native process APIs permit spaces,
//! punctuation, leading digits, and Unicode; only representation-invalid data
//! is rejected, by `validate_native_environment` where an entry enters. This
//! module composes entries and checks only the PATH value it joins.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Merges application-owned environment entries into a caller environment.
///
/// Reserved entries always win. On Windows, environment names are matched
/// using native case-insensitive comparison so the result cannot contain a
/// differently-cased alias for a reserved name. Both maps were validated
/// where they were built or entered (configuration, `launch.prepare`, the
/// App API managed environment, the reserved builders); this only merges.
pub fn merge_reserved_environment(
    mut environment: BTreeMap<String, String>,
    reserved: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    for (name, value) in reserved {
        #[cfg(windows)]
        environment.retain(|existing, _| !native_environment_names_equal(existing, name));
        environment.insert(name.clone(), value.clone());
    }
    environment
}

/// Builds a native PATH value by placing application-owned entries before an
/// inherited PATH while retaining only the first occurrence of each entry.
///
/// The caller supplies the inherited value so this helper stays deterministic
/// and never mutates or implicitly reads the process environment.
pub fn prepend_path_entries(
    prepended: impl IntoIterator<Item = PathBuf>,
    inherited_path: Option<&OsStr>,
) -> Result<String, String> {
    let mut entries = Vec::new();
    for entry in prepended {
        if entry.as_os_str().is_empty() {
            return Err("PATH prepend entry must not be empty".to_string());
        }
        push_unique_path_entry(&mut entries, entry);
    }
    if let Some(inherited_path) = inherited_path {
        for entry in std::env::split_paths(inherited_path) {
            push_unique_path_entry(&mut entries, entry);
        }
    }

    std::env::join_paths(entries)
        .map_err(|error| format!("failed to build PATH: {error}"))?
        .into_string()
        .map_err(|_| "PATH is not valid UTF-8".to_string())
}

/// The names of this process's environment variables, which its children
/// inherit; a name that is not UTF-8 is left out, as no pattern names it.
pub fn process_environment_names() -> Vec<String> {
    std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .collect()
}

/// Whether the variable `name` is one `pattern` names: exactly, or by the
/// prefix before a final `*` (`npm_config_*`). On Windows both compare as
/// the system compares variable names, without regard to case.
pub fn native_environment_name_matches(name: &str, pattern: &str) -> bool {
    let Some(prefix) = pattern.strip_suffix('*') else {
        return native_names_equal(name, pattern);
    };
    name.get(..prefix.len())
        .is_some_and(|head| native_names_equal(head, prefix))
}

fn native_names_equal(left: &str, right: &str) -> bool {
    #[cfg(windows)]
    {
        native_environment_names_equal(left, right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn is_native_path_name(name: &str) -> bool {
    #[cfg(windows)]
    {
        native_environment_names_equal(name, "PATH")
    }
    #[cfg(not(windows))]
    {
        name == "PATH"
    }
}

fn join_configured_path_values(values: &[String]) -> Result<OsString, String> {
    let entries = values
        .iter()
        .flat_map(|value| std::env::split_paths(OsStr::new(value)))
        .collect::<Vec<_>>();
    std::env::join_paths(entries)
        .map_err(|error| format!("failed to normalize configured PATH: {error}"))
}

/// Normalizes an environment to one native `PATH` while prepending the supplied
/// application-owned entries. A configured PATH replaces the process fallback;
/// the process PATH is used only when the environment has no native PATH key.
pub fn merge_prepended_path_environment(
    mut environment: BTreeMap<String, String>,
    prepended: impl IntoIterator<Item = PathBuf>,
    process_path: Option<&OsStr>,
) -> Result<BTreeMap<String, String>, String> {
    let configured_values = environment
        .iter()
        .filter(|(name, _)| is_native_path_name(name))
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    environment.retain(|name, _| !is_native_path_name(name));

    let configured_path = if configured_values.is_empty() {
        None
    } else {
        Some(join_configured_path_values(&configured_values)?)
    };
    let inherited_path = if configured_values.is_empty() {
        process_path
    } else {
        configured_path.as_deref().filter(|value| !value.is_empty())
    };
    let value = prepend_path_entries(prepended, inherited_path)?;
    environment.insert("PATH".to_string(), value);
    Ok(environment)
}

fn push_unique_path_entry(entries: &mut Vec<PathBuf>, entry: PathBuf) {
    if !entries
        .iter()
        .any(|existing| native_path_entries_equal(existing, &entry))
    {
        entries.push(entry);
    }
}

#[cfg(not(windows))]
pub fn native_path_entries_equal(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(windows)]
pub fn native_path_entries_equal(left: &Path, right: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};

    let normalize = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .map(|character| {
                if character == b'/' as u16 {
                    b'\\' as u16
                } else {
                    character
                }
            })
            .collect::<Vec<_>>()
    };
    let left = normalize(left);
    let right = normalize(right);
    let Ok(left_len) = i32::try_from(left.len()) else {
        return false;
    };
    let Ok(right_len) = i32::try_from(right.len()) else {
        return false;
    };

    // SAFETY: both pointers remain valid for their explicit lengths throughout
    // the call. CompareStringOrdinal does not require NUL termination.
    unsafe {
        CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) == CSTR_EQUAL
    }
}

#[cfg(windows)]
pub fn native_environment_names_equal(left: &str, right: &str) -> bool {
    use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};

    let left: Vec<u16> = left.encode_utf16().collect();
    let right: Vec<u16> = right.encode_utf16().collect();
    let Ok(left_len) = i32::try_from(left.len()) else {
        return false;
    };
    let Ok(right_len) = i32::try_from(right.len()) else {
        return false;
    };

    // SAFETY: both pointers remain valid for their explicit lengths throughout
    // the call. CompareStringOrdinal does not require NUL termination.
    unsafe {
        CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) == CSTR_EQUAL
    }
}

#[cfg(test)]
mod tests;
