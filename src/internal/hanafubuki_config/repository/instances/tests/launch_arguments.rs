use super::*;

#[test]
fn launch_arguments_persist_as_canonical_string_list_and_revision_is_compare_and_swap() {
    let paths = test_paths("launch-arguments");
    let state = ConfigState::default();
    let _ = std::fs::remove_file(paths.instance_config_file("demo"));
    create_instance_config_inner(&paths, &state, sample_instance()).unwrap();
    let updated = update_instance_launch_args_inner(
        &paths,
        &state,
        "demo",
        1,
        vec!["--listen".into(), "127.0.0.1".into()],
    )
    .unwrap();
    assert_eq!(updated.config_revision, 2);
    assert_eq!(updated.launch_args, ["--listen", "127.0.0.1"]);
    assert_eq!(updated.launch_argument_origin, LaunchArgumentOrigin::User);
    let reloaded = get_instance_config_inner(&paths, &state, "demo").unwrap();
    assert_eq!(reloaded.launch_args, updated.launch_args);
    let raw: Value = safe_file::read_json(&paths.instance_config_file("demo"))
        .unwrap()
        .unwrap();
    assert_eq!(raw["launchArgs"], json!(["--listen", "127.0.0.1"]));
    assert!(
        update_instance_launch_args_inner(
            &paths,
            &state,
            "demo",
            1,
            vec!["--port".into(), "8188".into()],
        )
        .unwrap_err()
        .contains("stale")
    );
    assert_eq!(
        get_instance_config_inner(&paths, &state, "demo")
            .unwrap()
            .launch_args,
        ["--listen", "127.0.0.1"]
    );
}

#[test]
fn builtin_launch_arguments_initialize_idempotently_and_preserve_explicit_user_empty() {
    let paths = test_paths("builtin-launch-arguments");
    let state = ConfigState::default();
    let _ = std::fs::remove_file(paths.instance_config_file("demo"));
    let mut pending = sample_instance();
    pending.launch_argument_origin = LaunchArgumentOrigin::PendingDefault;
    create_instance_config_inner(&paths, &state, pending).unwrap();

    let initialized = synchronize_builtin_instance_launch_args_inner(
        &paths,
        &state,
        "demo",
        vec!["--auto-launch".into()],
    )
    .unwrap();
    assert_eq!(initialized.config_revision, 2);
    assert_eq!(
        initialized.launch_argument_origin,
        LaunchArgumentOrigin::BuiltIn
    );
    assert_eq!(initialized.launch_args, ["--auto-launch"]);

    let repeated = synchronize_builtin_instance_launch_args_inner(
        &paths,
        &state,
        "demo",
        vec!["--auto-launch".into()],
    )
    .unwrap();
    assert_eq!(repeated.config_revision, 2);

    let user = update_instance_launch_args_inner(&paths, &state, "demo", 2, Vec::new()).unwrap();
    assert_eq!(user.config_revision, 3);
    assert_eq!(user.launch_argument_origin, LaunchArgumentOrigin::User);
    assert!(user.launch_args.is_empty());

    let preserved = synchronize_builtin_instance_launch_args_inner(
        &paths,
        &state,
        "demo",
        vec!["--replacement-default".into()],
    )
    .unwrap();
    assert_eq!(preserved.config_revision, 3);
    assert_eq!(preserved.launch_argument_origin, LaunchArgumentOrigin::User);
    assert!(preserved.launch_args.is_empty());

    let custom = update_instance_launch_args_inner(
        &paths,
        &state,
        "demo",
        3,
        vec!["--user-value".into(), "custom".into()],
    )
    .unwrap();
    let custom_preserved = synchronize_builtin_instance_launch_args_inner(
        &paths,
        &state,
        "demo",
        vec!["--another-default".into()],
    )
    .unwrap();
    assert_eq!(custom_preserved.config_revision, custom.config_revision);
    assert_eq!(custom_preserved.launch_args, ["--user-value", "custom"]);
    assert_eq!(
        custom_preserved.launch_argument_origin,
        LaunchArgumentOrigin::User
    );
}

#[test]
fn pending_default_origin_rejects_nonempty_arguments() {
    let paths = test_paths("pending-default-nonempty");
    let state = ConfigState::default();
    let mut pending = sample_instance();
    pending.launch_argument_origin = LaunchArgumentOrigin::PendingDefault;
    pending.launch_args = vec!["--not-yet-assigned".into()];
    assert!(
        create_instance_config_inner(&paths, &state, pending)
            .unwrap_err()
            .contains("pending default")
    );
}

#[test]
fn revision_check_is_a_short_repository_read_without_a_caller_callback() {
    let paths = test_paths("revision-check");
    let state = ConfigState::default();
    let _ = std::fs::remove_file(paths.instance_config_file("demo"));
    create_instance_config_inner(&paths, &state, sample_instance()).unwrap();
    require_instance_config_revision_inner(&paths, &state, "demo", 1).unwrap();
    update_instance_launch_args_inner(
        &paths,
        &state,
        "demo",
        1,
        vec!["--port".into(), "8188".into()],
    )
    .unwrap();
    assert_eq!(
        require_instance_config_revision_inner(&paths, &state, "demo", 1).unwrap_err(),
        "instance config revision is stale (observed 1, current 2)"
    );
}
