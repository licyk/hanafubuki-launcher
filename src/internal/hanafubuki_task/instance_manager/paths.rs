use std::path::PathBuf;

use super::InstancePaths;

pub fn build_instance_paths(instance_path: PathBuf) -> Result<InstancePaths, String> {
    if instance_path.as_os_str().is_empty() {
        return Err("instance_path must not be empty".to_string());
    }
    let paths = InstancePaths {
        data_dir: instance_path.join("data"),
        instance_path,
    };
    for dir in [&paths.instance_path, &paths.data_dir] {
        std::fs::create_dir_all(dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    }
    Ok(paths)
}
