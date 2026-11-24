use std::path::PathBuf;
use std::sync::Arc;

use crate::internal::hanafubuki_config::repository::instances::InstanceComponentPaths;
use crate::internal::hanafubuki_foundation::form_schema::SchemaGroup;
use crate::internal::hanafubuki_foundation::module_faults::ContributionOwner;
use crate::internal::hanafubuki_instance_contracts::{
    InstanceTypeDescriptor, InstanceTypeId, ServerRole, server_start_order,
};
use parking_lot::RwLock;
use serde_json::{Map, Value};

use crate::internal::hanafubuki_webui::discovery::DiscoverySignature;
use crate::internal::hanafubuki_webui::install::steps::{
    ProcessEnvironment, StepOperation, StepPlan,
};
use crate::internal::hanafubuki_webui::instances::InstanceSpec;

use super::{BuiltinProvider, UpdateCheck};

/// Where one instance is: what a type's placeholders resolve against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceLocation {
    pub instance_id: String,
    pub instance_root: PathBuf,
    pub components: InstanceComponentPaths,
}

/// A declared server's command for one instance, its placeholders resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerCommand {
    pub argv: Vec<String>,
    /// Its variables (the type's, under the role's declared ones, under a
    /// backend plan's), its `PATH` entries and its launcher code.
    pub env: ProcessEnvironment,
    /// The instance's `core` component.
    pub cwd: PathBuf,
}

/// One instance type: what it is, the checks its specs pass, and, for a
/// type the application runs from its declaration, its steps and server
/// commands for one instance. The application runs them itself: a provider
/// never starts a process or touches a path.
pub trait InstanceProvider: Send + Sync {
    fn descriptor(&self) -> &InstanceTypeDescriptor;

    /// A spec from a request (the frontend's or the Agent's): a field the
    /// type does not declare is refused.
    fn validate_spec(&self, spec: Map<String, Value>) -> Result<InstanceSpec, String>;

    /// A request's spec for an existing instance (a reinstall with new
    /// values): a type its backend provides lets the backend see where the
    /// instance's components are.
    fn validate_instance_spec(
        &self,
        spec: Map<String, Value>,
        _location: &InstanceLocation,
    ) -> Result<InstanceSpec, String> {
        self.validate_spec(spec)
    }

    /// A persisted spec: a field the type no longer declares is dropped
    /// first, so a record outlives a type that stopped taking an option.
    fn read_spec(&self, spec: Map<String, Value>) -> Result<InstanceSpec, String>;

    /// The type's install form: the fields its spec takes, by group.
    fn install_form(&self) -> Vec<SchemaGroup>;

    /// The steps an install operation runs; a built-in type installs
    /// through its family's installer instead.
    fn step_plan(
        &self,
        _operation: StepOperation,
        _spec: &InstanceSpec,
        _location: &InstanceLocation,
    ) -> Result<StepPlan, String> {
        Err(format!(
            "instance type `{}` declares no install steps",
            self.descriptor().id
        ))
    }

    /// Whether a ready instance of this type can be updated.
    fn offers_update(&self) -> bool {
        false
    }

    /// Whether the type's `update` would change the instance, as a type
    /// whose descriptor `checks_updates` through its extension's backend
    /// answers it; a built-in type checks through its versions instead.
    /// Read-only: the instance may be running.
    fn check_update(
        &self,
        _spec: &InstanceSpec,
        _location: &InstanceLocation,
    ) -> Result<UpdateCheck, String> {
        Err(no_update_check(&self.descriptor().id))
    }

    /// Whether [`Self::check_update`] can be asked now: a type its
    /// extension's backend provides waits for that backend to run.
    fn update_check_ready(&self) -> bool {
        true
    }

    /// The port a role listens on when the type chooses it; `None`
    /// allocates one for each start.
    fn server_port(&self, _role: &ServerRole, _spec: &InstanceSpec) -> Result<Option<u16>, String> {
        Ok(None)
    }

    /// The command a declared role runs, listening on `port`; `abandon`
    /// ends a wait it makes (for `{host.python}`) once a stop superseded
    /// the start.
    fn server_command(
        &self,
        role: &ServerRole,
        _spec: &InstanceSpec,
        _location: &InstanceLocation,
        _port: u16,
        _abandon: &dyn Fn() -> bool,
    ) -> Result<ServerCommand, String> {
        Err(format!(
            "instance type `{}` declares no command for server role `{role}`",
            self.descriptor().id
        ))
    }

    /// What the type adds to its terminal's environment: a declarative
    /// type's `[environment]`.
    fn terminal_environment(
        &self,
        _spec: &InstanceSpec,
        _location: &InstanceLocation,
    ) -> Result<ProcessEnvironment, String> {
        Ok(ProcessEnvironment::default())
    }

