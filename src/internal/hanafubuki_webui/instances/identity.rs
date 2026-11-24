use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::internal::hanafubuki_webui::instances) static NEXT_INSTANCE_ID: AtomicU64 =
    AtomicU64::new(1);

/// Resolve the caller's optional name into the persisted instance name.
///
/// The name is optional at the boundary: when it is absent or blank the
/// backend generates one from the type's default prefix plus a random suffix.
/// Generation lives here rather than in the frontend because the result is
/// persisted identity, and Rust already owns naming, uniqueness, and the id
/// derived from it.
/// The requested name, or `<default> <random suffix>` when none is given.
pub(super) fn resolve_instance_name(name: Option<&str>, default: &str) -> Result<String, String> {
    match name.map(str::trim).filter(|value| !value.is_empty()) {
        Some(provided) => normalize_instance_name(provided),
        None => Ok(format!("{default} {}", random_name_suffix())),
    }
}

/// Short lowercase hex suffix. Collisions are harmless — names are not
/// identities — so a failure to read system randomness falls back to the clock
/// rather than failing the whole creation.
fn random_name_suffix() -> String {
    let mut bytes = [0_u8; 3];
    if getrandom::fill(&mut bytes).is_err() {
        let fallback = now_millis() as u64;
        bytes.copy_from_slice(&fallback.to_le_bytes()[..3]);
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn normalize_instance_name(name: &str) -> Result<String, String> {
    let normalized = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err("instance name must not be empty".to_string());
    }
    if normalized.chars().count() > 80 {
        return Err("instance name must not exceed 80 characters".to_string());
    }
    if normalized.chars().any(char::is_control) {
        return Err("instance name contains control characters".to_string());
    }
    Ok(normalized)
}

pub(super) fn generate_instance_id(name: &str) -> String {
    let slug: String = name
        .chars()
        .flat_map(char::to_lowercase)
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() { "instance" } else { &slug };
    let sequence = NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{}-{sequence}",
        slug.chars().take(48).collect::<String>(),
        now_millis()
    )
}

pub(super) fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
