use std::sync::Arc;

use crate::internal::hanafubuki_foundation::module_faults::ContributionOwner;
use crate::internal::hanafubuki_instance_contracts::{
    InstanceTypeDescriptor, InstanceTypeId, InstanceTypeKind, ServerReadiness, ServerRestart,
    ServerRole, WebUiFamily, server_start_order,
};
use serde_json::{Map, Value, json};

use super::{BuiltinProvider, InstanceProvider, InstanceTypeRegistry};
use crate::internal::hanafubuki_webui::instances::InstanceSpec;

fn object(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn the_built_in_types_are_the_families_under_stable_ids() {
    let registry = InstanceTypeRegistry::with_builtin_types();
    let ids: Vec<String> = registry
        .descriptors()
        .into_iter()
        .map(|descriptor| {
            assert_eq!(descriptor.kind, InstanceTypeKind::Builtin);
            assert_eq!(descriptor.components, ["core", "python"]);
            descriptor.id.to_string()
        })
        .collect();
    assert_eq!(
        ids,
        [
            "builtin:sd-webui",
            "builtin:comfyui",
            "builtin:invokeai",
            "builtin:fooocus",
            "builtin:sd-trainer",
            "builtin:qwen-tts-webui",
            "builtin:rvc-next-webui",
            "builtin:mizuoto-tts-webui",
        ]
    );
    for family in WebUiFamily::ALL {
        let id = BuiltinProvider::type_id(family);
        assert_eq!(BuiltinProvider::family_of(&id), Some(family));
        assert!(registry.get(&id).is_some());
    }
}

#[test]
fn a_type_is_found_by_id_and_a_missing_one_says_so() {
    let registry = InstanceTypeRegistry::with_builtin_types();
    let missing = InstanceTypeId::parse("ext.missing:thing").unwrap();
    assert!(registry.get(&missing).is_none());
    assert_eq!(BuiltinProvider::family_of(&missing), None);
    assert_eq!(
        registry.require(&missing).err().unwrap(),
        "type `ext.missing:thing` is not available"
    );
    let comfyui = BuiltinProvider::type_id(WebUiFamily::Comfyui);
    assert_eq!(registry.require(&comfyui).unwrap().descriptor().id, comfyui);
}

#[test]
fn a_duplicate_type_id_is_refused() {
    let registry = InstanceTypeRegistry::with_builtin_types();
    let error = registry
        .register(
            ContributionOwner::Core,
            Arc::new(BuiltinProvider::new(WebUiFamily::Fooocus)),
        )
        .unwrap_err();
    assert_eq!(
        error,
        "instance type `builtin:fooocus` is already registered"
    );
    assert_eq!(registry.descriptors().len(), WebUiFamily::ALL.len());
}

#[test]
fn a_request_spec_is_strict_and_a_persisted_one_tolerant() {
    let provider = BuiltinProvider::new(WebUiFamily::Comfyui);
    let InstanceSpec::Builtin(spec) = provider
        .validate_spec(object(json!({ "pythonVersion": "3.11" })))
        .unwrap()
    else {
        panic!("a built-in type reads a built-in spec");
    };
    assert_eq!(spec.family(), WebUiFamily::Comfyui);
    assert_eq!(
        spec.to_object(),
        object(json!({
            "pytorch": null,
            "pythonVersion": "3.11",
            "predownloadModels": null,
            "predownloadExtensions": null,
        }))
    );

    for undeclared in [json!({ "branch": "main" }), json!({ "type": "comfyui" })] {
        let error = provider.validate_spec(object(undeclared)).unwrap_err();
        assert!(
            error.starts_with("ComfyUI install spec: unknown field"),
            "{error}"
        );
    }
    let read = provider
        .read_spec(object(json!({ "branch": "main", "pythonVersion": "3.11" })))
        .unwrap();
    assert_eq!(read, InstanceSpec::Builtin(spec));
}

/// What `list_instance_types` sends the frontend for one type.
#[test]
fn a_descriptor_serializes_as_the_frontend_reads_it() {
    let descriptor = BuiltinProvider::new(WebUiFamily::QwenTts)
        .descriptor()
        .clone();
    assert_eq!(
        serde_json::to_value(descriptor).unwrap(),
        json!({
            "id": "builtin:qwen-tts-webui",
            "kind": "builtin",
            "label": { "kind": "literal", "text": { "en": "Qwen TTS WebUI" } },
            "description": {
                "kind": "key",
                "key": "instances.create.family.descriptions.qwenTts"
            },
            "preview": "qwen-tts-webui",
            "components": ["core", "python"],
            "runtime": { "kind": "python" },
            "servers": [
                {
                    "role": "control",
                    "start": { "kind": "controlServer" },
                    "port": { "kind": "allocate" },
                    "readiness": { "kind": "control" },
                    "after": [],
                    "restart": "on-failure",
                    "webview": false,
                    "required": true
                },
                {
                    "role": "ui",
                    "start": { "kind": "launchWorkflow" },
                    "port": { "kind": "config", "key": "launchArguments" },
                    "readiness": { "kind": "captured", "timeoutSeconds": 120 },
                    "after": ["control"],
                    "restart": "never",
                    "webview": true,
                    "required": true
                }
            ],
            "operations": [
                "update", "terminal", "versions", "extensions", "launch-arguments",
                "snapshots", "environment", "models", "hotpatch"
            ],
            "ownOperations": [],
            "checksUpdates": true
        })
    );
}

#[test]
fn every_built_in_type_runs_its_control_server_then_its_webui() {
    for descriptor in InstanceTypeRegistry::with_builtin_types().descriptors() {
        let order: Vec<&str> = server_start_order(&descriptor.servers)
            .unwrap()
            .into_iter()
            .map(|role| role.role.as_str())
            .collect();
        assert_eq!(order, ["control", "ui"], "{}", descriptor.id);
        let ui = &descriptor.servers[1];
        assert!(matches!(ui.readiness, ServerReadiness::Captured { .. }));
        assert_eq!(ui.restart, ServerRestart::Never);
        assert!(ui.webview);
        let control = &descriptor.servers[0];
        assert_eq!(control.readiness, ServerReadiness::Control);
        assert_eq!(control.restart, ServerRestart::OnFailure);
        assert!(!control.webview);
    }
}

/// A type that is only a descriptor, for the registry's own checks.
struct DescriptorOnly(InstanceTypeDescriptor);

impl InstanceProvider for DescriptorOnly {
    fn descriptor(&self) -> &InstanceTypeDescriptor {
        &self.0
    }

    fn install_form(
        &self,
    ) -> Vec<crate::internal::hanafubuki_foundation::form_schema::SchemaGroup> {
        Vec::new()
    }

    fn validate_spec(&self, _spec: Map<String, Value>) -> Result<InstanceSpec, String> {
        Err("no spec".to_string())
    }

    fn read_spec(&self, _spec: Map<String, Value>) -> Result<InstanceSpec, String> {
        Err("no spec".to_string())
    }
}

#[test]
fn a_type_whose_roles_wait_for_each_other_is_not_registered() {
    let mut descriptor = BuiltinProvider::new(WebUiFamily::Comfyui)
        .descriptor()
        .clone();
    descriptor.id = InstanceTypeId::parse("test:cycle").unwrap();
    descriptor.servers[0].after = vec![ServerRole::ui()];
    let registry = InstanceTypeRegistry::default();
    let error = registry
        .register(
            ContributionOwner::Extension("test".into()),
            Arc::new(DescriptorOnly(descriptor)),
        )
        .unwrap_err();
    assert_eq!(
        error,
        "instance type `test:cycle`: server roles control, ui wait for each other in a cycle"
    );
    assert!(registry.descriptors().is_empty());
}
