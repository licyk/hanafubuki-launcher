//! Durable, typed journal for cross-domain instance maintenance.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::internal::hanafubuki_config::repository::safe_file;
use crate::internal::hanafubuki_foundation::persistence::quarantine_safe_json;
use crate::internal::hanafubuki_webui::instances::{
    InstanceMaintenanceIntent, InstanceMaintenanceJournalEntry, InstanceMaintenanceJournalPhase,
    InstanceMaintenanceJournalState, InstanceMaintenanceKind, InstanceMaintenanceOperation,
    InstanceMaintenancePhase, InstanceMigrationCheckpoint, InstanceMigrationPhase,
    InstanceMigrationTransferMode, maintenance_activity_id,
};

mod repair;

use repair::{UnrecoverableEntry, repair_document};

const JOURNAL_SCHEMA_VERSION: u32 = 4;
const TERMINAL_RETENTION: usize = 256;

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalDocument {
    schema_version: u32,
    entries: BTreeMap<String, InstanceMaintenanceJournalEntry>,
}

#[derive(Clone)]
pub struct InstanceMaintenanceJournal {
    file: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl InstanceMaintenanceJournal {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn reserve(
        &self,
        intent: InstanceMaintenanceIntent,
    ) -> Result<InstanceMaintenanceJournalEntry, String> {
        let _guard = self.lock.lock();
        let mut document = self.load_unlocked()?;
        let instance_id = intent.instance_id();
        if let Some(active) = document
            .entries
            .values()
            .find(|entry| entry.intent.instance_id() == instance_id && !entry.state.is_terminal())
        {
            return Err(format!(
                "instance {instance_id} already has durable maintenance operation {}",
                active.operation_id
            ));
        }
        let now = now_millis();
        let operation_id = new_operation_id(now)?;
        let state = match &intent {
            InstanceMaintenanceIntent::Migration(_) => InstanceMaintenanceJournalState::Migration {
                checkpoint: InstanceMigrationCheckpoint {
                    phase: InstanceMigrationPhase::Reserved,
                    transfer_mode: InstanceMigrationTransferMode::Pending,
                    source_identity: None,
                    transfer_root_identity: None,
                    config_committed: false,
                    cleanup_obligation: false,
                },
            },
            _ => InstanceMaintenanceJournalState::Standard {
                phase: InstanceMaintenanceJournalPhase::Reserved,
                cleanup_obligation: false,
            },
        };
        let entry = InstanceMaintenanceJournalEntry {
            operation_id: operation_id.clone(),
            intent,
            state,
            created_at: now,
            updated_at: now,
            last_error: None,
        };
        document.entries.insert(operation_id, entry.clone());
        prune_terminal(&mut document.entries);
        self.persist_unlocked(&document)?;
        Ok(entry)
    }

    pub fn update(
        &self,
        operation_id: &str,
        phase: InstanceMaintenanceJournalPhase,
        cleanup_obligation: bool,
        last_error: Option<String>,
    ) -> Result<InstanceMaintenanceJournalEntry, String> {
        let _guard = self.lock.lock();
        let mut document = self.load_unlocked()?;
        let entry = document
            .entries
            .get_mut(operation_id)
            .ok_or_else(|| format!("maintenance journal entry not found: {operation_id}"))?;
        match &mut entry.state {
            InstanceMaintenanceJournalState::Standard {
                phase: current_phase,
                cleanup_obligation: current_cleanup,
            } => {
                *current_phase = phase;
                *current_cleanup = cleanup_obligation;
            }
            InstanceMaintenanceJournalState::Migration { .. } => {
                return Err(format!(
                    "maintenance operation {operation_id} requires a typed migration update"
                ));
            }
        }
        entry.updated_at = now_millis();
        entry.last_error = last_error;
        let updated = entry.clone();
        self.persist_unlocked(&document)?;
        Ok(updated)
    }

    pub fn update_migration(
        &self,
        operation_id: &str,
        checkpoint: InstanceMigrationCheckpoint,
        last_error: Option<String>,
    ) -> Result<InstanceMaintenanceJournalEntry, String> {
        self.edit_migration(operation_id, last_error, move |current| {
            *current = checkpoint;
            Ok(())
        })
    }

    pub(crate) fn edit_migration(
        &self,
        operation_id: &str,
        last_error: Option<String>,
        edit: impl FnOnce(&mut InstanceMigrationCheckpoint) -> Result<(), String>,
    ) -> Result<InstanceMaintenanceJournalEntry, String> {
        let _guard = self.lock.lock();
        let mut document = self.load_unlocked()?;
        let entry = document
            .entries
            .get_mut(operation_id)
            .ok_or_else(|| format!("maintenance journal entry not found: {operation_id}"))?;
        match &mut entry.state {
            InstanceMaintenanceJournalState::Migration {
                checkpoint: current,
            } => edit(current)?,
            InstanceMaintenanceJournalState::Standard { .. } => {
                return Err(format!(
                    "maintenance operation {operation_id} is not a migration"
                ));
            }
        }
        entry.updated_at = now_millis();
        entry.last_error = last_error;
        let updated = entry.clone();
        self.persist_unlocked(&document)?;
        Ok(updated)
    }

    pub fn pending(&self) -> Result<Vec<InstanceMaintenanceJournalEntry>, String> {
        let _guard = self.lock.lock();
        let mut entries = self
            .load_unlocked()?
            .entries
            .into_values()
            .filter(|entry| !entry.state.is_terminal())
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.created_at);
        Ok(entries)
    }

