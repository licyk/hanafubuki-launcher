use super::{InstanceRuntime, InstanceTypeId};

#[test]
fn type_ids_are_a_namespace_and_a_name_of_the_id_alphabet() {
    for valid in ["builtin:comfyui", "com.example.tools:llama-cpp", "a1:b.2"] {
        assert_eq!(InstanceTypeId::parse(valid).unwrap().as_str(), valid);
    }
    for invalid in [
        "",
        "comfyui",
        ":comfyui",
        "builtin:",
        "builtin:ComfyUI",
        "builtin:sd_webui",
        "builtin:a:b",
        "builtin: comfyui",
    ] {
        assert!(InstanceTypeId::parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn type_ids_decode_through_the_same_validation() {
    let id: InstanceTypeId = serde_json::from_value(serde_json::json!("builtin:sd-webui")).unwrap();
    assert_eq!(serde_json::to_value(&id).unwrap(), "builtin:sd-webui");
    assert!(serde_json::from_value::<InstanceTypeId>(serde_json::json!("sdWebui")).is_err());
}

#[test]
fn a_runtime_serializes_by_kind_with_a_python_default_only_when_declared() {
    let python = InstanceRuntime::Python { version: None };
    assert_eq!(
        serde_json::to_value(&python).unwrap(),
        serde_json::json!({ "kind": "python" })
    );
    assert_eq!(
        serde_json::to_value(InstanceRuntime::SystemBinary {
            name: "node".to_string()
        })
        .unwrap(),
        serde_json::json!({ "kind": "system-binary", "name": "node" })
    );
    assert_eq!(
        serde_json::to_value(InstanceRuntime::None).unwrap(),
        serde_json::json!({ "kind": "none" })
    );
}
