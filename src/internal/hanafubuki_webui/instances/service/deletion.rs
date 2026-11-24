use crate::internal::hanafubuki_config::policy::instance_path::InstancePathPolicy;

use super::super::{DeleteInstanceRequest, WebUiInstanceSnapshot, delete_configured_directory};
use super::WebUiInstanceService;

impl WebUiInstanceService {
    pub fn validate_delete_request(
        &self,
        request: &DeleteInstanceRequest,
    ) -> Result<WebUiInstanceSnapshot, String> {
        let snapshot = self.get(&request.instance_id)?;
        if snapshot.install.instance_identity != request.expected_instance_identity {
            return Err("instance identity changed; reload maintenance details".to_string());
        }
        if snapshot.instance.config_revision != request.expected_config_revision {
            return Err("instance configuration changed; reload maintenance details".to_string());
        }
        Ok(snapshot)
    }

    /// Delete every directory the instance owns: its root plus each component
    /// configured outside that root.
    ///
    /// The whole plan is built before the first removal, so an unusable
    /// component path stops the deletion while the instance is still intact.
    /// A directory that is already gone counts as deleted, which is what lets
    /// a partially completed deletion be retried from its journal entry.
    pub fn delete_instance_files(&self, snapshot: &WebUiInstanceSnapshot) -> Result<(), String> {
        // An instance whose type is unavailable keeps its files: only its
        // record can be deleted.
        snapshot.install.require_available()?;
        let directories = InstancePathPolicy::plan_instance_deletion(
            &snapshot.install_dir,
            snapshot.instance.component_paths.as_ref(),
        )?;
        for directory in directories {
            delete_configured_directory(&directory)?;
        }
        Ok(())
    }
}