    pub fn get_entry(&self, operation_id: &str) -> Result<InstanceMaintenanceJournalEntry, String> {
        let _guard = self.lock.lock();
        self.load_unlocked()?
            .entries
            .get(operation_id)
            .cloned()
            .ok_or_else(|| format!("maintenance journal entry not found: {operation_id}"))
    }

    pub fn list_public(&self) -> Result<Vec<InstanceMaintenanceOperation>, String> {
        let _guard = self.lock.lock();
        let mut operations = self
            .load_unlocked()?
            .entries
            .into_values()
            .filter_map(|entry| public_operation(&entry))
            .collect::<Vec<_>>();
        operations.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(operations)
    }

    pub fn get_public(&self, operation_id: &str) -> Result<InstanceMaintenanceOperation, String> {
        let _guard = self.lock.lock();
        let document = self.load_unlocked()?;
        document
            .entries
            .get(operation_id)
            .and_then(public_operation)
            .ok_or_else(|| format!("maintenance operation not found: {operation_id}"))
    }

    fn load_unlocked(&self) -> Result<JournalDocument, String> {
        let empty = || JournalDocument {
            schema_version: JOURNAL_SCHEMA_VERSION,
            entries: BTreeMap::new(),
        };
        let raw = match safe_file::read_json::<Value>(&self.file) {
            Ok(value) => value,
            Err(error) => {
                log::error!(
                    "instance maintenance journal is unreadable; it was quarantined whole and a new journal started path={} error={error}",
                    self.file.display()
                );
                let document = empty();
                self.replace_repaired(&document)?;
                return Ok(document);
            }
        };
        let Some(raw) = raw else {
            return Ok(empty());
        };
        let stored_version = raw.get("schemaVersion").and_then(Value::as_u64);
        if stored_version.is_some_and(|version| version > u64::from(JOURNAL_SCHEMA_VERSION)) {
            // A newer version's journal is not this version's to rewrite: it
            // is kept whole in quarantine, and this version starts its own.
            let quarantined = quarantine_safe_json(&self.file)?;
            log::error!(
                "instance maintenance journal was written by a newer version (schema {}, this version reads {JOURNAL_SCHEMA_VERSION}); it was quarantined whole and not rewritten path={} quarantined={quarantined:?}",
                stored_version.unwrap_or_default(),
                self.file.display()
            );
            return Ok(empty());
        }
        let repaired = repair_document(&raw);
        if !repaired.unrecoverable.is_empty() {
            self.quarantine_unrecoverable(&repaired.unrecoverable)?;
        }
        let document = repaired.document;
        let canonical = serde_json::to_value(&document)
            .map_err(|error| format!("serialize repaired maintenance journal: {error}"))?;
        if canonical != raw {
            self.replace_repaired(&document)?;
        }
        let mut active_by_instance = BTreeMap::new();
        for (key, entry) in &document.entries {
            if key != &entry.operation_id {
                return Err(format!(
                    "maintenance journal key {key} does not match operation {}",
                    entry.operation_id
                ));
            }
            match (&entry.intent, &entry.state) {
                (
                    InstanceMaintenanceIntent::Migration(_),
                    InstanceMaintenanceJournalState::Migration { .. },
                ) => {}
                (InstanceMaintenanceIntent::Migration(_), _)
                | (_, InstanceMaintenanceJournalState::Migration { .. }) => {
                    return Err(format!(
                        "maintenance operation {} intent and journal state do not match",
                        entry.operation_id
                    ));
                }
                _ => {}
            }
            let invalid_cleanup = match &entry.state {
                InstanceMaintenanceJournalState::Standard {
                    phase: InstanceMaintenanceJournalPhase::CleanupRequired,
                    cleanup_obligation,
                } => !cleanup_obligation,
                InstanceMaintenanceJournalState::Migration { checkpoint }
                    if checkpoint.phase == InstanceMigrationPhase::RecoveryRequired =>
                {
                    !checkpoint.cleanup_obligation
                }
                _ => false,
            };
            if invalid_cleanup {
                return Err(format!(
                    "maintenance operation {} omitted its cleanup obligation",
                    entry.operation_id
                ));
            }
            if !entry.state.is_terminal() {
                let instance_id = entry.intent.instance_id();
                if let Some(existing) =
                    active_by_instance.insert(instance_id, entry.operation_id.as_str())
                {
                    return Err(format!(
                        "instance {instance_id} has multiple durable maintenance operations: {existing} and {}",
                        entry.operation_id
                    ));
                }
            }
        }
        Ok(document)
    }

