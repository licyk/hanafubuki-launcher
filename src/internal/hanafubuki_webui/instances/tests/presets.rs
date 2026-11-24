//! An instance made from a preset: its identity is its base type, the
//! preset is named for display only, the preset's launch arguments are
//! kept with the instance and stay over the built-in defaults whenever
//! those are set again, a user's edit wins over both, and a preset removed
//! later changes nothing about it.

use crate::internal::hanafubuki_instance_contracts::InstanceTypeId;
use serde_json::{Map, Value, json};

use super::*;
use crate::internal::hanafubuki_webui::instance_types::presets::{
    LaunchArgument, LaunchValue, PresetId, SaveInstancePresetRequest,
};

fn flag(name: &str, value: bool) -> LaunchArgument {
    LaunchArgument {
        name: name.to_string(),
        value: LaunchValue::Flag(value),
    }
}

fn save_preset(service: &WebUiInstanceService) -> PresetId {
    service
        .presets()
        .save(SaveInstancePresetRequest {
            base: type_of(&spec()),
            label: "Low VRAM".to_string(),
            description: None,
            values: json!({ "pythonVersion": "3.12" })
                .as_object()
                .unwrap()
                .clone(),
            launch_arguments: vec![flag("--lowvram", true), flag("--auto-launch", false)],
        })
        .unwrap()
        .id
}

/// What the creation form submits once a preset filled it: the values it
/// shows that are set.
fn filled_spec(service: &WebUiInstanceService, id: &PresetId) -> Map<String, Value> {
    let listing = service.presets().list(Some(&type_of(&spec()))).unwrap();
    let entry = listing
        .presets
        .into_iter()
        .find(|entry| entry.id == *id)
        .unwrap();
    entry
        .values
        .into_iter()
        .filter(|(_, value)| !value.is_null())
        .collect()
}

fn create_from(
    service: &WebUiInstanceService,
    values: Map<String, Value>,
    preset: Option<PresetId>,
) -> Result<WebUiInstanceSnapshot, String> {
    service.create(CreateWebUiInstanceRequest {
        name: Some("From a preset".to_string()),
        install_dir: None,
        component_paths: Default::default(),
        type_id: type_of(&spec()),
        spec: values,
        preset,
    })
}

