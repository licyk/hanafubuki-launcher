use super::*;

/// The Agent's settings are its own configuration's: no patch names them,
/// no schema section shows them, and the files they used to live in are left
/// as they are, neither read nor removed.
#[test]
fn the_agent_settings_are_not_global_settings() {
    let paths = test_paths("agent-keys");
    let state = ConfigState::default();
    std::fs::create_dir_all(&paths.settings_dir).unwrap();
    let legacy = [
        ("agent.json", r#"{"agentPermissionMode": "full_access"}"#),
        (
            "agent-providers.json",
            r#"{"agentProviders": {"revision": 3}}"#,
        ),
    ];
    for (name, text) in legacy {
        std::fs::write(paths.settings_dir.join(name), text).unwrap();
    }
    get_global_settings_inner(&paths, &state).unwrap();

    for key in [
        "agentPermissionMode",
        "agentMemory",
        "agentDisabledPlugins",
        "agentProviders",
        "webSearchProvider",
        "searxngBaseUrl",
        "webFetchUseJinaReader",
    ] {
        let error =
            update_global_settings_patch_inner(&paths, &state, json!({ key: null })).unwrap_err();
        assert_eq!(error, format!("unsupported settings key: {key}"));
    }
    let schema = serde_json::to_value(get_global_settings_schema()).unwrap();
    let sections = schema["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|section| section["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(!sections.contains(&"agent"), "{sections:?}");
    for (name, text) in legacy {
        assert_eq!(
            std::fs::read_to_string(paths.settings_dir.join(name)).unwrap(),
            text
        );
    }
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn proxy_policy_patch_is_atomic_and_preserves_valid_manual_address() {
    let paths = test_paths("proxy-policy");
    let state = ConfigState::default();
    let manual = update_global_settings_patch_inner(
        &paths,
        &state,
        json!({
            "proxyMode": "manual",
            "proxyAddress": "socks5://127.0.0.1:1080"
        }),
    )
    .unwrap();
    assert_eq!(manual.proxy_mode, ProxyMode::Manual);
    assert_eq!(
        manual.proxy_address.as_deref(),
        Some("socks5://127.0.0.1:1080")
    );

    let disabled =
        update_global_settings_patch_inner(&paths, &state, json!({"proxyMode": "disabled"}))
            .unwrap();
    assert_eq!(disabled.proxy_mode, ProxyMode::Disabled);
    assert_eq!(disabled.proxy_address, manual.proxy_address);

    let system =
        update_global_settings_patch_inner(&paths, &state, json!({"proxyMode": "system"})).unwrap();
    assert_eq!(system.proxy_mode, ProxyMode::System);
    assert_eq!(system.proxy_address, manual.proxy_address);
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}

#[test]
fn proxy_schema_uses_stable_values_message_keys_and_disablement() {
    let schema = serde_json::to_value(get_global_settings_schema()).unwrap();
    let fields = schema["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|section| section["id"] == "network")
        .unwrap()["fields"]
        .as_array()
        .unwrap();
    assert_eq!(fields[0]["key"], "proxyMode");
    assert_eq!(
        fields[0]["label"],
        json!({"kind": "key", "key": "settings.fields.proxyMode.label"})
    );
    assert_eq!(
        fields[0]["options"],
        json!([
            {"value": "system", "label": {"kind": "key", "key": "settings.options.proxyMode.system"}},
            {"value": "manual", "label": {"kind": "key", "key": "settings.options.proxyMode.manual"}},
            {"value": "disabled", "label": {"kind": "key", "key": "settings.options.proxyMode.disabled"}}
        ])
    );
    assert_eq!(fields[1]["key"], "proxyAddress");
    assert_eq!(
        fields[1]["disabledReason"],
        json!({"kind": "key", "key": "settings.fields.proxyAddress.manualOnly"})
    );
    assert_eq!(
        fields[1]["disabledWhen"],
        json!({"any": [
            {"key": "proxyMode", "equals": "system"},
            {"key": "proxyMode", "equals": "disabled"}
        ]})
    );
}

/// Every schema field, flattened across sections.
#[test]
fn a_grouped_pair_is_accepted_when_it_is_patched_together() {
    let paths = test_paths("grouped-pair");
    let state = ConfigState::default();

    // Half the pair is refused, which is why the editor submits both.
    let rejected =
        update_global_settings_patch_inner(&paths, &state, json!({"proxyMode": "manual"}));
    assert!(rejected.is_err());

    let settings = update_global_settings_patch_inner(
        &paths,
        &state,
        json!({"proxyMode": "manual", "proxyAddress": "http://127.0.0.1:7890"}),
    )
    .unwrap();
    assert_eq!(settings.proxy_mode, ProxyMode::Manual);
    assert_eq!(
        settings.proxy_address.as_deref(),
        Some("http://127.0.0.1:7890")
    );
}

#[test]
fn proxy_policy_rejects_missing_or_malformed_manual_addresses_without_writing() {
    let paths = test_paths("proxy-invalid");
    let state = ConfigState::default();
    let initial = get_global_settings_inner(&paths, &state).unwrap();
    for patch in [
        json!({"proxyMode": "manual"}),
        json!({"proxyMode": "manual", "proxyAddress": "http://host"}),
        json!({"proxyMode": "manual", "proxyAddress": "ftp://host:21"}),
        json!({"proxyMode": "manual", "proxyAddress": "http://bad\0host:80"}),
        json!({"proxyMode": "other", "proxyAddress": "http://host:8080"}),
    ] {
        assert!(update_global_settings_patch_inner(&paths, &state, patch).is_err());
        let current = get_global_settings_inner(&paths, &state).unwrap();
        assert_eq!(current.proxy_mode, initial.proxy_mode);
        assert_eq!(current.proxy_address, initial.proxy_address);
    }
    let _ = std::fs::remove_dir_all(&paths.app_data_dir);
}
