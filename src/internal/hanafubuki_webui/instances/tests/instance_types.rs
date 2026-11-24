//! Records through the type registry: today's records under the built-in
//! types, and a record whose type has no provider.

use crate::internal::hanafubuki_foundation::module_faults::ContributionOwner;
use std::collections::BTreeSet;

use super::*;
use crate::internal::hanafubuki_instance_contracts::InstanceTypeId;
use crate::internal::hanafubuki_python_runtime::PythonVersion;
use crate::internal::hanafubuki_webui::instance_types::BuiltinProvider;
use serde_json::{Value, json};

/// A record directory as this build's predecessor wrote it.
fn write_record(paths: &AppPaths, instance_id: &str, lifecycle: Value, install_dir: &Path) {
    let directory = paths.instance_record_dir(instance_id);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        paths.instance_lifecycle_file(instance_id),
        serde_json::to_vec_pretty(&lifecycle).unwrap(),
    )
    .unwrap();
    std::fs::write(
        paths.instance_config_file(instance_id),
        serde_json::to_vec_pretty(&json!({
            "id": instance_id,
            "name": instance_id,
            "configRevision": 1,
            "installDir": install_dir,
            "componentPaths": {
                "python": { "path": install_dir.join("python") },
                "core": { "path": install_dir.join("core") }
            },
            "quickPaths": [],
            "envVars": {},
            "launchArgs": [],
            "launchArgumentOrigin": "pendingDefault",
            "settingsOverrides": {}
        }))
        .unwrap(),
    )
    .unwrap();
}

/// The spec as the predecessor wrote it: tagged with the family, every
/// declared field present.
fn legacy_spec(family: WebUiFamily) -> Value {
    let fields = json!({ "pythonVersion": "3.11" });
    let spec = WebUiInstallSpec::from_object(family, fields.as_object().unwrap().clone()).unwrap();
    serde_json::to_value(spec).unwrap()
}

