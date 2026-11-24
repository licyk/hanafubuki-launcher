use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_config::ConfigState;
use crate::internal::hanafubuki_config::model::paths::AppPaths;
use crate::internal::hanafubuki_config::repository::instances::{
    InstanceConfig, LaunchArgumentOrigin,
};
use crate::internal::hanafubuki_config::repository::safe_file;
use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_instance_contracts::WebUiFamily;
use crate::internal::hanafubuki_webui::instances::{
    WebUiInstanceInstallSource, WebUiInstanceLifecycle,
};
use serde::Deserialize;

use super::{reconcile, scan};
use crate::workspace::portable_config;
use crate::workspace::settings::{DiscoveredInstance, LauncherSettings};
use crate::workspace::test_support::tempdir;

fn create_directory(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(&path).unwrap();
    resolved(&path).native().to_path_buf()
}

fn create_file(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, []).unwrap();
}

fn create_python_runtime(root: &Path, relative: &str) -> PathBuf {
    let path = create_directory(root, relative);
    #[cfg(windows)]
    create_file(&path, "python.exe");
    #[cfg(not(windows))]
    create_file(&path, "bin/python");
    path
}

fn resolved(path: &Path) -> ResolvedPath {
    ResolvedPath::resolve(path).unwrap()
}

fn discovered(
    id: &str,
    family: WebUiFamily,
    instance_path: Option<&str>,
    core_path: &str,
    python_path: Option<&str>,
) -> DiscoveredInstance {
    DiscoveredInstance {
        instance_id: id.to_string(),
        instance_identity: format!("{id:0<32}"),
        name: family.product_name().to_string(),
        family,
        instance_path: instance_path.map(str::to_string),
        core_path: core_path.to_string(),
        python_path: python_path.map(str::to_string),
    }
}

fn settings(instances: Vec<DiscoveredInstance>) -> LauncherSettings {
    LauncherSettings {
        scan_completed: true,
        discovered_instances: instances,
    }
}

fn app_paths(app_data: &Path) -> AppPaths {
    AppPaths::from_app_data_dir(app_data.to_path_buf()).unwrap()
}

fn reconcile_for_test(
    launcher_settings: &mut LauncherSettings,
    launcher_directory: &Path,
    app_data: &Path,
) -> Result<bool, String> {
    let paths = app_paths(app_data);
    let config = ConfigState::default();
    reconcile(
        launcher_settings,
        &resolved(launcher_directory),
        &paths,
        &config,
    )
}

fn read_config(paths: &AppPaths, id: &str) -> InstanceConfig {
    safe_file::read_json(&paths.instance_config_file(id))
        .unwrap()
        .unwrap()
}

/// The record as the launcher wrote it; the application's reader decodes it
/// through the type registry, which the launcher does not compose.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrittenRecord {
    #[serde(rename = "type")]
    type_id: String,
    spec: serde_json::Value,
    install_source: WebUiInstanceInstallSource,
    lifecycle: WebUiInstanceLifecycle,
}

fn read_lifecycle(paths: &AppPaths, id: &str) -> WrittenRecord {
    safe_file::read_json(&paths.instance_lifecycle_file(id))
        .unwrap()
        .unwrap()
}