#[test]
fn an_instance_made_from_a_saved_preset_keeps_its_base_type_id() {
    let (root, service) = setup(
        "preset-instance",
        vec![InstanceInstallTaskState::Exited { exit_code: 0 }],
    );
    let id = save_preset(&service);
    assert_eq!(id.to_string(), "user:low-vram");
    let created = create_from(&service, filled_spec(&service, &id), Some(id.clone())).unwrap();
    let instance_id = created.instance.id.clone();
    assert_eq!(created.install.type_id, type_of(&spec()));
    assert_eq!(created.install.preset_id.as_deref(), Some("user:low-vram"));
    assert_eq!(
        created.install.install_spec().unwrap().python_version(),
        Some(crate::internal::hanafubuki_python_runtime::PythonVersion::Python312)
    );

    // The preset's arguments over the type's defaults, set again by the
    // install's completion.
    let mut expected = default_launch_arguments(&spec());
    expected.retain(|argument| argument != "--auto-launch");
    expected.push("--lowvram".to_string());
    assert_eq!(created.instance.launch_args, expected);
    assert_eq!(
        created.instance.launch_argument_origin,
        LaunchArgumentOrigin::BuiltIn
    );
    assert_eq!(
        created.install.preset_launch_arguments,
        [flag("--lowvram", true), flag("--auto-launch", false)]
    );
    service.start_install(&instance_id).unwrap();
    let ready = wait_lifecycle(&service, &instance_id, |lifecycle| {
        matches!(lifecycle, WebUiInstanceLifecycle::Ready { .. })
    });
    assert_eq!(ready.instance.launch_args, expected);

    // The preset goes; the instance is as it was, and reading its record
    // repairs nothing.
    service.presets().delete("user:low-vram").unwrap();
    let lifecycle = AppPaths::from_app_data_dir(root.clone())
        .unwrap()
        .instance_lifecycle_file(&instance_id);
    let written = std::fs::read(&lifecycle).unwrap();
    let after = service.get(&instance_id).unwrap();
    assert_eq!(after.availability, InstanceTypeAvailability::Available);
    assert_eq!(after.install.type_id, type_of(&spec()));
    assert_eq!(after.install.preset_id.as_deref(), Some("user:low-vram"));
    assert_eq!(std::fs::read(&lifecycle).unwrap(), written);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_submitted_spec_is_final_over_its_preset() {
    let (root, service) = setup("preset-cleared", vec![]);
    let id = save_preset(&service);
    // The user cleared the Python version the preset filled in.
    let created = create_from(&service, Map::new(), Some(id)).unwrap();
    assert_eq!(
        created.install.install_spec().unwrap().python_version(),
        None
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_preset_creation_needs_a_usable_preset_over_its_type() {
    let (root, service) = setup("preset-refused", vec![]);
    let unknown = PresetId::parse("user:missing").unwrap();
    assert_eq!(
        create_from(&service, Map::new(), Some(unknown)).unwrap_err(),
        "there is no preset `user:missing`"
    );
    let plain = create_from(&service, Map::new(), None).unwrap();
    assert_eq!(plain.install.preset_id, None);
    assert!(plain.instance.launch_args.is_empty());
    assert_eq!(
        plain.instance.launch_argument_origin,
        LaunchArgumentOrigin::PendingDefault
    );
    let _ = std::fs::remove_dir_all(root);
}

/// A preset over the Stable Diffusion WebUI type, whose defaults follow
/// its branch: `--autolaunch` removed, `--theme` replaced, `--lowvram`
/// added.
fn save_branch_preset(service: &WebUiInstanceService) -> PresetId {
    service
        .presets()
        .save(SaveInstancePresetRequest {
            base: stable_type(),
            label: "Light theme".to_string(),
            description: None,
            values: Map::new(),
            launch_arguments: vec![
                flag("--autolaunch", false),
                LaunchArgument {
                    name: "--theme".to_string(),
                    value: LaunchValue::Text("light".to_string()),
                },
                flag("--lowvram", true),
            ],
        })
        .unwrap()
        .id
}

fn stable_type() -> InstanceTypeId {
    type_of(&stable_spec("sd_webui_main"))
}

fn create_on_branch(service: &WebUiInstanceService, preset: PresetId) -> WebUiInstanceSnapshot {
    service
        .create(CreateWebUiInstanceRequest {
            name: Some("Branch".to_string()),
            install_dir: None,
            component_paths: Default::default(),
            type_id: stable_type(),
            spec: stable_spec("sd_webui_main").to_object(),
            preset: Some(preset),
        })
        .unwrap()
}

fn switch_branch(
    service: &WebUiInstanceService,
    instance_id: &str,
    branch: &str,
) -> WebUiInstanceSnapshot {
    let current = service.get(instance_id).unwrap();
    service
        .persist_branch_preset_if_identity(
            instance_id,
            &current.install.instance_identity,
            branch.to_string(),
        )
        .unwrap();
    service.get(instance_id).unwrap()
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn a_presets_arguments_stay_over_the_defaults_a_branch_switch_sets() {
    let (root, service) = setup("preset-branch", vec![]);
    let id = save_branch_preset(&service);
    let created = create_on_branch(&service, id.clone());
    let instance_id = created.instance.id.clone();
    let main = created.instance.launch_args;
    assert_eq!(&main[..2], ["--theme", "light"]);
    assert!(!main.contains(&"--autolaunch".to_string()));
    assert_eq!(main.last().map(String::as_str), Some("--lowvram"));

    // The new branch's defaults, the preset's arguments over them.
    let next = switch_branch(&service, &instance_id, "sd_next_main");
    assert_eq!(
        next.instance.launch_args,
        strings(&["--use-cuda", "--theme", "light", "--lowvram"])
    );
    assert_eq!(
        next.instance.launch_argument_origin,
        LaunchArgumentOrigin::BuiltIn
    );

    // They are the instance's: the preset going changes nothing.
    service.presets().delete(&id.to_string()).unwrap();
    let back = switch_branch(&service, &instance_id, "sd_webui_main");
    assert_eq!(back.instance.launch_args, main);

    // The user's own arguments win over both.
    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    update_instance_launch_args_inner(
        &paths,
        &ConfigState::default(),
        &instance_id,
        back.instance.config_revision,
        strings(&["--medvram"]),
    )
    .unwrap();
    let edited = switch_branch(&service, &instance_id, "sd_next_main");
    assert_eq!(edited.instance.launch_args, strings(&["--medvram"]));
    assert_eq!(
        edited.instance.launch_argument_origin,
        LaunchArgumentOrigin::User
    );
    let _ = std::fs::remove_dir_all(root);
}

/// A record written before the preset's arguments were kept: they were
/// stored as the user's, which a branch switch keeps, and reading it
/// repairs nothing.
#[test]
fn a_record_without_its_presets_arguments_keeps_them_as_the_users() {
    let (root, service) = setup("preset-old-record", vec![]);
    let created = create_on_branch(&service, save_branch_preset(&service));
    let instance_id = created.instance.id.clone();
    let paths = AppPaths::from_app_data_dir(root.clone()).unwrap();
    let lifecycle = paths.instance_lifecycle_file(&instance_id);
    let mut record: Value = serde_json::from_slice(&std::fs::read(&lifecycle).unwrap()).unwrap();
    record
        .as_object_mut()
        .unwrap()
        .remove("presetLaunchArguments")
        .unwrap();
    std::fs::write(&lifecycle, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
    update_instance_launch_args_inner(
        &paths,
        &ConfigState::default(),
        &instance_id,
        created.instance.config_revision,
        created.instance.launch_args.clone(),
    )
    .unwrap();
    let written = std::fs::read(&lifecycle).unwrap();

    let read = service.get(&instance_id).unwrap();
    assert!(read.install.preset_launch_arguments.is_empty());
    assert_eq!(std::fs::read(&lifecycle).unwrap(), written);
    let switched = switch_branch(&service, &instance_id, "sd_next_main");
    assert_eq!(switched.instance.launch_args, created.instance.launch_args);
    assert_eq!(
        switched.instance.launch_argument_origin,
        LaunchArgumentOrigin::User
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_preset_id_or_arguments_that_do_not_read_are_dropped_from_the_record() {
    let (root, service) = setup("preset-record", vec![]);
    let created = create(&service, "Plain");
    let lifecycle = AppPaths::from_app_data_dir(root.clone())
        .unwrap()
        .instance_lifecycle_file(&created.instance.id);
    let mut record: Value = serde_json::from_slice(&std::fs::read(&lifecycle).unwrap()).unwrap();
    assert!(record.get("presetId").is_none());
    record["presetId"] = json!(7);
    // Arguments a preset could not hold are dropped together.
    record["presetLaunchArguments"] = json!([
        { "name": "--lowvram", "value": true },
        { "name": "lowvram", "value": true }
    ]);
    std::fs::write(&lifecycle, record.to_string()).unwrap();
    let read = service.get(&created.instance.id).unwrap();
    assert_eq!(read.install.preset_id, None);
    assert!(read.install.preset_launch_arguments.is_empty());
    assert_eq!(read.availability, InstanceTypeAvailability::Available);
    let _ = std::fs::remove_dir_all(root);
}
