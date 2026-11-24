use super::*;
use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use serde_json::json;
use std::collections::BTreeMap;

fn config(id: &str, root: &str) -> InstanceConfig {
    let root = PathBuf::from(root);
    InstanceConfig {
        id: id.to_string(),
        name: id.to_string(),
        config_revision: 1,
        install_dir: Some(root.clone()),
        component_paths: Some(InstanceComponentPaths::defaults(
            &root,
            InstanceComponentPaths::PYTHON_AND_CORE,
        )),
        quick_paths: Vec::new(),
        env_vars: BTreeMap::new(),
        launch_args: Vec::new(),
        launch_argument_origin:
            crate::internal::hanafubuki_config::repository::instances::LaunchArgumentOrigin::User,
        settings_overrides: json!({}),
    }
}

fn absolute_root(label: &str) -> PathBuf {
    crate::internal::hanafubuki_config::test_support::temp_root().join(format!(
        "hanafubuki-instance-path-{label}-{}",
        std::process::id()
    ))
}

fn migration_root(label: &str) -> PathBuf {
    let root = crate::internal::hanafubuki_config::test_support::temp_root().join(format!(
        "hanafubuki-instance-path-migration-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("source/python")).unwrap();
    std::fs::create_dir_all(root.join("source/core")).unwrap();
    root
}

#[test]
fn component_scope_is_derived_from_path_containment() {
    let root = PathBuf::from("/instances/demo");
    assert!(InstancePathPolicy::component_is_in_instance_root(
        &root,
        &root.join("python")
    ));
    assert!(!InstancePathPolicy::component_is_in_instance_root(
        &root,
        &PathBuf::from("/shared/python")
    ));
    assert!(!InstancePathPolicy::component_is_in_instance_root(
        &root, &root
    ));
}

#[test]
fn accepts_root_as_core_with_python_nested_below_it() {
    let root = absolute_root("portable-core");
    let paths = InstanceComponentPaths::python_and_core(root.join("python"), root.clone());

    let validated =
        InstancePathPolicy::validate_layout("portable", root.clone(), paths, &[]).unwrap();
    assert_eq!(validated.install_dir, root);
    assert_eq!(validated.component_paths.core(), validated.install_dir);
}

#[test]
fn rejects_nested_python_when_core_is_not_the_instance_root() {
    let root = absolute_root("portable");
    let core = root.join("core");
    let paths = InstanceComponentPaths::python_and_core(core.join("python"), core);

    assert!(
        InstancePathPolicy::validate_layout("portable", root, paths, &[])
            .unwrap_err()
            .contains("only when core equals the instance root")
    );
}

#[test]
fn migration_relocates_internal_components_and_preserves_those_outside_the_root() {
    let root = migration_root("containment");
    let source = root.join("source");
    let destination_parent = root.join("destination");
    std::fs::create_dir(&destination_parent).unwrap();
    let destination = destination_parent.join("source");
    let external = root.join("shared-python");
    std::fs::create_dir(&external).unwrap();
    let paths = InstanceComponentPaths::python_and_core(external.clone(), source.join("core"));

    let planned =
        InstancePathPolicy::plan_migration("one", source, destination_parent, paths, &[]).unwrap();
    assert_eq!(
        planned.destination_component_paths.python().unwrap(),
        external
    );
    assert_eq!(
        planned.destination_component_paths.core(),
        destination.join("core")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn migration_relocates_a_root_as_core_layout() {
    let root = crate::internal::hanafubuki_config::test_support::temp_root().join(format!(
        "hanafubuki-instance-path-root-core-migration-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    let destination_parent = root.join("destination");
    std::fs::create_dir_all(source.join("python")).unwrap();
    std::fs::create_dir(&destination_parent).unwrap();
    let destination = destination_parent.join("source");
    let paths = InstanceComponentPaths::python_and_core(source.join("python"), source.clone());

    let planned =
        InstancePathPolicy::plan_migration("root-core", source, destination_parent, paths, &[])
            .unwrap();
    assert_eq!(planned.destination_install_dir, destination);
    assert_eq!(
        planned.destination_component_paths.core(),
        planned.destination_install_dir
    );
    assert_eq!(
        planned.destination_component_paths.python().unwrap(),
        planned.destination_install_dir.join("python")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn migration_requires_an_existing_parent_and_an_absent_final_destination() {
    let root = migration_root("invalid-destinations");
    let source = root.join("source");
    let paths = InstanceComponentPaths::defaults(&source, InstanceComponentPaths::PYTHON_AND_CORE);
    std::fs::create_dir(source.join("child")).unwrap();
    assert!(
        InstancePathPolicy::plan_migration(
            "one",
            source.clone(),
            PathBuf::from("relative"),
            paths.clone(),
            &[]
        )
        .is_err()
    );
    assert!(
        InstancePathPolicy::plan_migration(
            "one",
            source.clone(),
            source.join("child"),
            paths.clone(),
            &[]
        )
        .unwrap_err()
        .contains("must not overlap")
    );
    assert!(
        InstancePathPolicy::plan_migration("one", source.clone(), root.clone(), paths.clone(), &[])
            .unwrap_err()
            .contains("must not overlap")
    );
    let destination_parent = root.join("existing");
    std::fs::create_dir(&destination_parent).unwrap();
    let planned = InstancePathPolicy::plan_migration(
        "one",
        source.clone(),
        destination_parent.clone(),
        paths.clone(),
        &[],
    )
    .unwrap();
    assert_eq!(
        planned.destination_install_dir,
        destination_parent.join("source")
    );
    std::fs::create_dir(destination_parent.join("source")).unwrap();
    assert!(
        InstancePathPolicy::plan_migration("one", source, destination_parent, paths, &[])
            .unwrap_err()
            .contains("already exists")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn migration_accepts_a_filesystem_root_as_the_destination_parent() {
    let root = migration_root("filesystem-root-parent");
    let source_name = format!("hanafubuki-migration-root-target-{}", std::process::id());
    let source = root.join(&source_name);
    std::fs::create_dir_all(source.join("python")).unwrap();
    std::fs::create_dir_all(source.join("core")).unwrap();
    let filesystem_root = source.ancestors().last().unwrap().to_path_buf();
    let destination = filesystem_root.join(&source_name);
    assert!(
        InstancePathPolicy::validate_layout(
            "root",
            filesystem_root.clone(),
            InstanceComponentPaths::defaults(
                &filesystem_root,
                InstanceComponentPaths::PYTHON_AND_CORE
            ),
            &[],
        )
        .unwrap_err()
        .contains("refusing to use filesystem root")
    );
    assert!(!destination.exists());

    let planned = InstancePathPolicy::plan_migration(
        "one",
        source.clone(),
        filesystem_root,
        InstanceComponentPaths::defaults(&source, InstanceComponentPaths::PYTHON_AND_CORE),
        &[],
    )
    .unwrap();
    assert_eq!(planned.destination_install_dir, destination);
    assert_eq!(
        planned.destination_component_paths.python().unwrap(),
        destination.join("python")
    );
    assert_eq!(
        planned.destination_component_paths.core(),
        destination.join("core")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn migration_rejects_other_instance_conflicts() {
    let root = migration_root("conflicts");
    let source = root.join("source");
    let destination_parent = root.join("destination");
    std::fs::create_dir(&destination_parent).unwrap();
    let destination = destination_parent.join("source");
    let other = config("other", destination.join("python").to_str().unwrap());
    assert!(
        InstancePathPolicy::plan_migration(
            "one",
            source.clone(),
            destination_parent,
            InstanceComponentPaths::defaults(&source, InstanceComponentPaths::PYTHON_AND_CORE),
            &[other]
        )
        .unwrap_err()
        .contains("conflicts with instance other")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn migration_rejects_destination_symlink_ancestor() {
    use std::os::unix::fs::symlink;

    let root = migration_root("symlink");
    let real = root.join("real");
    std::fs::create_dir(&real).unwrap();
    let linked = root.join("linked");
    symlink(&real, &linked).unwrap();
    let source = root.join("source");
    let error = InstancePathPolicy::plan_migration(
        "one",
        source.clone(),
        linked,
        InstanceComponentPaths::defaults(&source, InstanceComponentPaths::PYTHON_AND_CORE),
        &[],
    )
    .unwrap_err();
    assert!(error.contains("symbolic link"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rejects_cross_instance_root_and_component_overlap() {
    let base = crate::internal::hanafubuki_config::test_support::temp_root()
        .join(format!("hanafubuki-policy-overlap-{}", std::process::id()));
    let existing_root = base.join("one");
    let existing = vec![config("one", existing_root.to_str().unwrap())];
    let root = base.join("two");
    let mut components =
        InstanceComponentPaths::defaults(&root, InstanceComponentPaths::PYTHON_AND_CORE);
    components.set(
        InstanceComponentPaths::CORE,
        existing_root.join("core").join("child"),
    );
    let error =
        InstancePathPolicy::validate_layout("two", root, components, &existing).unwrap_err();
    assert!(error.contains("conflicts with instance one"));
}

#[test]
fn component_deletion_accepts_paths_outside_the_instance_root() {
    let root =
        crate::internal::hanafubuki_config::test_support::temp_root().join("hanafubuki-policy/one");
    let paths = InstanceComponentPaths::python_and_core(
        crate::internal::hanafubuki_config::test_support::temp_root().join("shared-python"),
        crate::internal::hanafubuki_config::test_support::temp_root().join("shared-core"),
    );
    InstancePathPolicy::validate_reinstall(&root, &paths, true, true).unwrap();
}

#[test]
fn component_deletion_rejects_the_instance_root_and_its_ancestors() {
    let root =
        crate::internal::hanafubuki_config::test_support::temp_root().join("hanafubuki-policy/one");
    let root_as_core = InstanceComponentPaths::python_and_core(root.join("python"), root.clone());
    assert!(
        InstancePathPolicy::validate_reinstall(&root, &root_as_core, false, true)
            .unwrap_err()
            .contains("equal to the instance directory")
    );
    let core_above_root = InstanceComponentPaths::python_and_core(
        root.join("python"),
        crate::internal::hanafubuki_config::test_support::temp_root().join("hanafubuki-policy"),
    );
    assert!(
        InstancePathPolicy::validate_reinstall(&root, &core_above_root, false, true)
            .unwrap_err()
            .contains("must not equal or contain the instance directory")
    );
    let relative_core =
        InstanceComponentPaths::python_and_core(root.join("python"), PathBuf::from("core"));
    assert!(
        InstancePathPolicy::validate_reinstall(&root, &relative_core, false, true)
            .unwrap_err()
            .contains("must be absolute")
    );
}

#[test]
fn deletion_plan_adds_components_outside_the_root_and_folds_in_internal_ones() {
    let root =
        crate::internal::hanafubuki_config::test_support::temp_root().join("hanafubuki-policy/one");
    let external_core = crate::internal::hanafubuki_config::test_support::temp_root()
        .join("hanafubuki-policy/shared-core");
    let paths = InstanceComponentPaths::python_and_core(root.join("python"), external_core.clone());
    assert_eq!(
        InstancePathPolicy::plan_instance_deletion(&root, Some(&paths)).unwrap(),
        vec![external_core, root.clone()]
    );
    assert_eq!(
        InstancePathPolicy::plan_instance_deletion(&root, None).unwrap(),
        vec![root]
    );
}

#[test]
fn deletion_plan_lists_one_entry_for_nested_components_outside_the_root() {
    let root =
        crate::internal::hanafubuki_config::test_support::temp_root().join("hanafubuki-policy/one");
    let external_core = crate::internal::hanafubuki_config::test_support::temp_root()
        .join("hanafubuki-policy/shared");
    let paths = InstanceComponentPaths::python_and_core(
        external_core.join("python"),
        external_core.clone(),
    );
    assert_eq!(
        InstancePathPolicy::plan_instance_deletion(&root, Some(&paths)).unwrap(),
        vec![external_core, root]
    );
}

#[test]
fn deletion_plan_rejects_a_component_containing_the_instance_root() {
    let root = crate::internal::hanafubuki_config::test_support::temp_root()
        .join("hanafubuki-policy/nested/one");
    let paths = InstanceComponentPaths::python_and_core(
        root.join("python"),
        crate::internal::hanafubuki_config::test_support::temp_root().join("hanafubuki-policy"),
    );
    assert!(
        InstancePathPolicy::plan_instance_deletion(&root, Some(&paths))
            .unwrap_err()
            .contains("contains the instance directory")
    );
}

#[test]
fn deletion_plan_rejects_filesystem_root_and_relative_paths() {
    let root =
        crate::internal::hanafubuki_config::test_support::temp_root().join("hanafubuki-policy/one");
    let drive_root = root.ancestors().last().unwrap();
    let filesystem_root =
        InstanceComponentPaths::python_and_core(root.join("python"), drive_root.to_path_buf());
    assert!(
        InstancePathPolicy::plan_instance_deletion(&root, Some(&filesystem_root))
            .unwrap_err()
            .contains("filesystem root")
    );
    let relative =
        InstanceComponentPaths::python_and_core(root.join("python"), PathBuf::from("core"));
    assert!(
        InstancePathPolicy::plan_instance_deletion(&root, Some(&relative))
            .unwrap_err()
            .contains("must be absolute")
    );
    let paths = InstanceComponentPaths::defaults(&root, InstanceComponentPaths::PYTHON_AND_CORE);
    assert!(
        InstancePathPolicy::plan_instance_deletion(drive_root, Some(&paths))
            .unwrap_err()
            .contains("filesystem root")
    );
}

#[test]
fn installation_layout_accepts_an_existing_core_outside_the_root_with_or_without_recreation() {
    let temporary = crate::internal::hanafubuki_config::test_support::tempdir();
    let base = temporary.path();
    let root = base.join("instance");
    let external_core = base.join("attached-core");
    std::fs::create_dir_all(&external_core).unwrap();
    let paths = InstanceComponentPaths::python_and_core(root.join("python"), external_core);
    InstancePathPolicy::validate_installation_layout(&root, &paths, false, false).unwrap();
    InstancePathPolicy::validate_installation_layout(&root, &paths, false, true).unwrap();
}

#[test]
fn installation_layout_accepts_core_equal_to_root_only_without_recreation() {
    let root = crate::internal::hanafubuki_config::test_support::temp_root()
        .join("hanafubuki-policy/core-root");
    let paths = InstanceComponentPaths::python_and_core(root.join("python"), root.clone());
    InstancePathPolicy::validate_installation_layout(&root, &paths, false, false).unwrap();
    assert!(
        InstancePathPolicy::validate_installation_layout(&root, &paths, false, true)
            .unwrap_err()
            .contains("cannot be deleted")
    );
}

#[cfg(windows)]
#[test]
fn accepts_canonical_windows_paths_with_verbatim_drive_prefixes() {
    let canonical =
        std::fs::canonicalize(crate::internal::hanafubuki_config::test_support::temp_root())
            .unwrap();
    assert!(
        canonical
            .components()
            .any(|component| matches!(component, std::path::Component::Prefix(_)))
    );
    validate_no_symlink_ancestors(&canonical, "temporary directory").unwrap();
}

#[test]
fn rejects_relative_instance_layout_paths() {
    let root = PathBuf::from("relative/root");
    let paths = InstanceComponentPaths::defaults(&root, InstanceComponentPaths::PYTHON_AND_CORE);
    assert!(InstancePathPolicy::validate_layout("relative", root, paths, &[]).is_err());
}

#[cfg(windows)]
#[test]
fn canonical_existing_directory_returns_a_native_windows_path() {
    let canonical = InstancePathPolicy::canonical_existing_directory(
        "temporary directory",
        &crate::internal::hanafubuki_config::test_support::temp_root(),
    )
    .unwrap();
    assert!(!canonical.to_string_lossy().starts_with(r"\\?\"));
}

#[cfg(windows)]
#[test]
fn windows_overlap_treats_verbatim_and_disk_paths_as_the_same_identity() {
    let native = Path::new(r"E:\Softwares\ComfyUI");
    let verbatim_child = Path::new(r"\\?\e:\softwares\comfyui\models");
    assert!(paths_overlap(native, verbatim_child));
    assert!(path_starts_with(verbatim_child, native));
}

#[cfg(windows)]
#[test]
fn windows_internal_relative_path_is_case_and_verbatim_insensitive() {
    assert_eq!(
        crate::internal::hanafubuki_foundation::paths::relative_below(
            Path::new(r"\\?\e:\softwares\comfyui\Python\bin"),
            Path::new(r"E:\Softwares\ComfyUI"),
        ),
        Some(PathBuf::from(r"Python\bin"))
    );
}

/// A type that declares `core` and `models` and no Python: every component
/// gets the rules Python has in the built-in layout.
fn core_and_models(core: PathBuf, models: PathBuf) -> InstanceComponentPaths {
    InstanceComponentPaths::new(core).with("models", models)
}

#[test]
fn a_declared_component_gets_the_layout_rules_of_python() {
    let root = absolute_root("declared-layout");
    let separate = core_and_models(root.join("core"), root.join("models"));
    let validated =
        InstancePathPolicy::validate_layout("declared", root.clone(), separate, &[]).unwrap();
    assert_eq!(validated.component_paths.python(), None);
    assert_eq!(
        validated.component_paths.get("models"),
        Some(root.join("models").as_path())
    );

    let root_as_core = core_and_models(root.clone(), root.join("models"));
    InstancePathPolicy::validate_layout("declared", root.clone(), root_as_core, &[]).unwrap();

    let nested = core_and_models(root.join("core"), root.join("core").join("models"));
    assert!(
        InstancePathPolicy::validate_layout("declared", root.clone(), nested, &[])
            .unwrap_err()
            .contains("models may be inside core only when core equals the instance root")
    );
    let shared = core_and_models(root.join("core"), root.join("core"));
    assert!(
        InstancePathPolicy::validate_layout("declared", root.clone(), shared, &[])
            .unwrap_err()
            .contains("instance models path must not equal or contain the core path")
    );
    let at_root = core_and_models(root.join("core"), root.clone());
    assert!(
        InstancePathPolicy::validate_layout("declared", root, at_root, &[])
            .unwrap_err()
            .contains("models path")
    );
}

#[test]
fn a_declared_component_conflicts_with_another_instance() {
    let existing_root = absolute_root("declared-existing");
    let mut existing = config("one", existing_root.to_str().unwrap());
    existing.component_paths = Some(core_and_models(
        existing_root.join("core"),
        absolute_root("declared-shared-models"),
    ));
    let root = absolute_root("declared-candidate");
    let paths = core_and_models(root.join("core"), absolute_root("declared-shared-models"));

    let error = InstancePathPolicy::validate_layout("two", root, paths, &[existing]).unwrap_err();
    assert!(error.contains("models path"), "{error}");
    assert!(error.contains("conflicts with instance one"), "{error}");
}

#[test]
fn deletion_plans_every_declared_component_outside_the_root() {
    let root = crate::internal::hanafubuki_config::test_support::temp_root()
        .join("hanafubuki-policy/declared");
    let models = crate::internal::hanafubuki_config::test_support::temp_root()
        .join("hanafubuki-policy/shared-models");
    let paths = core_and_models(root.join("core"), models.clone());

    let planned = InstancePathPolicy::plan_instance_deletion(&root, Some(&paths)).unwrap();
    assert_eq!(planned, vec![models, root]);
}

#[test]
fn migration_moves_every_declared_component_inside_the_root() {
    let root = migration_root("declared");
    let source = root.join("source");
    std::fs::create_dir_all(source.join("models")).unwrap();
    let destination_parent = root.join("destination");
    std::fs::create_dir(&destination_parent).unwrap();
    let external = root.join("shared-models");
    std::fs::create_dir(&external).unwrap();

    let internal = core_and_models(source.join("core"), source.join("models"));
    let planned = InstancePathPolicy::plan_migration(
        "declared",
        source.clone(),
        destination_parent.clone(),
        internal,
        &[],
    )
    .unwrap();
    assert_eq!(
        planned.destination_component_paths.get("models"),
        Some(destination_parent.join("source/models").as_path())
    );

    let preserved = core_and_models(source.join("core"), external.clone());
    let planned =
        InstancePathPolicy::plan_migration("declared", source, destination_parent, preserved, &[])
            .unwrap();
    assert_eq!(
        planned.destination_component_paths.get("models"),
        Some(external.as_path())
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn installation_layout_places_python_only_for_a_type_that_has_it() {
    let root = absolute_root("declared-install");
    let paths = core_and_models(root.join("core"), root.join("models"));
    InstancePathPolicy::validate_installation_layout(&root, &paths, false, false).unwrap();
    assert_eq!(
        InstancePathPolicy::validate_installation_layout(&root, &paths, true, false).unwrap_err(),
        "the instance has no `python` component"
    );
    let outside = paths.with(InstanceComponentPaths::PYTHON, absolute_root("elsewhere"));
    assert_eq!(
        InstancePathPolicy::validate_installation_layout(&root, &outside, false, false)
            .unwrap_err(),
        "installation requires Python inside the instance root"
    );
}

fn node_external(root: &Path, node: PathBuf) -> InstanceComponentPaths {
    let mut paths = InstanceComponentPaths::new(root.join("core"));
    paths.set_external("node", node);
    paths
}

#[test]
fn a_record_without_external_reads_as_owned_and_an_owned_one_writes_none() {
    let paths: InstanceComponentPaths = serde_json::from_value(json!({
        "core": { "path": "/i/core" },
        "node": { "path": "/opt/node", "external": true, "unknown": 1 },
    }))
    .unwrap();
    assert!(!paths.is_external("core"));
    assert!(paths.is_external("node"));
    assert_eq!(
        serde_json::to_value(&paths).unwrap(),
        json!({
            "core": { "path": "/i/core" },
            "node": { "path": "/opt/node", "external": true },
        })
    );
}

#[test]
fn deleting_an_instance_keeps_an_external_component_outside_its_root() {
    let temporary = crate::internal::hanafubuki_config::test_support::tempdir();
    let root = temporary.path().join("instance");
    let node = temporary.path().join("users-node");
    let models = temporary.path().join("models");
    let mut paths = node_external(&root, node.clone());
    paths.set("models", models.clone());
    for directory in [&root.join("core"), &node, &models] {
        std::fs::create_dir_all(directory).unwrap();
    }

    let planned = InstancePathPolicy::plan_instance_deletion(&root, Some(&paths)).unwrap();
    assert_eq!(
        planned,
        vec![
            normalize_path(models.clone()).unwrap(),
            normalize_path(root.clone()).unwrap()
        ]
    );
    for directory in planned {
        std::fs::remove_dir_all(directory).unwrap();
    }
    assert!(node.is_dir());
}

#[test]
fn a_root_that_is_its_external_core_is_kept_and_its_owned_components_go_alone() {
    let temporary = crate::internal::hanafubuki_config::test_support::tempdir();
    let prefix = temporary.path().join("global-prefix");
    let mut paths = InstanceComponentPaths::new(PathBuf::new());
    paths.set_external(InstanceComponentPaths::CORE, prefix.clone());
    paths.set_external("node", prefix.join("bin"));
    paths.set("cache", prefix.join("hanafubuki-cache"));
    std::fs::create_dir_all(prefix.join("bin")).unwrap();

    InstancePathPolicy::validate_component_layout(&prefix, &paths).unwrap();
    assert_eq!(
        InstancePathPolicy::plan_instance_deletion(&prefix, Some(&paths)).unwrap(),
        vec![normalize_path(prefix.join("hanafubuki-cache")).unwrap()]
    );
}

#[test]
fn an_external_component_inside_a_directory_the_instance_owns_is_refused() {
    let root = absolute_root("external-inside");
    let inside_root = node_external(&root, root.join("node"));
    assert!(
        InstancePathPolicy::validate_component_layout(&root, &inside_root)
            .unwrap_err()
            .contains("must not be inside the instance directory")
    );
    // A root-as-core layout the instance owns.
    let mut inside_core = InstanceComponentPaths::new(root.clone());
    inside_core.set_external("node", root.join("node"));
    assert!(
        InstancePathPolicy::validate_component_layout(&root, &inside_core)
            .unwrap_err()
            .contains("must not be inside the core")
    );
    // Deletion refuses it too, whatever the record says.
    assert!(
        InstancePathPolicy::plan_instance_deletion(&root, Some(&inside_root))
            .unwrap_err()
            .contains("would delete the external node folder")
    );
}

#[test]
fn two_instances_may_share_an_external_folder_but_not_an_owned_one() {
    let shared = absolute_root("shared-node-24");
    let other_root = absolute_root("other");
    let mut other = config("other", other_root.to_str().unwrap());
    other.component_paths = Some(node_external(&other_root, shared.clone()));
    let root = absolute_root("one");

    InstancePathPolicy::validate_layout(
        "one",
        root.clone(),
        node_external(&root, shared.clone()),
        std::slice::from_ref(&other),
    )
    .unwrap();
    let mut owned = InstanceComponentPaths::new(root.join("core"));
    owned.set("node", shared);
    assert!(
        InstancePathPolicy::validate_layout("one", root, owned, &[other])
            .unwrap_err()
            .contains("conflicts with instance other")
    );
}

#[test]
fn a_reinstall_never_recreates_an_external_component() {
    let temporary = crate::internal::hanafubuki_config::test_support::tempdir();
    let root = temporary.path().join("instance");
    let mut paths = InstanceComponentPaths::new(PathBuf::new());
    paths.set_external(
        InstanceComponentPaths::CORE,
        temporary.path().join("users-core"),
    );
    assert!(
        InstancePathPolicy::validate_reinstall(&root, &paths, false, true)
            .unwrap_err()
            .contains("a reinstall never deletes it")
    );
    InstancePathPolicy::validate_reinstall(&root, &paths, false, false).unwrap();
}

#[test]
fn a_migration_does_not_move_an_instance_whose_root_is_external() {
    let base = migration_root("external-root");
    let source = base.join("source");
    let mut paths = InstanceComponentPaths::new(PathBuf::new());
    paths.set_external(InstanceComponentPaths::CORE, source.clone());
    std::fs::create_dir_all(base.join("destination")).unwrap();

    let error =
        InstancePathPolicy::plan_migration("prefix", source, base.join("destination"), paths, &[])
            .unwrap_err();
    assert!(error.contains("which a migration must not move"), "{error}");
    let _ = std::fs::remove_dir_all(base);
}
