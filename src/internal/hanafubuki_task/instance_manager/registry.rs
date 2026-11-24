use std::sync::atomic::Ordering;

use crate::internal::hanafubuki_process::ownership::ProcessOwnerId;

use super::{InstanceIdentity, InstanceManagerState, InstanceProcessKind, InstanceProcessRef};

pub fn register_instance_process_inner(
    state: &InstanceManagerState,
    identity: &InstanceIdentity,
    process_id: String,
    kind: InstanceProcessKind,
    label: Option<String>,
) -> Result<InstanceProcessRef, String> {
    register_instance_process_with_reservation(state, identity, process_id, kind, label, None)
}

fn register_instance_process_with_reservation(
    state: &InstanceManagerState,
    identity: &InstanceIdentity,
    process_id: String,
    kind: InstanceProcessKind,
    label: Option<String>,
    reservation_id: Option<ProcessOwnerId>,
) -> Result<InstanceProcessRef, String> {
    if process_id.trim().is_empty() {
        return Err("process_id must not be empty".to_string());
    }
    let mut processes = state.processes.lock();
    if processes
        .values()
        .flatten()
        .any(|entry| entry.process_id == process_id)
    {
        return Err(format!("process already registered: {process_id}"));
    }
    let instance_id = identity.as_str().to_string();
    let process_ref = InstanceProcessRef {
        instance_id: instance_id.clone(),
        process_id,
        kind,
        label,
        created_at: now_millis().to_string(),
        reservation_id,
    };
    processes
        .entry(instance_id)
        .or_default()
        .push(process_ref.clone());
    Ok(process_ref)
}

pub fn register_reserved_instance_process_inner(
    state: &InstanceManagerState,
    identity: &InstanceIdentity,
    process_id: String,
    kind: InstanceProcessKind,
    label: Option<String>,
    reservation_id: &ProcessOwnerId,
) -> Result<InstanceProcessRef, String> {
    if state
        .fail_reserved_registration
        .swap(false, Ordering::SeqCst)
    {
        return Err("injected reserved instance process registration failure".to_string());
    }
    register_instance_process_with_reservation(
        state,
        identity,
        process_id,
        kind,
        label,
        Some(reservation_id.clone()),
    )
}

pub(crate) fn ensure_reserved_instance_process_inner(
    state: &InstanceManagerState,
    identity: &InstanceIdentity,
    process_id: String,
    kind: InstanceProcessKind,
    label: Option<String>,
    reservation_id: &ProcessOwnerId,
) -> Result<InstanceProcessRef, String> {
    let mut processes = state.processes.lock();
    if let Some(existing) = processes.values().flatten().find(|entry| {
        entry.process_id == process_id
            && entry.instance_id == identity.as_str()
            && entry.kind == kind
            && entry.reservation_id.as_ref() == Some(reservation_id)
    }) {
        return Ok(existing.clone());
    }
    if processes
        .values()
        .flatten()
        .any(|entry| entry.process_id == process_id)
    {
        return Err(format!(
            "process is already registered to a different owner: {process_id}"
        ));
    }
    let instance_id = identity.as_str().to_string();
    let process_ref = InstanceProcessRef {
        instance_id: instance_id.clone(),
        process_id,
        kind,
        label,
        created_at: now_millis().to_string(),
        reservation_id: Some(reservation_id.clone()),
    };
    processes
        .entry(instance_id)
        .or_default()
        .push(process_ref.clone());
    Ok(process_ref)
}

pub(crate) fn find_reserved_instance_process_by_receipt(
    state: &InstanceManagerState,
    process_id: &str,
    reservation_id: &ProcessOwnerId,
) -> Option<InstanceProcessRef> {
    state
        .processes
        .lock()
        .values()
        .flatten()
        .find(|entry| {
            entry.process_id == process_id && entry.reservation_id.as_ref() == Some(reservation_id)
        })
        .cloned()
}

pub fn inject_reserved_instance_registration_failure(state: &InstanceManagerState) {
    state
        .fail_reserved_registration
        .store(true, Ordering::SeqCst);
}

pub fn unregister_reserved_instance_process_inner(
    state: &InstanceManagerState,
    identity: &InstanceIdentity,
    process_id: &str,
    reservation_id: &ProcessOwnerId,
) -> Result<(), String> {
    let instance_id = identity.as_str();
    let mut processes = state.processes.lock();
    let Some(refs) = processes.get_mut(instance_id) else {
        return Ok(());
    };
    refs.retain(|entry| {
        !(entry.process_id == process_id && entry.reservation_id.as_ref() == Some(reservation_id))
    });
    if refs.is_empty() {
        processes.remove(instance_id);
    }
    Ok(())
}

pub fn unregister_reserved_instance_process_by_receipt_inner(
    state: &InstanceManagerState,
    process_id: &str,
    reservation_id: &ProcessOwnerId,
) -> Result<(), String> {
    let mut processes = state.processes.lock();
    let mut empty_instance = None;
    for (instance_id, refs) in processes.iter_mut() {
        let before = refs.len();
        refs.retain(|entry| {
            !(entry.process_id == process_id
                && entry.reservation_id.as_ref() == Some(reservation_id))
        });
        if refs.len() != before {
            if refs.is_empty() {
                empty_instance = Some(instance_id.clone());
            }
            break;
        }
    }
    if let Some(instance_id) = empty_instance {
        processes.remove(&instance_id);
    }
    Ok(())
}

pub fn unregister_instance_process_inner(
    state: &InstanceManagerState,
    identity: &InstanceIdentity,
    process_id: &str,
) -> Result<(), String> {
    let instance_id = identity.as_str();
    let mut processes = state.processes.lock();
    if let Some(refs) = processes.get_mut(instance_id) {
        refs.retain(|entry| entry.process_id != process_id);
        if refs.is_empty() {
            processes.remove(instance_id);
        }
    }
    Ok(())
}

pub fn list_instance_processes_registered_inner(
    state: &InstanceManagerState,
    identity: &InstanceIdentity,
) -> Vec<InstanceProcessRef> {
    state
        .processes
        .lock()
        .get(identity.as_str())
        .cloned()
        .unwrap_or_default()
}

pub fn assert_instance_owns_process(
    state: &InstanceManagerState,
    identity: &InstanceIdentity,
    process_id: &str,
    kind: InstanceProcessKind,
) -> Result<(), String> {
    let owns = state
        .processes
        .lock()
        .get(identity.as_str())
        .is_some_and(|refs| {
            refs.iter()
                .any(|entry| entry.process_id == process_id && entry.kind == kind)
        });
    if owns {
        Ok(())
    } else {
        Err(format!(
            "instance {} does not own process {process_id}",
            identity.as_str()
        ))
    }
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
