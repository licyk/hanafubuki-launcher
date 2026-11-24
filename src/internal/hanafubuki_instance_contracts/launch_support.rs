/// Launch ownership Management granted for one instance identity.
///
/// Returned by [`InstanceLaunchSupport::reserve_instance_launch`] and handed
/// back to [`InstanceLaunchSupport::release_instance_launch`], so a release
/// names the exact reservation it ends rather than an identity string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceLaunchReservation {
    instance_id: String,
    instance_identity: String,
}

impl InstanceLaunchReservation {
    pub fn new(instance_id: impl Into<String>, instance_identity: impl Into<String>) -> Self {
        Self {
            instance_id: instance_id.into(),
            instance_identity: instance_identity.into(),
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn instance_identity(&self) -> &str {
        &self.instance_identity
    }
}

/// The desktop Hotpatch configuration document Management normalized for a
/// launch, serialized once. Launch passes it to the runtime unread, so it is
/// opaque here: no domain can inspect or edit it across the boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopHotpatchConfig(String);

impl DesktopHotpatchConfig {
    /// Wraps a document Management has already validated and serialized.
    pub fn from_validated_json(json: String) -> Self {
        Self(json)
    }

    pub fn as_json(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct DesktopHotpatchLaunchConfig {
    pub instance_id: String,
    pub instance_identity: String,
    pub api_generation: u64,
    pub config: DesktopHotpatchConfig,
}

/// Launch-facing view of Management ownership and Hotpatch configuration.
pub trait InstanceLaunchSupport: Send + Sync {
    fn assert_instance_available(&self, instance_id: &str) -> Result<(), String>;
    fn reserve_instance_launch(
        &self,
        instance_id: &str,
    ) -> Result<InstanceLaunchReservation, String>;
    fn release_instance_launch(&self, reservation: &InstanceLaunchReservation);
    fn desktop_hotpatch_config_for_launch(
        &self,
        instance_id: &str,
    ) -> Result<DesktopHotpatchLaunchConfig, String>;
    fn validate_desktop_hotpatch_config_for_launch(
        &self,
        binding: &DesktopHotpatchLaunchConfig,
    ) -> Result<(), String>;
}
