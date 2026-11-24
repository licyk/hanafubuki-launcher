//! A preset's launch arguments over a type's default ones, identity-keyed
//! by name: a preset's argument replaces the default of that name in its
//! place (`false` removes it), the others are appended.

use super::{LaunchArgument, LaunchValue};

/// `defaults` (argv tokens: an option and the values that follow it) with
/// `preset` over them.
pub fn merge_launch_arguments(defaults: &[String], preset: &[LaunchArgument]) -> Vec<String> {
    let mut groups = groups(defaults);
    for argument in preset {
        let tokens = tokens(argument);
        match groups
            .iter_mut()
            .find(|(name, _)| name.as_deref() == Some(argument.name.as_str()))
        {
            Some((_, existing)) => *existing = tokens,
            None => groups.push((Some(argument.name.clone()), tokens)),
        }
    }
    groups.into_iter().flat_map(|(_, tokens)| tokens).collect()
}

/// The tokens one argument passes: its name, then its value unless it is a
/// flag; none for `false`.
fn tokens(argument: &LaunchArgument) -> Vec<String> {
    match &argument.value {
        LaunchValue::Flag(true) => vec![argument.name.clone()],
        LaunchValue::Flag(false) => Vec::new(),
        LaunchValue::Number(number) => vec![argument.name.clone(), number.to_string()],
        LaunchValue::Text(text) => vec![argument.name.clone(), text.clone()],
    }
}

/// `argv` as options, each with the values after it; what comes before the
/// first option has no name.
fn groups(argv: &[String]) -> Vec<(Option<String>, Vec<String>)> {
    let mut groups: Vec<(Option<String>, Vec<String>)> = Vec::new();
    for token in argv {
        if is_option(token) {
            groups.push((Some(token.clone()), vec![token.clone()]));
        } else if let Some((_, tokens)) = groups.last_mut() {
            tokens.push(token.clone());
        } else {
            groups.push((None, vec![token.clone()]));
        }
    }
    groups
}

/// `-x` or `--name`, but not a negative number, which is a value.
fn is_option(token: &str) -> bool {
    token.starts_with('-') && token.len() > 1 && token.parse::<f64>().is_err()
}
