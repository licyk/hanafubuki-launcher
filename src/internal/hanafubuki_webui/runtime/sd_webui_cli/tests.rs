use super::*;

fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

#[test]
fn builds_list_with_module_prefix() {
    let args = build_sd_webui_all_in_one_cli_args(
        "/path/to/python",
        &owned(&["self-manager", "api", "serve"]),
    )
    .unwrap();
    assert_eq!(
        args,
        owned(&[
            "/path/to/python",
            "-m",
            "sd_webui_all_in_one",
            "self-manager",
            "api",
            "serve",
        ])
    );
}

#[test]
fn builds_list_with_no_cli_args() {
    let args = build_sd_webui_all_in_one_cli_args("python3", &[]).unwrap();
    assert_eq!(args, owned(&["python3", "-m", "sd_webui_all_in_one"]));
}

#[test]
fn rejects_empty_python_path() {
    assert!(build_sd_webui_all_in_one_cli_args("", &[]).is_err());
    assert!(build_sd_webui_all_in_one_cli_args("   ", &[]).is_err());
}
