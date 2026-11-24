//! Decoding a persisted set of disabled names, one name at a time.

use std::collections::BTreeSet;

use serde_json::Map;

use super::PersistedSettingsIssueKind;
use super::field::FieldReader;
use crate::internal::hanafubuki_config::repository::settings::names::DisabledNames;

/// How a decoder's reports name the list and one of its names.
pub(super) struct DisabledList {
    /// `module`.
    pub list: &'static str,
    /// `plugin`, `module`.
    pub item: &'static str,
}

/// A name that cannot be one is dropped alone. Losing the whole set would
/// enable what the user turned off, so that is reported at error attention.
pub(super) fn decode_disabled_names(
    fields: &mut FieldReader<'_>,
    names: &DisabledNames,
    DisabledList { list, item }: DisabledList,
) -> BTreeSet<String> {
    let key = names.key;
    let pointer = fields.pointer(key);
    let Some(raw) = fields.raw(key) else {
        fields.missing(key);
        return BTreeSet::new();
    };
    let Some(items) = raw.as_array() else {
        fields.invalid(
            key,
            PersistedSettingsIssueKind::InvalidPolicyField,
            Some(&format!(
                "the disabled {list} list is unreadable; every {item} is enabled until {item}s are turned off again"
            )),
        );
        return BTreeSet::new();
    };
    let mut kept = BTreeSet::new();
    let items_object = Map::new();
    let mut nested = FieldReader::nested(&items_object, &pointer, fields.issues());
    for (index, value) in items.iter().enumerate() {
        let index = index.to_string();
        let name = value.as_str().filter(|name| names.is_valid_name(name));
        match name {
            Some(name) if kept.len() < names.max_names => {
                kept.insert(name.to_string());
            }
            Some(_) => nested.invalid(
                &index,
                PersistedSettingsIssueKind::InvalidPolicyField,
                Some(&format!(
                    "the disabled {list} list holds too many names; this one was dropped and its {item} is enabled"
                )),
            ),
            None => nested.invalid(
                &index,
                PersistedSettingsIssueKind::InvalidField,
                Some(&format!(
                    "a disabled {list} name that no {item} can carry was dropped"
                )),
            ),
        }
    }
    kept
}
