//! Repair of a stored maintenance journal: unknown fields are dropped, an entry
//! whose state cannot be read becomes recovery-required, and what cannot be
//! kept is handed back for quarantine. The journal invariants themselves are
//! checked by the caller and stay strict.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::internal::hanafubuki_config::repository::instances::INSTANCE_COMPONENT_PATHS_CURRENT_JSON_SHAPE;
use crate::internal::hanafubuki_foundation::json_normalization::{
    JsonField, JsonShape, JsonVariant, OPAQUE_JSON_SHAPE, normalize_current_json,
};
use crate::internal::hanafubuki_webui::instances::{
    InstanceMaintenanceIntent, InstanceMaintenanceJournalEntry, InstanceMaintenanceJournalPhase,
    InstanceMaintenanceJournalState, InstanceMigrationCheckpoint, InstanceMigrationPhase,
    InstanceMigrationTransferMode,
};

use super::{JOURNAL_SCHEMA_VERSION, JournalDocument, now_millis};

const IDENTITY_FIELDS: &[JsonField] = &[
    JsonField::new("storageId", &OPAQUE_JSON_SHAPE),
    JsonField::new("fileId", &OPAQUE_JSON_SHAPE),
];
const IDENTITY_SHAPE: JsonShape = JsonShape::Object(IDENTITY_FIELDS);
const NULLABLE_IDENTITY_SHAPE: JsonShape = JsonShape::Nullable(&IDENTITY_SHAPE);
const MIGRATION_CHECKPOINT_FIELDS: &[JsonField] = &[
    JsonField::new("phase", &OPAQUE_JSON_SHAPE),
    JsonField::new("transferMode", &OPAQUE_JSON_SHAPE),
    JsonField::new("sourceIdentity", &NULLABLE_IDENTITY_SHAPE),
    JsonField::new("transferRootIdentity", &NULLABLE_IDENTITY_SHAPE),
    JsonField::new("configCommitted", &OPAQUE_JSON_SHAPE),
    JsonField::new("cleanupObligation", &OPAQUE_JSON_SHAPE),
];
const MIGRATION_CHECKPOINT_SHAPE: JsonShape = JsonShape::Object(MIGRATION_CHECKPOINT_FIELDS);
const STANDARD_STATE_FIELDS: &[JsonField] = &[
    JsonField::new("phase", &OPAQUE_JSON_SHAPE),
    JsonField::new("cleanupObligation", &OPAQUE_JSON_SHAPE),
];
const MIGRATION_STATE_FIELDS: &[JsonField] =
    &[JsonField::new("checkpoint", &MIGRATION_CHECKPOINT_SHAPE)];
const STATE_VARIANTS: &[JsonVariant] = &[
    JsonVariant::new("standard", STANDARD_STATE_FIELDS),
    JsonVariant::new("migration", MIGRATION_STATE_FIELDS),
];
const STATE_SHAPE: JsonShape = JsonShape::TaggedObject {
    discriminator: "type",
    common: &[],
    variants: STATE_VARIANTS,
};
const INTENT_COMMON_FIELDS: &[JsonField] = &[JsonField::new("instanceId", &OPAQUE_JSON_SHAPE)];
const COMPONENT_PATH_UPDATE_FIELDS: &[JsonField] = &[
    JsonField::new("expectedConfigRevision", &OPAQUE_JSON_SHAPE),
    JsonField::new(
        "componentPaths",
        &INSTANCE_COMPONENT_PATHS_CURRENT_JSON_SHAPE,
    ),
];
const REINSTALL_FIELDS: &[JsonField] = &[
    JsonField::new("expectedInstanceIdentity", &OPAQUE_JSON_SHAPE),
    JsonField::new("expectedConfigRevision", &OPAQUE_JSON_SHAPE),
    JsonField::new("forceRecreatePython", &OPAQUE_JSON_SHAPE),
    JsonField::new("forceRecreateCore", &OPAQUE_JSON_SHAPE),
];
const DELETE_FIELDS: &[JsonField] = &[
    JsonField::new("expectedInstanceIdentity", &OPAQUE_JSON_SHAPE),
    JsonField::new("expectedConfigRevision", &OPAQUE_JSON_SHAPE),
];
const MIGRATION_FIELDS: &[JsonField] = &[
    JsonField::new("expectedInstanceIdentity", &OPAQUE_JSON_SHAPE),
    JsonField::new("expectedConfigRevision", &OPAQUE_JSON_SHAPE),
    JsonField::new("sourceInstallDir", &OPAQUE_JSON_SHAPE),
    JsonField::new("destinationInstallDir", &OPAQUE_JSON_SHAPE),
    JsonField::new(
        "sourceComponentPaths",
        &INSTANCE_COMPONENT_PATHS_CURRENT_JSON_SHAPE,
    ),
    JsonField::new(
        "destinationComponentPaths",
        &INSTANCE_COMPONENT_PATHS_CURRENT_JSON_SHAPE,
    ),
];
const INTENT_VARIANTS: &[JsonVariant] = &[
    JsonVariant::new("componentPathUpdate", COMPONENT_PATH_UPDATE_FIELDS),
    JsonVariant::new("reinstall", REINSTALL_FIELDS),
    JsonVariant::new("delete", DELETE_FIELDS),
    JsonVariant::new("deleteRecord", DELETE_FIELDS),
    JsonVariant::new("migration", MIGRATION_FIELDS),
];
const INTENT_SHAPE: JsonShape = JsonShape::TaggedObject {
    discriminator: "kind",
    common: INTENT_COMMON_FIELDS,
    variants: INTENT_VARIANTS,
};
const ENTRY_FIELDS: &[JsonField] = &[
    JsonField::new("operationId", &OPAQUE_JSON_SHAPE),
    JsonField::new("intent", &INTENT_SHAPE),
    JsonField::new("state", &STATE_SHAPE),
    JsonField::new("createdAt", &OPAQUE_JSON_SHAPE),
    JsonField::new("updatedAt", &OPAQUE_JSON_SHAPE),
    JsonField::new("lastError", &OPAQUE_JSON_SHAPE),
];
const ENTRY_SHAPE: JsonShape = JsonShape::Object(ENTRY_FIELDS);