#[test]
fn scan_recognizes_every_family_and_both_trainer_layouts() {
    let temporary = tempdir();
    let root = create_directory(temporary.path(), "portable");
    let app_data = create_directory(temporary.path(), "app-data");

    create_file(&root, "sd/core/launch.py");
    create_file(&root, "sd/core/webui.py");
    create_directory(&root, "sd/core/modules");
    create_python_runtime(&root, "sd/python");

    create_file(&root, "comfy/core/main.py");
    create_file(&root, "comfy/core/nodes.py");
    create_directory(&root, "comfy/core/comfy");
    create_python_runtime(&root, "comfy/core/vendor/python");
    create_python_runtime(&root, "comfy/python");

    create_file(&root, "invoke/core/invokeai.yaml");

    create_file(&root, "fooocus/core/launch.py");
    create_directory(&root, "fooocus/core/ldm_patched");
    create_directory(&root, "fooocus/core/presets");
    create_directory(&root, "fooocus/core/wildcards");

    create_file(&root, "trainer-one/core/gui.py");
    create_directory(&root, "trainer-one/core/mikazuki");

    create_file(&root, "trainer-two/core/kohya_gui.py");
    create_directory(&root, "trainer-two/core/kohya_gui");

    create_file(&root, "qwen/core/launch.py");
    create_directory(&root, "qwen/core/qwen_tts_webui");

    create_file(&root, "rvc/core/launch.py");
    create_directory(&root, "rvc/core/rvc_next_webui");

    create_file(&root, "mizuoto/core/launch.py");
    create_directory(&root, "mizuoto/core/mizuoto_tts_webui");

    let mut progress = Vec::new();
    let found = scan(&resolved(&root), &resolved(&app_data), |event| {
        progress.push(event)
    })
    .unwrap();
    assert_eq!(found.len(), 9);
    let final_progress = progress.last().expect("final scan progress");
    assert_eq!(final_progress.webuis, 9);
    assert_eq!(final_progress.files, 14);
    assert!(final_progress.directories >= 20);

    let by_path = found
        .iter()
        .map(|entry| (entry.core_path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(by_path["sd/core"].family, WebUiFamily::StableDiffusionWebui);
    assert_eq!(by_path["sd/core"].instance_path.as_deref(), Some("sd"));
    assert_eq!(by_path["sd/core"].python_path.as_deref(), Some("sd/python"));
    assert_eq!(by_path["comfy/core"].family, WebUiFamily::Comfyui);
    assert_eq!(
        by_path["comfy/core"].python_path.as_deref(),
        Some("comfy/python")
    );
    assert_eq!(
        by_path["comfy/core"].instance_path.as_deref(),
        Some("comfy")
    );
    assert_eq!(by_path["invoke/core"].family, WebUiFamily::Invokeai);
    assert_eq!(by_path["invoke/core"].python_path, None);
    assert_eq!(by_path["fooocus/core"].family, WebUiFamily::Fooocus);
    assert_eq!(by_path["trainer-one/core"].family, WebUiFamily::SdTrainer);
    assert_eq!(by_path["trainer-two/core"].family, WebUiFamily::SdTrainer);
    assert_eq!(by_path["qwen/core"].family, WebUiFamily::QwenTts);
    assert_eq!(by_path["rvc/core"].family, WebUiFamily::RvcNext);
    assert_eq!(by_path["mizuoto/core"].family, WebUiFamily::MizuotoTts);
}

#[test]
fn scan_rejects_partial_features_and_accepts_unique_runtime_inside_core() {
    let temporary = tempdir();
    let root = create_directory(temporary.path(), "portable");
    let app_data = create_directory(temporary.path(), "app-data");

    create_file(&root, "partial-sd/core/launch.py");
    create_directory(&root, "partial-sd/core/modules");
    create_file(&root, "partial-comfy/core/main.py");
    create_directory(&root, "partial-comfy/core/comfy");

    create_file(&root, "qwen/core/launch.py");
    create_directory(&root, "qwen/core/qwen_tts_webui");
    create_python_runtime(&root, "qwen/core/py311");

    let found = scan(&resolved(&root), &resolved(&app_data), |_| {}).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].family, WebUiFamily::QwenTts);
    assert_eq!(found[0].instance_path.as_deref(), Some("qwen/core"));
    assert_eq!(found[0].python_path.as_deref(), Some("qwen/core/py311"));
}

#[test]
fn scan_leaves_ambiguous_direct_python_runtimes_incomplete() {
    let temporary = tempdir();
    let root = create_directory(temporary.path(), "portable");
    let app_data = create_directory(temporary.path(), "app-data");

    create_file(&root, "comfy/main.py");
    create_file(&root, "comfy/nodes.py");
    create_directory(&root, "comfy/comfy");
    create_python_runtime(&root, "comfy/py310");
    create_python_runtime(&root, "comfy/py311");

    let found = scan(&resolved(&root), &resolved(&app_data), |_| {}).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].family, WebUiFamily::Comfyui);
    assert_eq!(found[0].instance_path, None);
    assert_eq!(found[0].python_path, None);
}

