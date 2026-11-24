//! The checks of a type's `[management]`: the operations it offers, and the
//! built-in type whose management it adopts for the sections the control
//! server serves.

use crate::internal::hanafubuki_instance_contracts::{
    InstanceRuntime, InstanceTypeId, WebUiFamily,
};

use super::super::document::ManagementDocument;
use super::Issues;
use crate::internal::hanafubuki_webui::instance_types::builtin::{
    BUILTIN_OPERATIONS, BuiltinProvider,
};

/// The standard operations every declarative type may declare: those it
/// offers without a control server.
const OPERATIONS: [&str; 2] = ["update", "terminal"];

/// The standard operations a type offers by adopting a built-in type's
/// management: the sections the control server serves for it.
const ADOPTED_OPERATIONS: [&str; 4] = ["versions", "extensions", "models", "environment"];

/// The standard operation a type its backend provides declares to take part
/// in the application's update check (`provider.checkUpdate`).
const UPDATE_CHECK: &str = "update-check";

/// What of the type its management is checked against.
pub(super) struct Type<'a> {
    /// Its backend provides it (`provider = "backend"`).
    pub backend: bool,
    pub runtime: &'a InstanceRuntime,
    /// `core` declares `external = "allowed"`.
    pub core_may_be_external: bool,
    /// It declares the `control` role.
    pub control: bool,
}

/// A type's management, checked.
pub(super) struct Declared {
    pub offers_update: bool,
    pub checks_updates: bool,
    pub adopts: Option<InstanceTypeId>,
}

pub(super) fn declared(
    issues: &mut Issues,
    document: &ManagementDocument,
    declaring: Type<'_>,
) -> Declared {
    let adopts = document
        .adopts
        .as_deref()
        .and_then(|adopts| adopted(issues, adopts, &declaring));
    operations(
        issues,
        &document.operations,
        declaring.backend,
        document.adopts.is_some(),
    );
    let offers = |name: &str| {
        document
            .operations
            .iter()
            .any(|operation| operation == name)
    };
    Declared {
        offers_update: offers("update"),
        checks_updates: declaring.backend && offers(UPDATE_CHECK),
        adopts,
    }
}

/// Whether `operation` is a standard one, which the application offers
/// itself: never one of a type's own. `diagnostics` is every type's, so no
/// type declares it.
pub fn is_standard_operation(operation: &str) -> bool {
    BUILTIN_OPERATIONS.contains(&operation) || [UPDATE_CHECK, "diagnostics"].contains(&operation)
}

/// The built-in type `adopts` names, for a type that can take its
/// management: on Python with the `control` role, the management API that
/// serves it, and with a `core` of its own, which the sections change.
fn adopted(issues: &mut Issues, adopts: &str, declaring: &Type<'_>) -> Option<InstanceTypeId> {
    const LOCATION: &str = "management.adopts";
    let id = InstanceTypeId::parse(adopts)
        .ok()
        .filter(|id| BuiltinProvider::family_of(id).is_some());
    if id.is_none() {
        let builtin: Vec<String> = WebUiFamily::ALL
            .into_iter()
            .map(|family| BuiltinProvider::type_id(family).to_string())
            .collect();
        issues.add(
            LOCATION,
            format!("`{adopts}` is not a built-in type ({})", builtin.join(", ")),
        );
    }
    if !declaring.runtime.is_python() || !declaring.control {
        issues.add(
            LOCATION,
            super::super::documentation::ADOPTS_RUNTIME.message,
        );
    }
    if declaring.core_may_be_external {
        issues.add(LOCATION, super::super::documentation::ADOPTS_CORE.message);
    }
    id
}

/// The operations a type offers: `update` and `terminal`; with `adopts`
/// the sections of the built-in type it adopts; and for a type its backend
/// provides `update-check` (with `update`) and its own
/// (`provider.manage.<name>`), named with `[a-z0-9-]`. The other standard
/// operations are the built-in types' alone.
fn operations(issues: &mut Issues, declared: &[String], backend: bool, adopts: bool) {
    for (index, operation) in declared.iter().enumerate() {
        let location = format!("management.operations[{index}]");
        if OPERATIONS.contains(&operation.as_str()) {
            continue;
        }
        if operation == UPDATE_CHECK {
            if !backend {
                issues.add(
                    location,
                    super::super::documentation::UPDATE_PROVIDER.message,
                );
            } else if !declared.iter().any(|operation| operation == "update") {
                issues.add(location, super::super::documentation::UPDATE_CHECK.message);
            }
            continue;
        }
        if ADOPTED_OPERATIONS.contains(&operation.as_str()) {
            if !adopts {
                issues.add(
                    location,
                    format!(
                        "`{operation}` is a built-in type's management, which a type offers by \
                         adopting one (`[management] adopts = \"builtin:<name>\"`)"
                    ),
                );
            }
        } else if is_standard_operation(operation) {
            issues.add(
                location,
                format!("`{operation}` is the application's own management, which an extension's type cannot declare"),
            );
        } else if !backend {
            issues.add(
                location,
                format!(
                    "`{operation}` is not an operation a declarative type can offer ({}): \
                     an operation of a type's own is its backend's (`provider = \"backend\"`)",
                    OPERATIONS.join(", ")
                ),
            );
        } else if !is_name(operation) {
            issues.add(
                location,
                format!("operation `{operation}` is not a name of [a-z0-9-]"),
            );
        }
    }
}

fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
}