pub(super) struct RepairedJournal {
    pub document: JournalDocument,
    pub unrecoverable: BTreeMap<String, UnrecoverableEntry>,
}

/// A stored entry the live journal cannot keep, with why.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UnrecoverableEntry {
    pub reason: String,
    pub entry: Value,
}

/// Repair a journal document of this schema (or an older one) entry by entry.
///
/// An entry whose intent can still be read but whose state cannot is kept as
/// recovery-required. One that cannot be recovered at all, or a second active
/// entry for an instance, is not kept in the live journal but returned with
/// its raw value, so the caller can quarantine it rather than drop it.
pub(super) fn repair_document(raw: &Value) -> RepairedJournal {
    let mut entries = BTreeMap::new();
    let mut unrecoverable = BTreeMap::new();
    let Some(mut document) = raw.as_object().cloned() else {
        return RepairedJournal {
            document: JournalDocument {
                schema_version: JOURNAL_SCHEMA_VERSION,
                entries,
            },
            unrecoverable,
        };
    };
    let raw_entries = document
        .remove("entries")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    for (operation_id, original) in raw_entries {
        let mut value = original.clone();
        sanitize_entry(&mut value);
        let recovered = serde_json::from_value::<InstanceMaintenanceJournalEntry>(value.clone())
            .or_else(|error| recover_invalid_entry(&operation_id, &value, error));
        let mut entry = match recovered {
            Ok(entry) => entry,
            Err(error) => {
                unrecoverable.insert(
                    operation_id,
                    UnrecoverableEntry {
                        reason: format!("the entry cannot be decoded: {error}"),
                        entry: original,
                    },
                );
                continue;
            }
        };
        entry.operation_id = operation_id.clone();
        normalize_cleanup_obligation(&mut entry);
        if entries
            .values()
            .any(|existing: &InstanceMaintenanceJournalEntry| {
                !existing.state.is_terminal()
                    && !entry.state.is_terminal()
                    && existing.intent.instance_id() == entry.intent.instance_id()
            })
        {
            unrecoverable.insert(
                operation_id,
                UnrecoverableEntry {
                    reason: format!(
                        "instance {} already has an active maintenance operation",
                        entry.intent.instance_id()
                    ),
                    entry: original,
                },
            );
            continue;
        }
        entries.insert(operation_id, entry);
    }
    RepairedJournal {
        document: JournalDocument {
            schema_version: JOURNAL_SCHEMA_VERSION,
            entries,
        },
        unrecoverable,
    }
}

fn recover_invalid_entry(
    operation_id: &str,
    value: &Value,
    error: serde_json::Error,
) -> Result<InstanceMaintenanceJournalEntry, serde_json::Error> {
    let Some(object) = value.as_object() else {
        return Err(error);
    };
    let Some(intent_value) = object.get("intent") else {
        return Err(error);
    };
    let intent = match serde_json::from_value::<InstanceMaintenanceIntent>(intent_value.clone()) {
        Ok(intent) => intent,
        Err(_) => return Err(error),
    };
    let state = match intent {
        InstanceMaintenanceIntent::Migration(_) => InstanceMaintenanceJournalState::Migration {
            checkpoint: InstanceMigrationCheckpoint {
                phase: InstanceMigrationPhase::RecoveryRequired,
                transfer_mode: InstanceMigrationTransferMode::Pending,
                source_identity: None,
                transfer_root_identity: None,
                config_committed: false,
                cleanup_obligation: true,
            },
        },
        _ => InstanceMaintenanceJournalState::Standard {
            phase: InstanceMaintenanceJournalPhase::CleanupRequired,
            cleanup_obligation: true,
        },
    };
    let now = now_millis();
    Ok(InstanceMaintenanceJournalEntry {
        operation_id: operation_id.to_string(),
        intent,
        state,
        created_at: object
            .get("createdAt")
            .and_then(Value::as_u64)
            .map(u128::from)
            .unwrap_or(now),
        updated_at: now,
        last_error: Some(format!(
            "incompatible maintenance state was reset to recovery-required: {error}"
        )),
    })
}

fn normalize_cleanup_obligation(entry: &mut InstanceMaintenanceJournalEntry) {
    match &mut entry.state {
        InstanceMaintenanceJournalState::Standard {
            phase: InstanceMaintenanceJournalPhase::CleanupRequired,
            cleanup_obligation,
        } => *cleanup_obligation = true,
        InstanceMaintenanceJournalState::Migration { checkpoint }
            if checkpoint.phase == InstanceMigrationPhase::RecoveryRequired =>
        {
            checkpoint.cleanup_obligation = true;
        }
        _ => {}
    }
}

fn sanitize_entry(value: &mut Value) {
    normalize_current_json(value, &ENTRY_SHAPE, |_| {});
}