#[test]
fn reconcile_creates_ready_and_incomplete_records() {
    let temporary = tempdir();
    let root = create_directory(temporary.path(), "portable");
    let app_data = create_directory(temporary.path(), "app-data");
    create_directory(&root, "ready/core");
    create_directory(&root, "ready/python");
    create_directory(&root, "incomplete/core");

    let mut launcher_settings = settings(vec![
        discovered(
            "launcher-ready",
            WebUiFamily::Comfyui,
            Some("ready"),
            "ready/core",
            Some("ready/python"),
        ),
        discovered(
            "launcher-incomplete",
            WebUiFamily::Fooocus,
            None,
            "incomplete/core",
            None,
        ),
    ]);
    assert!(!reconcile_for_test(&mut launcher_settings, &root, &app_data).unwrap());

    let paths = app_paths(&app_data);
    let ready = read_config(&paths, "launcher-ready");
    assert_eq!(
        ready.install_dir.as_deref(),
        Some(root.join("ready").as_path())
    );
    assert_eq!(
        ready.component_paths.as_ref().unwrap().python().unwrap(),
        root.join("ready/python")
    );
    assert_eq!(ready.launch_argument_origin, LaunchArgumentOrigin::BuiltIn);
    assert!(!ready.launch_args.is_empty());
    let ready_record = read_lifecycle(&paths, "launcher-ready");
    assert!(matches!(
        ready_record.lifecycle,
        WebUiInstanceLifecycle::Ready { .. }
    ));
    assert_eq!(ready_record.type_id, "builtin:comfyui");
    assert!(ready_record.spec.get("type").is_none());

    let incomplete = read_config(&paths, "launcher-incomplete");
    let incomplete_root = paths.managed_instances_dir.join("launcher-incomplete");
    assert_eq!(
        incomplete.install_dir.as_deref(),
        Some(incomplete_root.as_path())
    );
    assert_eq!(
        incomplete
            .component_paths
            .as_ref()
            .unwrap()
            .python()
            .unwrap(),
        incomplete_root.join("python")
    );
    assert!(incomplete_root.is_dir());
    assert_eq!(
        incomplete.launch_argument_origin,
        LaunchArgumentOrigin::PendingDefault
    );
    assert!(incomplete.launch_args.is_empty());
    assert!(matches!(
        read_lifecycle(&paths, "launcher-incomplete").install_source,
        WebUiInstanceInstallSource::DiscoveredCore
    ));
    assert!(matches!(
        read_lifecycle(&paths, "launcher-incomplete").lifecycle,
        WebUiInstanceLifecycle::Created
    ));
}

#[test]
fn reconcile_uses_core_as_the_instance_root_for_nested_python() {
    let temporary = tempdir();
    let root = create_directory(temporary.path(), "portable");
    let app_data = create_directory(temporary.path(), "app-data");
    create_directory(&root, "nested/core/python");

    let mut launcher_settings = settings(vec![discovered(
        "launcher-nested",
        WebUiFamily::Comfyui,
        Some("nested/core"),
        "nested/core",
        Some("nested/core/python"),
    )]);
    reconcile_for_test(&mut launcher_settings, &root, &app_data).unwrap();

    let config = read_config(&app_paths(&app_data), "launcher-nested");
    let core = root.join("nested/core");
    assert_eq!(config.install_dir.as_deref(), Some(core.as_path()));
    let components = config.component_paths.unwrap();
    assert_eq!(components.core(), core);
    assert_eq!(
        components.python().unwrap(),
        root.join("nested/core/python")
    );
}

#[test]
fn reconcile_rebases_paths_without_resetting_lifecycle() {
    let temporary = tempdir();
    let first_root = create_directory(temporary.path(), "first");
    let second_root = create_directory(temporary.path(), "second");
    let app_data = create_directory(temporary.path(), "app-data");
    create_directory(&first_root, "instance/core");
    create_directory(&first_root, "instance/python");
    create_directory(&second_root, "instance/core");
    create_directory(&second_root, "instance/python");

    let mut launcher_settings = settings(vec![discovered(
        "launcher-moved",
        WebUiFamily::StableDiffusionWebui,
        Some("instance"),
        "instance/core",
        Some("instance/python"),
    )]);
    reconcile_for_test(&mut launcher_settings, &first_root, &app_data).unwrap();
    let paths = app_paths(&app_data);
    let lifecycle = read_lifecycle(&paths, "launcher-moved");

    reconcile_for_test(&mut launcher_settings, &second_root, &app_data).unwrap();
    let moved = read_config(&paths, "launcher-moved");
    assert_eq!(moved.config_revision, 2);
    assert_eq!(
        moved.install_dir.as_deref(),
        Some(second_root.join("instance").as_path())
    );
    assert_eq!(
        moved.component_paths.as_ref().unwrap().core(),
        second_root.join("instance/core")
    );
    assert_eq!(read_lifecycle(&paths, "launcher-moved"), lifecycle);
}