    fn replace_repaired(&self, document: &JournalDocument) -> Result<(), String> {
        let quarantined = quarantine_safe_json(&self.file)?;
        safe_file::write_json_atomic(&self.file, document)?;
        log::warn!(
            "destructively repaired instance maintenance journal path={} quarantined={}",
            self.file.display(),
            quarantined.len()
        );
        Ok(())
    }

    /// Keep the entries the live journal cannot hold in a file of their own,
    /// beside the quarantined original, so pending cleanup they may describe
    /// is never silently forgotten.
    fn quarantine_unrecoverable(
        &self,
        entries: &BTreeMap<String, UnrecoverableEntry>,
    ) -> Result<(), String> {
        let parent = self
            .file
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", self.file.display()))?;
        let name = self
            .file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("maintenance-journal");
        let target = parent.join(".recovery").join(format!(
            "{name}.unrecoverable.{}.{}.json",
            now_millis(),
            std::process::id()
        ));
        safe_file::write_json_atomic(
            &target,
            &serde_json::json!({
                "schemaVersion": JOURNAL_SCHEMA_VERSION,
                "entries": entries,
            }),
        )?;
        log::error!(
            "instance maintenance journal entries could not be recovered and were quarantined operations={:?} path={}",
            entries.keys().collect::<Vec<_>>(),
            target.display()
        );
        Ok(())
    }

    fn persist_unlocked(&self, document: &JournalDocument) -> Result<(), String> {
        safe_file::write_json_atomic(&self.file, document)
    }
}

