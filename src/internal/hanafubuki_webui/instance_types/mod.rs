//! The instance type registry and its providers.
//!
//! An instance's identity is its type id. Code outside a provider reaches a
//! type through the registry by that id; the built-in provider wraps the
//! per-family code, and the exhaustive dispatch over the built-in families
//! stays inside it (`docs/rules/instance-types.md`). A declarative provider
//! interprets a type descriptor: its install steps, its servers' commands
//! and the parameters both take.

mod builtin;
mod declarative;
pub mod presets;
mod registry;

pub use builtin::{BUILTIN_OPERATIONS, BuiltinProvider};
#[cfg(feature = "schema")]
pub use declarative::descriptor_schema;
pub use declarative::{
    BackendCommand, DeclarativeProvider, DeclarativeSpec, DescriptorIssue, DescriptorIssues,
    ExtensionContext, HostPython, HostPythonPaths, TypeBackend, UpdateCheck, is_standard_operation,
};
pub(crate) use registry::unavailable_reason;
pub use registry::{InstanceLocation, InstanceProvider, InstanceTypeRegistry, ServerCommand};

#[cfg(test)]
pub(crate) use declarative::tests::fixtures as declarative_fixtures;

#[cfg(test)]
mod tests;
