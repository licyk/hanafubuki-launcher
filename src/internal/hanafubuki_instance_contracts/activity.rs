//! What one instance module asks another about an instance's work. A module
//! reaches a sibling through these, never through the sibling's service
//! type, so the sibling's crate stays out of its own.

/// The `instance-launch` module's launches of an instance.
pub trait InstanceLaunchActivity: Send + Sync {
    /// A launch of the instance that has not ended.
    fn instance_has_active_launch(&self, instance_id: &str) -> bool;

    /// A launch of the instance in a phase that uses its API server.
    fn instance_has_api_work(&self, instance_id: &str) -> bool;

    /// Whether the instance's running launch started with arguments other
    /// than `current_arguments`, so the change needs a restart.
    fn restart_required_for(
        &self,
        instance_id: &str,
        instance_identity: &str,
        current_arguments: &[String],
    ) -> bool;
}

/// The `instance-management` module's work on an instance.
pub trait InstanceManagementActivity: Send + Sync {
    /// A management operation of the instance that is starting or has not
    /// ended.
    fn instance_has_active_work(&self, instance_id: &str) -> bool;
}