fn public_operation(
    entry: &InstanceMaintenanceJournalEntry,
) -> Option<InstanceMaintenanceOperation> {
    let (instance_id, maintenance_kind, phase) = match (&entry.intent, &entry.state) {
        (intent, InstanceMaintenanceJournalState::Standard { phase, .. }) => {
            let (instance_id, kind, committing) = match intent {
                InstanceMaintenanceIntent::ComponentPathUpdate(request) => (
                    &request.instance_id,
                    InstanceMaintenanceKind::ComponentPathUpdate,
                    InstanceMaintenancePhase::UpdatingComponentPaths,
                ),
                InstanceMaintenanceIntent::Reinstall(request) => (
                    &request.instance_id,
                    InstanceMaintenanceKind::Reinstall,
                    InstanceMaintenancePhase::StartingReinstall,
                ),
                InstanceMaintenanceIntent::Delete(request) => (
                    &request.instance_id,
                    InstanceMaintenanceKind::Delete,
                    InstanceMaintenancePhase::DeletingInstance,
                ),
                InstanceMaintenanceIntent::DeleteRecord(request) => (
                    &request.instance_id,
                    InstanceMaintenanceKind::DeleteRecord,
                    InstanceMaintenancePhase::RemovingRecord,
                ),
                InstanceMaintenanceIntent::Migration(_) => return None,
            };
            let public_phase = match phase {
                InstanceMaintenanceJournalPhase::Reserved
                | InstanceMaintenanceJournalPhase::Quiescing => InstanceMaintenancePhase::Stopping,
                InstanceMaintenanceJournalPhase::Committing => committing,
                InstanceMaintenanceJournalPhase::CleanupRequired => {
                    InstanceMaintenancePhase::RecoveryRequired
                }
                InstanceMaintenanceJournalPhase::Completed => InstanceMaintenancePhase::Completed,
                InstanceMaintenanceJournalPhase::Failed => InstanceMaintenancePhase::Failed,
            };
            (instance_id.clone(), kind, public_phase)
        }
        (
            InstanceMaintenanceIntent::Migration(plan),
            InstanceMaintenanceJournalState::Migration { checkpoint },
        ) => {
            let public_phase = match checkpoint.phase {
                InstanceMigrationPhase::Reserved | InstanceMigrationPhase::Quiescing => {
                    InstanceMaintenancePhase::Stopping
                }
                InstanceMigrationPhase::Transferring => InstanceMaintenancePhase::MovingFiles,
                InstanceMigrationPhase::SwitchingConfig => {
                    InstanceMaintenancePhase::SwitchingConfig
                }
                InstanceMigrationPhase::RemovingSource => InstanceMaintenancePhase::RemovingSource,
                InstanceMigrationPhase::RecoveryRequired => {
                    InstanceMaintenancePhase::RecoveryRequired
                }
                InstanceMigrationPhase::Completed => InstanceMaintenancePhase::Completed,
                InstanceMigrationPhase::Failed => InstanceMaintenancePhase::Failed,
                InstanceMigrationPhase::Cancelled => InstanceMaintenancePhase::Cancelled,
            };
            (
                plan.instance_id.clone(),
                InstanceMaintenanceKind::Migration,
                public_phase,
            )
        }
        _ => return None,
    };
    Some(InstanceMaintenanceOperation {
        operation_id: entry.operation_id.clone(),
        activity_id: maintenance_activity_id(&entry.operation_id),
        instance_id,
        kind: maintenance_kind,
        phase,
        created_at: entry.created_at,
        updated_at: entry.updated_at,
        error: entry.last_error.clone(),
        cancellable: matches!(
            &entry.state,
            InstanceMaintenanceJournalState::Migration { checkpoint }
                if checkpoint.can_cancel_without_transfer()
        ),
    })
}

fn prune_terminal(entries: &mut BTreeMap<String, InstanceMaintenanceJournalEntry>) {
    let mut terminal = entries
        .values()
        .filter(|entry| entry.state.is_terminal())
        .map(|entry| (entry.updated_at, entry.operation_id.clone()))
        .collect::<Vec<_>>();
    terminal.sort_by_key(|(updated_at, _)| *updated_at);
    let remove_count = terminal.len().saturating_sub(TERMINAL_RETENTION);
    for (_, operation_id) in terminal.into_iter().take(remove_count) {
        entries.remove(&operation_id);
    }
}

fn new_operation_id(now: u128) -> Result<String, String> {
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("generate maintenance operation id: {error}"))?;
    let suffix = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("maintenance-{now}-{suffix}"))
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
