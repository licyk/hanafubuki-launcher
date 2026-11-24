//! Pure policy for instance roots, component paths, and destructive scope.

use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_foundation::paths::{
    PathKey, ResolvedPath, normalize_path, validate_not_dangerous,
};

use crate::internal::hanafubuki_config::repository::instances::{
    InstanceComponentPaths, InstanceConfig,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedInstancePaths {
    pub install_dir: PathBuf,
    pub component_paths: InstanceComponentPaths,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedInstanceMigrationPaths {
    pub source_install_dir: PathBuf,
    pub destination_install_dir: PathBuf,
    pub source_component_paths: InstanceComponentPaths,
    pub destination_component_paths: InstanceComponentPaths,
}

pub struct InstancePathPolicy;

impl InstancePathPolicy {
    /// How a component is named to the user: the two built-in components by
    /// their product names, any other by its own.
    pub fn component_label(name: &str) -> String {
        match name {
            InstanceComponentPaths::PYTHON => "Python".to_string(),
            InstanceComponentPaths::CORE => "Core".to_string(),
            other => other.to_string(),
        }
    }

    pub fn paths_overlap(left: &Path, right: &Path) -> bool {
        paths_overlap(left, right)
    }

    pub fn component_is_in_instance_root(instance_root: &Path, component: &Path) -> bool {
        path_starts_with(component, instance_root) && !paths_equal(component, instance_root)
    }

    pub fn paths_refer_to_same_location(left: &Path, right: &Path) -> bool {
        paths_equal(left, right)
    }

    pub fn revalidate_migration_source(
        expected_source_install_dir: &Path,
        source_component_paths: &InstanceComponentPaths,
    ) -> Result<(), String> {
        let canonical = Self::canonical_existing_directory(
            "instance migration source",
            expected_source_install_dir,
        )?;
        if !paths_equal(&canonical, expected_source_install_dir) {
            return Err(format!(
                "instance migration source identity path changed (expected {}, current {})",
                expected_source_install_dir.display(),
                canonical.display()
            ));
        }
        Self::validate_component_layout(expected_source_install_dir, source_component_paths)?;
        Ok(())
    }

    pub fn plan_migration(
        instance_id: &str,
        source_install_dir: PathBuf,
        destination_parent_dir: PathBuf,
        source_component_paths: InstanceComponentPaths,
        existing: &[InstanceConfig],
    ) -> Result<ValidatedInstanceMigrationPaths, String> {
        let source_install_dir =
            Self::canonical_existing_directory("instance migration source", &source_install_dir)?;
        let destination_parent_dir = Self::canonical_existing_migration_parent(
            "instance migration destination parent",
            &destination_parent_dir,
        )?;
        let source_name = source_install_dir.file_name().ok_or_else(|| {
            format!(
                "instance migration source has no directory name: {}",
                source_install_dir.display()
            )
        })?;
        if let Some((name, path)) = source_component_paths
            .external()
            .find(|(_, path)| path_starts_with(path, &source_install_dir))
        {
            return Err(format!(
                "instance directory {} holds the external {} folder {}, which a migration must not move",
                source_install_dir.display(),
                Self::component_label(name),
                path.display()
            ));
        }
        let destination_install_dir = destination_parent_dir.join(source_name);
        validate_no_symlink_ancestors(&destination_install_dir, "instance migration destination")?;
        if paths_overlap(&source_install_dir, &destination_install_dir) {
            return Err(format!(
                "instance migration source and destination must not overlap: {} and {}",
                source_install_dir.display(),
                destination_install_dir.display()
            ));
        }
        if destination_install_dir.exists() {
            return Err(format!(
                "instance migration destination already exists: {}",
                destination_install_dir.display()
            ));
        }
        let destination_component_paths = source_component_paths.try_map(|name, source| {
            let Some(relative) = crate::internal::hanafubuki_foundation::paths::relative_below(
                source,
                &source_install_dir,
            ) else {
                return Ok(source.to_path_buf());
            };
            if !relative.as_os_str().is_empty() {
                return Ok(destination_install_dir.join(relative));
            }
            if name == InstanceComponentPaths::CORE {
                return Ok(destination_install_dir.clone());
            }
            Err(format!(
                "{} path must not equal instance root",
                Self::component_label(name)
            ))
        })?;

        let validated = Self::validate_layout(
            instance_id,
            destination_install_dir.clone(),
            destination_component_paths,
            existing,
        )?;
        Ok(ValidatedInstanceMigrationPaths {
            source_install_dir,
            destination_install_dir: validated.install_dir,
            source_component_paths,
            destination_component_paths: validated.component_paths,
        })
    }

    /// No component may equal another, and only Core may contain another.
    pub fn validate_component_separation(
        component_paths: &InstanceComponentPaths,
    ) -> Result<(), String> {
        for (name, path) in non_core_components(component_paths) {
            for (other_name, other_path) in component_paths.iter() {
                if other_name != name
                    && (paths_equal(path, other_path) || path_starts_with(other_path, path))
                {
                    return Err(format!(
                        "instance {} path must not equal or contain the {} path",
                        Self::component_label(name),
                        Self::component_label(other_name).to_lowercase()
                    ));
                }
            }
        }
        Ok(())
    }

    /// Validate the two supported component layouts relative to an instance root:
    /// separate components, or an instance whose root is Core and whose other
    /// components are nested below it.
    pub fn validate_component_layout(
        install_dir: &Path,
        component_paths: &InstanceComponentPaths,
    ) -> Result<(), String> {
        Self::validate_component_separation(component_paths)?;
        let core = component_paths.core();
        for (name, path) in non_core_components(component_paths) {
            if path_starts_with(path, core) && !paths_equal(core, install_dir) {
                return Err(format!(
                    "instance {} may be inside core only when core equals the instance root",
                    Self::component_label(name)
                ));
            }
            validate_component_relationship(
                install_dir,
                path,
                &Self::component_label(name),
                false,
            )?;
        }
        validate_component_relationship(install_dir, core, "Core", true)?;
        validate_external_placement(install_dir, component_paths)
    }

    /// Validate one complete instance layout against immutable config snapshots.
    pub fn validate_layout(
        instance_id: &str,
        install_dir: PathBuf,
        component_paths: InstanceComponentPaths,
        existing: &[InstanceConfig],
    ) -> Result<ValidatedInstancePaths, String> {
        let install_dir = normalize_safe(install_dir)?;
        let component_paths =
            component_paths.try_map(|_, path| normalize_safe(path.to_path_buf()))?;

        Self::validate_component_layout(&install_dir, &component_paths)?;
        validate_no_symlink_ancestors(&install_dir, "instance root")?;
        for (name, path) in component_paths.iter() {
            if Self::component_is_in_instance_root(&install_dir, path) {
                validate_no_symlink_ancestors(path, &Self::component_label(name))?;
            }
        }

        let candidate = labelled_locations(Some(&install_dir), Some(&component_paths));
        for other in existing.iter().filter(|other| other.id != instance_id) {
            let other_paths =
                labelled_locations(other.install_dir.as_deref(), other.component_paths.as_ref());
            for (candidate_label, candidate_path, candidate_external) in &candidate {
                // Nothing deletes or replaces an external folder, so two
                // instances may share one (a Node.js of the user's).
                if let Some((other_label, other_path, _)) =
                    other_paths.iter().find(|(_, other_path, other_external)| {
                        !(*candidate_external && *other_external)
                            && paths_overlap(candidate_path, other_path)
                    })
                {
                    return Err(format!(
                        "instance {instance_id} {candidate_label} path {} conflicts with instance {} {other_label} path {}",
                        candidate_path.display(),
                        other.id,
                        other_path.display()
                    ));
                }
            }
        }
        Ok(ValidatedInstancePaths {
            install_dir,
            component_paths,
        })
    }

    pub fn validate_reinstall(
        install_dir: &Path,
        component_paths: &InstanceComponentPaths,
        recreate_python: bool,
        recreate_core: bool,
    ) -> Result<(), String> {
        let recreated = [
            (recreate_python, InstanceComponentPaths::PYTHON),
            (recreate_core, InstanceComponentPaths::CORE),
        ];
        for (_, name) in recreated.into_iter().filter(|(recreate, _)| *recreate) {
            let label = Self::component_label(name);
            let path = component_paths.require(name)?;
            if component_paths.is_external(name) {
                return Err(format!(
                    "{label} is an external folder ({}); a reinstall never deletes it",
                    path.display()
                ));
            }
            Self::validate_component_deletion(install_dir, path, &label)?;
        }
        Ok(())
    }

    /// Validate that the current component layout can be consumed by the
    /// installer selected by the durable instance record.
    ///
    /// This is deliberately separate from `validate_component_layout`: an
    /// external Core must still resolve to the exact existing directory before
    /// the installer may update it. Callers use this pure check before acquiring
    /// lifecycle maintenance and repeat it at install reservation.
    pub fn validate_installation_layout(
        install_dir: &Path,
        component_paths: &InstanceComponentPaths,
        recreate_python: bool,
        recreate_core: bool,
    ) -> Result<(), String> {
        Self::validate_reinstall(install_dir, component_paths, recreate_python, recreate_core)?;
        // A type without Python has no `python` component to place.
        if let Some(python) = component_paths.python() {
            if !Self::component_is_in_instance_root(install_dir, python) {
                return Err("installation requires Python inside the instance root".to_string());
            }
        }

        let core = component_paths.core();
        let core_is_managed = Self::component_is_in_instance_root(install_dir, core);
        let core_is_instance_root = Self::paths_refer_to_same_location(core, install_dir);
        if !core_is_managed && !core_is_instance_root {
            let current = Self::canonical_existing_directory("external WebUI Core", core)?;
            if !Self::paths_refer_to_same_location(&current, core) {
                return Err("external WebUI Core identity changed before installation".to_string());
            }
        }
        Ok(())
    }

    /// Every directory removed when an instance is deleted, in removal order.
    ///
    /// Deleting an instance deletes the directories it owns wherever they are
    /// configured, so a component outside the instance root is listed here and
    /// removed on its own. Components under the root are left out because the
    /// root removal already takes them, and a component that contains the root
    /// is refused: deletion never climbs above the instance it was asked for.
    /// An external component is never removed, nor anything that holds one:
    /// a root that is its external core stays, and the owned components
    /// inside it are listed on their own.
    pub fn plan_instance_deletion(
        install_dir: &Path,
        component_paths: Option<&InstanceComponentPaths>,
    ) -> Result<Vec<PathBuf>, String> {
        let install_dir = normalize_safe(install_dir.to_path_buf())?;
        let mut external = Vec::new();
        let mut directories: Vec<PathBuf> = Vec::new();
        if let Some(component_paths) = component_paths {
            for (name, component) in component_paths.external() {
                external.push((
                    Self::component_label(name),
                    normalize_safe(component.to_path_buf())?,
                ));
            }
        }
        let root_kept = external
            .iter()
            .any(|(_, component)| paths_equal(component, &install_dir));
        if let Some(component_paths) = component_paths {
            for (name, component) in component_paths.iter() {
                if component_paths.is_external(name) {
                    continue;
                }
                let label = Self::component_label(name);
                let label = label.as_str();
                let component = normalize_safe(component.to_path_buf())?;
                if !root_kept && path_starts_with(&component, &install_dir) {
                    continue;
                }
                if path_starts_with(&install_dir, &component) && !root_kept {
                    return Err(format!(
                        "{label} path {} contains the instance directory {} and cannot be deleted",
                        component.display(),
                        install_dir.display()
                    ));
                }
                validate_no_symlink_ancestors(&component, label)?;
                // Two components may nest in each other, so only the outer
                // one is listed: the inner path is gone once it runs.
                if directories
                    .iter()
                    .any(|planned| path_starts_with(&component, planned))
                {
                    continue;
                }
                directories.retain(|planned| !path_starts_with(planned, &component));
                directories.push(component);
            }
        }
        if !root_kept {
            validate_no_symlink_ancestors(&install_dir, "instance root")?;
            directories.push(install_dir);
        }
        for directory in &directories {
            if let Some((label, component)) = external
                .iter()
                .find(|(_, component)| path_starts_with(component, directory))
            {
                return Err(format!(
                    "deleting {} would delete the external {label} folder {} inside it",
                    directory.display(),
                    component.display()
                ));
            }
        }
        Ok(directories)
    }

    pub fn canonical_existing_directory(label: &str, path: &Path) -> Result<PathBuf, String> {
        Self::canonical_existing_directory_with(label, path, normalize_safe)
    }

    pub fn canonical_existing_migration_parent(
        label: &str,
        path: &Path,
    ) -> Result<PathBuf, String> {
        Self::canonical_existing_directory_with(label, path, normalize_absolute)
    }

    fn canonical_existing_directory_with(
        label: &str,
        path: &Path,
        normalize: fn(PathBuf) -> Result<PathBuf, String>,
    ) -> Result<PathBuf, String> {
        let input = normalize(path.to_path_buf())?;
        validate_no_symlink_ancestors(&input, label)?;
        let resolved = ResolvedPath::resolve(&input)
            .map_err(|error| format!("resolve existing {label} directory: {error}"))?;
        // Records keep the native form; only the resolver holds the system form.
        let canonical = normalize(resolved.native().to_path_buf())?;
        validate_no_symlink_ancestors(&canonical, label)?;
        if !resolved.system().is_dir() {
            return Err(format!(
                "existing {label} path is not a directory: {}",
                resolved.native_text()
            ));
        }
        Ok(canonical)
    }

    /// Authorize deleting one component directory on its own, before the
    /// installer recreates it.
    ///
    /// A component outside the instance root is deleted like any other, so the
    /// only refusals left are the paths that would take the instance root with
    /// them: the root itself, one of its ancestors, or a symlinked route to
    /// either. A root-as-core layout is therefore rebuilt by deleting the
    /// instance, not by recreating its core underneath itself.
    fn validate_component_deletion(
        install_dir: &Path,
        component: &Path,
        label: &str,
    ) -> Result<(), String> {
        // Normalized first: a relative or traversing path compares as neither
        // inside nor outside the instance, so the containment rules below only
        // mean something once both sides are absolute.
        let install_dir = normalize_safe(install_dir.to_path_buf())?;
        let component = normalize_safe(component.to_path_buf())?;
        if paths_equal(&component, &install_dir) {
            return Err(format!(
                "{label} path equal to the instance directory cannot be deleted on its own"
            ));
        }
        validate_component_relationship(&install_dir, &component, label, false)?;
        validate_no_symlink_ancestors(&install_dir, "instance root")?;
        validate_no_symlink_ancestors(&component, label)
    }
}

fn non_core_components(
    component_paths: &InstanceComponentPaths,
) -> impl Iterator<Item = (&str, &Path)> {
    component_paths
        .iter()
        .filter(|(name, _)| *name != InstanceComponentPaths::CORE)
}

fn normalize_safe(path: PathBuf) -> Result<PathBuf, String> {
    let path = normalize_absolute(path)?;
    validate_not_dangerous(&path)?;
    Ok(path)
}

fn normalize_absolute(path: PathBuf) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!(
            "instance path must be absolute: {}",
            path.display()
        ));
    }
    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    }) {
        return Err(format!(
            "instance path must not contain current or parent traversal: {}",
            path.display()
        ));
    }
    normalize_path(path)
}

