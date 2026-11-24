/// The bounds on an instance's launch argument list: the list the instance
/// config stores, the one the user edits, and the one a launch merges from
/// the defaults and the user's arguments. The configuration repository and
/// the launch domain both check them, through this one definition.
pub const MAX_LAUNCH_ARGUMENTS: usize = 256;

/// The longest launch argument, in bytes.
pub const MAX_LAUNCH_ARGUMENT_BYTES: usize = 4096;

/// Why one launch argument cannot be passed to a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchArgumentIssue {
    Empty,
    TooLong,
    /// A NUL cannot be part of a process argument.
    Nul,
}

/// The first issue of one argument, if it has one.
pub fn launch_argument_issue(argument: &str) -> Option<LaunchArgumentIssue> {
    if argument.is_empty() {
        Some(LaunchArgumentIssue::Empty)
    } else if argument.len() > MAX_LAUNCH_ARGUMENT_BYTES {
        Some(LaunchArgumentIssue::TooLong)
    } else if argument.contains('\0') {
        Some(LaunchArgumentIssue::Nul)
    } else {
        None
    }
}

/// Whether `arguments` is within the count bound and every argument has no
/// issue.
pub fn launch_arguments_within_bounds<S: AsRef<str>>(arguments: &[S]) -> bool {
    arguments.len() <= MAX_LAUNCH_ARGUMENTS
        && arguments
            .iter()
            .all(|argument| launch_argument_issue(argument.as_ref()).is_none())
}

#[cfg(test)]
mod tests;