    /// How the scanner recognizes an existing installation of the type,
    /// tried in order; the built-in families' layouts are the scanner's own.
    fn discovery_signatures(&self) -> &[DiscoverySignature] {
        &[]
    }
}

/// The instance types this application can install, run and manage, by id:
/// the built-in ones, fixed once the application is composed and the
/// core's, and those an extension registers while it is active, its own.
#[derive(Default)]
pub struct InstanceTypeRegistry {
    providers: RwLock<Vec<(ContributionOwner, Arc<dyn InstanceProvider>)>>,
}

impl InstanceTypeRegistry {
    /// Every built-in type, in the order the creation wizard offers them.
    pub fn with_builtin_types() -> Self {
        let registry = Self::default();
        // The built-in ids are distinct by construction (one per family).
        let _ = registry.register_all(
            ContributionOwner::Core,
            BuiltinProvider::all().map(|provider| {
                let provider: Arc<dyn InstanceProvider> = Arc::new(provider);
                provider
            }),
        );
        registry
    }

    /// Adds a type `owner` provides whose id is new and whose server roles
    /// are a valid start order.
    pub fn register(
        &self,
        owner: ContributionOwner,
        provider: Arc<dyn InstanceProvider>,
    ) -> Result<(), String> {
        self.register_all(owner, [provider])
    }

    /// Adds every one of `providers`, all `owner`'s, or none when one of
    /// them cannot be added.
    pub fn register_all(
        &self,
        owner: ContributionOwner,
        providers: impl IntoIterator<Item = Arc<dyn InstanceProvider>>,
    ) -> Result<(), String> {
        let providers: Vec<_> = providers.into_iter().collect();
        let mut registered = self.providers.write();
        for (index, provider) in providers.iter().enumerate() {
            let descriptor = provider.descriptor();
            let id = &descriptor.id;
            let mut taken = registered
                .iter()
                .map(|(_, known)| known)
                .chain(&providers[..index]);
            if taken.any(|known| &known.descriptor().id == id) {
                return Err(format!("instance type `{id}` is already registered"));
            }
            server_start_order(&descriptor.servers)
                .map_err(|error| format!("instance type `{id}`: {error}"))?;
        }
        registered.extend(
            providers
                .into_iter()
                .map(|provider| (owner.clone(), provider)),
        );
        Ok(())
    }

    /// Removes the types `ids` names that an extension registered; a
    /// built-in type, the core's, stays. Instances of a removed type are
    /// listed as unavailable from then on.
    pub fn unregister(&self, ids: &[InstanceTypeId]) {
        self.providers.write().retain(|(owner, provider)| {
            *owner == ContributionOwner::Core || !ids.contains(&provider.descriptor().id)
        });
    }

    /// The ids of the types `owner` provides, in registration order.
    pub fn owned_by(&self, owner: &ContributionOwner) -> Vec<InstanceTypeId> {
        self.providers
            .read()
            .iter()
            .filter(|(provider_owner, _)| provider_owner == owner)
            .map(|(_, provider)| provider.descriptor().id.clone())
            .collect()
    }

    pub fn get(&self, id: &InstanceTypeId) -> Option<Arc<dyn InstanceProvider>> {
        self.providers
            .read()
            .iter()
            .map(|(_, provider)| provider)
            .find(|provider| &provider.descriptor().id == id)
            .cloned()
    }

    /// The type's provider, or why an instance of it can do nothing.
    pub fn require(&self, id: &InstanceTypeId) -> Result<Arc<dyn InstanceProvider>, String> {
        self.get(id).ok_or_else(|| unavailable_reason(id))
    }

    /// Every registered type's discovery signatures, in registration order:
    /// a scan takes them once, so a type that leaves meanwhile is still
    /// recognized until the scan ends.
    pub fn discovery_signatures(&self) -> Vec<DiscoverySignature> {
        self.providers
            .read()
            .iter()
            .flat_map(|(_, provider)| provider.discovery_signatures().iter().cloned())
            .collect()
    }

    pub fn descriptors(&self) -> Vec<InstanceTypeDescriptor> {
        self.providers
            .read()
            .iter()
            .map(|(_, provider)| provider.descriptor().clone())
            .collect()
    }
}

/// Why an instance of a type no provider is registered for is listed but
/// refuses every operation except deleting its record.
pub(crate) fn unavailable_reason(id: &InstanceTypeId) -> String {
    format!("type `{id}` is not available")
}

/// Why an instance of type `id` is not checked for updates.
pub(in crate::internal::hanafubuki_webui::instance_types) fn no_update_check(
    id: &InstanceTypeId,
) -> String {
    format!("instance type `{id}` has no update check")
}