fn validate_component_relationship(
    install_dir: &Path,
    component: &Path,
    label: &str,
    allow_equal: bool,
) -> Result<(), String> {
    if (!allow_equal && paths_equal(component, install_dir))
        || (path_starts_with(install_dir, component) && !paths_equal(component, install_dir))
    {
        return Err(format!(
            "{label} path {} must not equal or contain the instance directory {}",
            component.display(),
            install_dir.display()
        ));
    }
    Ok(())
}

fn validate_no_symlink_ancestors(path: &Path, label: &str) -> Result<(), String> {
    let mut cursor = PathBuf::new();
    for component in path.components() {
        cursor.push(component.as_os_str());
        // A Windows prefix such as `\\?\E:` or `\\?\UNC\server\share` is not
        // independently inspectable. Wait for the following root component so
        // the first filesystem query receives a complete rooted path.
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        match std::fs::symlink_metadata(&cursor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "{label} path must not traverse symbolic link {}",
                    cursor.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(format!("inspect {}: {error}", cursor.display())),
        }
    }
    Ok(())
}

/// An external component never lies inside a directory the instance owns,
/// since deleting that directory would take it: neither inside an owned
/// component, nor inside the root unless the root is itself external (an
/// attached installation whose core is its root).
fn validate_external_placement(
    install_dir: &Path,
    component_paths: &InstanceComponentPaths,
) -> Result<(), String> {
    let root_is_external = component_paths
        .external()
        .any(|(_, path)| paths_equal(path, install_dir));
    for (name, path) in component_paths.external() {
        let owner = component_paths
            .iter()
            .filter(|(other, _)| !component_paths.is_external(other))
            .find(|(_, owned)| path_starts_with(path, owned))
            .map(|(other, _)| InstancePathPolicy::component_label(other).to_lowercase());
        let owner = owner.or_else(|| {
            (!root_is_external && path_starts_with(path, install_dir))
                .then(|| "instance directory".to_string())
        });
        if let Some(owner) = owner {
            return Err(format!(
                "external {} folder {} must not be inside the {owner}, which the instance owns",
                InstancePathPolicy::component_label(name),
                path.display()
            ));
        }
    }
    Ok(())
}

/// The root and every component with whether it is external, for the
/// conflict check between instances.
fn labelled_locations<'a>(
    install_dir: Option<&'a Path>,
    component_paths: Option<&'a InstanceComponentPaths>,
) -> Vec<(String, &'a Path, bool)> {
    let mut locations: Vec<(String, &Path, bool)> = install_dir
        .map(|root| ("root".to_string(), root, false))
        .into_iter()
        .collect();
    if let Some(components) = component_paths {
        locations.extend(components.iter().map(|(name, path)| {
            (
                InstancePathPolicy::component_label(name),
                path,
                components.is_external(name),
            )
        }));
    }
    locations
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    paths_equal(left, right) || path_starts_with(left, right) || path_starts_with(right, left)
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    PathKey::of(left) == PathKey::of(right)
}

fn path_starts_with(path: &Path, base: &Path) -> bool {
    PathKey::of(path).starts_with(&PathKey::of(base))
}

#[cfg(test)]
mod tests;
