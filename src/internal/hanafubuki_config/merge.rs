//! Recursive JSON merge and structured path helpers for layered configuration.
//!
//! These utilities implement the merge semantics used when resolving the
//! configuration layers (defaults -> global -> instance -> runtime). The rules
//! are intentionally simple and explicit:
//!
//! - If both `base` and `overlay` are JSON objects, their keys are merged
//!   recursively.
//! - Otherwise `overlay` replaces `base` (scalars and mismatched types).
//! - Arrays are replaced as whole values; they are never merged element-wise.
//!
//! `null` handling has **two explicit modes**, because the two call sites need
//! opposite semantics:
//!
//! - [`merge_json_keep_null`] — a `null` in the overlay is a real value that
//!   **replaces** the base key with `null`. This is required when resolving
//!   layered configuration into a typed `AppSettings`, whose nullable fields
//!   (custom mirrors) must be allowed to be `null` and must remain present
//!   in the merged JSON so deserialization does not fail on a missing key.
//! - [`merge_json_remove_null`] — a `null` in the overlay **removes** that key
//!   from the result ("unset this key"). This is used for edit/patch operations
//!   (e.g. deleting an `envVars` entry or clearing an optional field) where the
//!   caller wants the key gone rather than set to JSON `null`.

use serde_json::Value;

/// Recursively merge `overlay` onto `base`, treating a `null` overlay value as a
/// real value that replaces the base key. See the module docs.
pub fn merge_json_keep_null(base: Value, overlay: Value) -> Value {
    merge_json_inner(base, overlay, false)
}

/// Recursively merge `overlay` onto `base`, treating a `null` overlay value as a
/// request to remove that key. See the module docs.
pub fn merge_json_remove_null(base: Value, overlay: Value) -> Value {
    merge_json_inner(base, overlay, true)
}

fn merge_json_inner(base: Value, overlay: Value, remove_null: bool) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                if remove_null && overlay_value.is_null() {
                    base_map.remove(&key);
                    continue;
                }
                let merged = match base_map.remove(&key) {
                    Some(base_value) => merge_json_inner(base_value, overlay_value, remove_null),
                    None => overlay_value,
                };
                base_map.insert(key, merged);
            }
            Value::Object(base_map)
        }
        // Overlay replaces base for every non-object-vs-object combination
        // (scalars, arrays, and type mismatches). In keep-null mode a bare
        // `null` overlay therefore replaces the base value with `null`.
        (_, overlay) => overlay,
    }
}

#[cfg(test)]
mod tests;
