use super::*;

/// The union of what the copies in the instance config validation, the raw
/// argument check, the candidate validation and the merge checked.
#[test]
fn each_argument_is_non_empty_bounded_and_free_of_nul() {
    assert_eq!(launch_argument_issue("--listen"), None);
    assert_eq!(launch_argument_issue("a b"), None);
    assert_eq!(launch_argument_issue(""), Some(LaunchArgumentIssue::Empty));
    assert_eq!(
        launch_argument_issue(&"a".repeat(MAX_LAUNCH_ARGUMENT_BYTES)),
        None
    );
    assert_eq!(
        launch_argument_issue(&"a".repeat(MAX_LAUNCH_ARGUMENT_BYTES + 1)),
        Some(LaunchArgumentIssue::TooLong)
    );
    // The byte length counts, not the characters.
    assert_eq!(
        launch_argument_issue(&"é".repeat(MAX_LAUNCH_ARGUMENT_BYTES / 2 + 1)),
        Some(LaunchArgumentIssue::TooLong)
    );
    assert_eq!(
        launch_argument_issue("--a\0b"),
        Some(LaunchArgumentIssue::Nul)
    );
}

#[test]
fn a_list_is_within_bounds_only_when_its_count_and_every_argument_are() {
    let empty: [&str; 0] = [];
    assert!(launch_arguments_within_bounds(&empty));
    let full = vec!["--x".to_string(); MAX_LAUNCH_ARGUMENTS];
    assert!(launch_arguments_within_bounds(&full));
    let over = vec!["--x".to_string(); MAX_LAUNCH_ARGUMENTS + 1];
    assert!(!launch_arguments_within_bounds(&over));
    assert!(!launch_arguments_within_bounds(&["--ok", ""]));
    assert!(!launch_arguments_within_bounds(&["--ok", "a\0"]));
}
