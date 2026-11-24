//! The checks of a type's discovery signatures: markers below the matched
//! directory, a root at or below it, declared components below the root,
//! and values the type's install form reads.

use std::collections::BTreeMap;

use crate::internal::hanafubuki_foundation::form_schema::SchemaField;
use crate::internal::hanafubuki_instance_contracts::{ExternalComponent, InstanceTypeId};

use super::super::document::DiscoveryDocument;
use super::super::parameters::DeclarativeSpec;
use super::Issues;
use crate::internal::hanafubuki_webui::discovery::{DiscoverySignature, RelativePath};

/// Each signature, checked at `discovery[<index>]`; none when the type's id
/// did not parse (which is an issue of its own).
pub(in super::super) fn signatures(
    issues: &mut Issues,
    declared: Vec<DiscoveryDocument>,
    id: Option<&InstanceTypeId>,
    components: &[String],
    external: &BTreeMap<String, ExternalComponent>,
    parameters: &[SchemaField],
) -> Vec<DiscoverySignature> {
    declared
        .into_iter()
        .enumerate()
        .filter_map(|(index, document)| {
            let location = format!("discovery[{index}]");
            signature(
                issues, &location, document, id, components, external, parameters,
            )
        })
        .collect()
}

fn signature(
    issues: &mut Issues,
    location: &str,
    document: DiscoveryDocument,
    id: Option<&InstanceTypeId>,
    components: &[String],
    external: &BTreeMap<String, ExternalComponent>,
    parameters: &[SchemaField],
) -> Option<DiscoverySignature> {
    let known = issues.0.len();
    if document.markers.is_empty() {
        issues.add(
            format!("{location}.markers"),
            super::super::documentation::MARKERS.message,
        );
    }
    let markers: Vec<RelativePath> = document
        .markers
        .iter()
        .enumerate()
        .filter_map(|(index, marker)| {
            issues.take(format!("{location}.markers[{index}]"), marker_path(marker))
        })
        .collect();
    let root = issues.take(
        format!("{location}.root"),
        RelativePath::parse(&document.root),
    );
    let component_paths = document
        .components
        .into_iter()
        .filter_map(|(name, path)| {
            let location = format!("{location}.components.{name}");
            if !components.contains(&name) {
                issues.add(location, format!("`{name}` is not a declared component"));
                return None;
            }
            Some((name, issues.take(location, RelativePath::parse(&path))?))
        })
        .collect();
    for (index, name) in document.external.iter().enumerate() {
        if !external.contains_key(name) {
            issues.add(
                format!("{location}.external[{index}]"),
                format!("`{name}` is not a component declared with `external = \"allowed\"`"),
            );
        }
    }
    issues.take(
        format!("{location}.values"),
        DeclarativeSpec::read(parameters, document.values.clone(), true),
    );
    if issues.0.len() > known {
        return None;
    }
    Some(DiscoverySignature {
        type_id: id?.clone(),
        markers,
        root: root?,
        components: component_paths,
        values: document.values,
        external: document.external.into_iter().collect(),
    })
}

/// A marker names a path below the directory, never the directory itself.
fn marker_path(marker: &str) -> Result<RelativePath, String> {
    let path = RelativePath::parse(marker)?;
    if path.is_empty() {
        return Err(format!("`{marker}` names no path below the directory"));
    }
    Ok(path)
}