#[test]
fn missing_webui_prunes_scan_result_but_preserves_all_records() {
    let temporary = tempdir();
    let root = create_directory(temporary.path(), "portable");
    let app_data = create_directory(temporary.path(), "app-data");
    create_directory(&root, "instance/core");

    let mut launcher_settings = settings(vec![discovered(
        "launcher-pruned",
        WebUiFamily::Invokeai,
        None,
        "instance/core",
        None,
    )]);
    reconcile_for_test(&mut launcher_settings, &root, &app_data).unwrap();
    let paths = app_paths(&app_data);
    let manual_dir = paths.instance_record_dir("manual-instance");
    fs::create_dir_all(&manual_dir).unwrap();
    let manual_file = manual_dir.join("config.json");
    fs::write(&manual_file, b"manual sentinel").unwrap();

    fs::remove_dir_all(root.join("instance/core")).unwrap();
    assert!(reconcile_for_test(&mut launcher_settings, &root, &app_data).unwrap());
    assert!(launcher_settings.discovered_instances.is_empty());
    assert!(paths.instance_config_file("launcher-pruned").is_file());
    assert!(paths.instance_lifecycle_file("launcher-pruned").is_file());
    assert_eq!(fs::read(manual_file).unwrap(), b"manual sentinel");
}

#[test]
fn portable_configuration_normalizes_removed_component_fields_before_reconcile() {
    let temporary = tempdir();
    let root = create_directory(temporary.path(), "portable");
    let app_data = create_directory(temporary.path(), "app-data");
    create_directory(&root, "instance/core");
    create_directory(&root, "instance/python");

    let mut launcher_settings = settings(vec![discovered(
        "launcher-normalized",
        WebUiFamily::Comfyui,
        Some("instance"),
        "instance/core",
        Some("instance/python"),
    )]);
    reconcile_for_test(&mut launcher_settings, &root, &app_data).unwrap();

    let paths = app_paths(&app_data);
    let config_file = paths.instance_config_file("launcher-normalized");
    let mut raw: serde_json::Value = safe_file::read_json(&config_file).unwrap().unwrap();
    raw["componentPaths"]["python"]
        .as_object_mut()
        .unwrap()
        .insert("ownership".to_string(), serde_json::json!("managed"));
    raw["componentPaths"]["core"]
        .as_object_mut()
        .unwrap()
        .insert("ownership".to_string(), serde_json::json!("managed"));
    safe_file::write_json_atomic(&config_file, &raw).unwrap();

    let prepared = portable_config::reconcile(&resolved(&app_data)).unwrap();
    let normalized: serde_json::Value = safe_file::read_json(&config_file).unwrap().unwrap();
    assert!(
        normalized["componentPaths"]["python"]
            .get("ownership")
            .is_none()
    );
    assert!(
        normalized["componentPaths"]["core"]
            .get("ownership")
            .is_none()
    );
    assert!(!prepared.config_state.recovery_reports().is_empty());
    assert!(
        !reconcile(
            &mut launcher_settings,
            &resolved(&root),
            &prepared.app_paths,
            &prepared.config_state,
        )
        .unwrap()
    );
}

#[cfg(unix)]
#[test]
fn scan_does_not_follow_directory_symlinks() {
    use std::os::unix::fs::symlink;

    let temporary = tempdir();
    let root = create_directory(temporary.path(), "portable");
    let app_data = create_directory(temporary.path(), "app-data");
    let external = create_directory(temporary.path(), "external");
    create_file(&external, "launch.py");
    create_file(&external, "webui.py");
    create_directory(&external, "modules");
    symlink(&external, root.join("linked-webui")).unwrap();

    let found = scan(&resolved(&root), &resolved(&app_data), |_| {}).unwrap();
    assert!(found.is_empty());
}