fn record_files(paths: &AppPaths, instance_id: &str) -> BTreeSet<String> {
    std::fs::read_dir(paths.instance_record_dir(instance_id))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn todays_records_read_as_their_built_in_type_and_write_the_new_shape() {
    let (root, service) = setup("type-migration", vec![]);
    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    for family in WebUiFamily::ALL {
        let instance_id = format!("legacy-{}", family.cli_command());
        let install_dir = root.join("instances").join(&instance_id);
        write_record(
            &paths,
            &instance_id,
            json!({
                "instanceId": instance_id,
                "instanceIdentity": format!("identity-{instance_id}"),
                "spec": legacy_spec(family),
                "installSource": { "type": "standard" },
                "lifecycle": { "state": "created" },
                "createdAt": 1,
                "updatedAt": 1
            }),
            &install_dir,
        );
        let lifecycle_file = paths.instance_lifecycle_file(&instance_id);
        let written = std::fs::read(&lifecycle_file).unwrap();
        let files = record_files(&paths, &instance_id);

        let snapshot = service.get(&instance_id).unwrap();
        assert_eq!(
            snapshot.install.type_id,
            BuiltinProvider::type_id(family),
            "{family:?}"
        );
        let spec = snapshot.install.install_spec().unwrap();
        assert_eq!(spec.family(), family);
        assert_eq!(spec.python_version(), Some(PythonVersion::Python311));
        assert_eq!(snapshot.availability, InstanceTypeAvailability::Available);
        let components = snapshot.instance.component_paths.as_ref().unwrap();
        assert_eq!(
            components.python(),
            Some(install_dir.join("python").as_path())
        );
        assert_eq!(components.core(), install_dir.join("core"));
        // Reading moves nothing: no rewrite, no quarantine copy.
        assert_eq!(std::fs::read(&lifecycle_file).unwrap(), written);
        assert_eq!(record_files(&paths, &instance_id), files);

        service
            .transition(&instance_id, WebUiInstanceLifecycle::Created)
            .unwrap();
        let rewritten = read_json(&lifecycle_file);
        assert_eq!(
            rewritten["type"],
            format!("builtin:{}", family.cli_command())
        );
        assert!(rewritten["spec"].get("type").is_none(), "{rewritten}");
        assert_eq!(rewritten["spec"]["pythonVersion"], "3.11");
        assert_eq!(
            read_json(&paths.instance_config_file(&instance_id))["componentPaths"],
            json!({
                "python": { "path": install_dir.join("python") },
                "core": { "path": install_dir.join("core") }
            })
        );

        let stable = std::fs::read(&lifecycle_file).unwrap();
        let again = service.get(&instance_id).unwrap();
        assert_eq!(again.install, service.get(&instance_id).unwrap().install);
        assert_eq!(again.install.install_spec().unwrap(), spec);
        assert_eq!(std::fs::read(&lifecycle_file).unwrap(), stable);
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_record_naming_neither_a_type_nor_a_family_is_quarantined() {
    let (root, service) = setup("type-unusable", vec![]);
    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    for (instance_id, lifecycle) in [
        (
            "unknown-family",
            json!({ "spec": { "type": "retiredFamily" } }),
        ),
        ("invalid-type", json!({ "type": "Not A Type", "spec": {} })),
    ] {
        let mut lifecycle = lifecycle;
        let object = lifecycle.as_object_mut().unwrap();
        object.insert("instanceId".to_string(), json!(instance_id));
        object.insert("instanceIdentity".to_string(), json!("identity"));
        object.insert("installSource".to_string(), json!({ "type": "standard" }));
        object.insert("lifecycle".to_string(), json!({ "state": "created" }));
        object.insert("createdAt".to_string(), json!(1));
        object.insert("updatedAt".to_string(), json!(1));
        write_record(&paths, instance_id, lifecycle, &root.join(instance_id));
        let error = service.get(instance_id).unwrap_err();
        assert!(error.contains("quarantined"), "{error}");
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_instance_of_an_unavailable_type_is_listed_and_refuses_all_but_record_deletion() {
    let (root, service) = setup("type-unavailable", vec![]);
    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    let available = create(&service, "Available");
    service.require_available(&available.instance.id).unwrap();
    let instance_id = "missing-type";
    let install_dir = root.join("instances").join(instance_id);
    std::fs::create_dir_all(install_dir.join("core")).unwrap();
    std::fs::write(install_dir.join("core").join("keep.txt"), b"user data").unwrap();
    let lifecycle = json!({
        "instanceId": instance_id,
        "instanceIdentity": "identity-missing",
        "type": "ext.missing:thing",
        "spec": { "model": "llama", "type": "kept as written", "nested": { "a": [1, 2] } },
        "installSource": { "type": "standard" },
        "lifecycle": { "state": "ready", "operationId": "install-1" },
        "createdAt": 1,
        "updatedAt": 2,
        "fromANewerBuild": true
    });
    write_record(&paths, instance_id, lifecycle.clone(), &install_dir);
    let lifecycle_file = paths.instance_lifecycle_file(instance_id);
    let written = std::fs::read(&lifecycle_file).unwrap();
    let reason = "type `ext.missing:thing` is not available";

    let listed = service.list().unwrap();
    assert_eq!(listed.len(), 2);
    let snapshot = listed
        .into_iter()
        .find(|snapshot| snapshot.instance.id == instance_id)
        .unwrap();
    assert_eq!(
        snapshot.install.type_id,
        InstanceTypeId::parse("ext.missing:thing").unwrap()
    );
    assert_eq!(
        snapshot.availability,
        InstanceTypeAvailability::Unavailable {
            reason: reason.to_string()
        }
    );
    assert_eq!(snapshot.install.install_spec().unwrap_err(), reason);
    // Its paths are shown as configured.
    assert_eq!(snapshot.install_dir, install_dir);
    assert_eq!(
        snapshot.instance.component_paths.as_ref().unwrap().core(),
        install_dir.join("core")
    );
    for action in [
        &snapshot.install_action,
        &snapshot.reinstall_action,
        &snapshot.force_recreate_python_action,
        &snapshot.force_recreate_core_action,
        &snapshot.reinstall_spec_action,
    ] {
        assert_eq!(
            *action,
            WebUiInstanceInstallAction::Unavailable {
                reason: reason.to_string()
            }
        );
    }
    let serialized = serde_json::to_value(&snapshot.install).unwrap();
    assert_eq!(serialized["type"], "ext.missing:thing");
    assert_eq!(serialized["spec"], lifecycle["spec"]);

    let delete = DeleteInstanceRequest {
        instance_id: instance_id.to_string(),
        expected_instance_identity: "identity-missing".to_string(),
        expected_config_revision: 1,
    };
    let refusals = [
        service.require_available(instance_id),
        service.start_install(instance_id).map(|_| ()),
        service
            .start_reinstall(ReinstallWebUiInstanceRequest {
                instance_id: instance_id.to_string(),
                expected_instance_identity: "identity-missing".to_string(),
                expected_config_revision: 1,
                force_recreate_python: false,
                force_recreate_core: false,
                spec: None,
            })
            .map(|_| ()),
        service
            .rename(RenameWebUiInstanceRequest {
                instance_id: instance_id.to_string(),
                expected_instance_identity: "identity-missing".to_string(),
                expected_config_revision: 1,
                name: "Renamed".to_string(),
            })
            .map(|_| ()),
        service
            .update_quick_paths(&UpdateWebUiInstanceQuickPathsRequest {
                instance_id: instance_id.to_string(),
                expected_config_revision: 1,
                quick_paths: Vec::new(),
            })
            .map(|_| ()),
        service
            .update_component_paths(&UpdateWebUiInstanceComponentPathsRequest {
                instance_id: instance_id.to_string(),
                expected_config_revision: 1,
                component_paths: InstanceComponentPaths::new(install_dir.join("elsewhere")),
            })
            .map(|_| ()),
        service
            .preview_migration(&InstanceMigrationRequest {
                instance_id: instance_id.to_string(),
                expected_instance_identity: "identity-missing".to_string(),
                expected_config_revision: 1,
                destination_parent_dir: root.join("moved"),
            })
            .map(|_| ()),
        service.begin_instance_deletion(&delete).map(|_| ()),
        service
            .validate_delete_request(&delete)
            .and_then(|snapshot| service.delete_instance_files(&snapshot)),
    ];
    for refusal in refusals {
        assert_eq!(refusal.unwrap_err(), reason);
    }
    assert_eq!(std::fs::read(&lifecycle_file).unwrap(), written);

    service.begin_instance_record_deletion(&delete).unwrap();
    service
        .delete_state_if_identity(instance_id, "identity-missing")
        .unwrap();
    assert!(!paths.instance_record_dir(instance_id).exists());
    assert_eq!(
        std::fs::read(install_dir.join("core").join("keep.txt")).unwrap(),
        b"user data"
    );
    let remaining: Vec<String> = service
        .list()
        .unwrap()
        .into_iter()
        .map(|snapshot| snapshot.instance.id)
        .collect();
    assert_eq!(remaining, [available.instance.id]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_type_declaring_its_own_components_lays_them_out_and_only_them() {
    use crate::internal::hanafubuki_instance_contracts::InstanceTypeDescriptor;
    use crate::internal::hanafubuki_webui::instance_types::{
        InstanceProvider, InstanceTypeRegistry,
    };

    /// A test-only type: ComfyUI's spec, with a `models` component.
    struct WithModels(InstanceTypeDescriptor, BuiltinProvider);
    impl InstanceProvider for WithModels {
        fn descriptor(&self) -> &InstanceTypeDescriptor {
            &self.0
        }
        fn install_form(
            &self,
        ) -> Vec<crate::internal::hanafubuki_foundation::form_schema::SchemaGroup> {
            self.1.install_form()
        }
        fn validate_spec(
            &self,
            spec: serde_json::Map<String, Value>,
        ) -> Result<InstanceSpec, String> {
            self.1.validate_spec(spec)
        }
        fn read_spec(&self, spec: serde_json::Map<String, Value>) -> Result<InstanceSpec, String> {
            self.1.read_spec(spec)
        }
    }
    let builtin = BuiltinProvider::new(WebUiFamily::Comfyui);
    let mut descriptor = builtin.descriptor().clone();
    descriptor.id = InstanceTypeId::parse("test:with-models").unwrap();
    descriptor.components.push("models".to_string());
    let registry = InstanceTypeRegistry::with_builtin_types();
    registry
        .register(
            ContributionOwner::Extension("test".into()),
            Arc::new(WithModels(descriptor, builtin)),
        )
        .unwrap();

    let root = crate::internal::hanafubuki_webui::test_support::temp_root().join(format!(
        "hanafubuki-webui-instances-declared-components-{}-{}",
        std::process::id(),
        NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    let service = WebUiInstanceService::new(
        super::repository(paths, Arc::new(ConfigState::default()), Arc::new(registry)),
        InstanceInstallService::new(Arc::new(FakeBackend::new(
            root.join("install-root"),
            vec![],
        ))),
    )
    .unwrap();
    let created = service
        .create(CreateWebUiInstanceRequest {
            name: Some("Models".to_string()),
            install_dir: None,
            component_paths: Default::default(),
            type_id: InstanceTypeId::parse("test:with-models").unwrap(),
            spec: serde_json::Map::new(),
            preset: None,
        })
        .unwrap();
    let components = created.instance.component_paths.clone().unwrap();
    assert_eq!(
        components.names().collect::<Vec<_>>(),
        ["core", "models", "python"]
    );
    assert_eq!(
        components.get("models"),
        Some(created.install_dir.join("models").as_path())
    );

    // A path change names exactly the declared components.
    let request = |component_paths| UpdateWebUiInstanceComponentPathsRequest {
        instance_id: created.instance.id.clone(),
        expected_config_revision: created.instance.config_revision,
        component_paths,
    };
    let two = InstanceComponentPaths::python_and_core(
        created.install_dir.join("python"),
        created.install_dir.join("core"),
    );
    let error = service
        .update_component_paths(&request(two.clone()))
        .unwrap_err();
    assert_eq!(
        error,
        "componentPaths must name exactly the components of type `test:with-models`: core, python, models"
    );
    let moved = service
        .update_component_paths(&request(two.with("models", root.join("shared-models"))))
        .unwrap();
    assert_eq!(
        moved.instance.component_paths.unwrap().get("models"),
        Some(root.join("shared-models").as_path())
    );
    let _ = std::fs::remove_dir_all(root);
}
