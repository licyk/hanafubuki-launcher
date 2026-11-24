use serde_json::{Map, Value};

use super::field::{FieldRead, FieldReader};
use super::{AppSettings, PersistedSettingsIssue, PersistedSettingsIssueKind};
use crate::internal::hanafubuki_config::repository::settings::{ProxyMode, validate_proxy_address};

/// Proxy policy only. The Agent's web search and fetch settings are its own
/// configuration's.
pub(super) const FIELDS: &[&str] = &["proxyMode", "proxyAddress"];

/// `ProxyMode` has no fail-closed choice: system, manual, and disabled all
/// send traffic somewhere. So the repair keeps a usable configured proxy
/// whenever there is one, and when there is none it falls back to the default
/// system proxy and reports that at error attention rather than silently.
pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    let address_read = fields.read_quiet("proxyAddress", |value: Option<String>| {
        value.as_deref().map(validate_proxy_address).transpose()
    });
    let address = match address_read {
        FieldRead::Valid(address) => address,
        FieldRead::Missing => {
            fields.missing("proxyAddress");
            None
        }
        FieldRead::Invalid => {
            fields.invalid(
                "proxyAddress",
                PersistedSettingsIssueKind::InvalidPolicyField,
                Some("the proxy address is not a usable proxy URL with a host and port and was removed"),
            );
            None
        }
    };
    let mode = match fields.read_quiet::<ProxyMode>("proxyMode", Ok) {
        FieldRead::Valid(ProxyMode::Manual) if address.is_none() => {
            fields.invalid(
                "proxyMode",
                PersistedSettingsIssueKind::InvalidPolicyField,
                Some("the manual proxy has no usable address; the system proxy is used until a proxy address is configured again"),
            );
            defaults.proxy_mode
        }
        FieldRead::Valid(mode) => mode,
        FieldRead::Missing if address.is_some() => {
            fields.invalid(
                "proxyMode",
                PersistedSettingsIssueKind::InvalidPolicyField,
                Some("the proxy mode is missing; the configured proxy address stays in use as a manual proxy"),
            );
            ProxyMode::Manual
        }
        FieldRead::Missing => {
            fields.missing("proxyMode");
            defaults.proxy_mode
        }
        FieldRead::Invalid if address.is_some() => {
            fields.invalid(
                "proxyMode",
                PersistedSettingsIssueKind::InvalidPolicyField,
                Some("the proxy mode is not supported; the configured proxy address stays in use as a manual proxy"),
            );
            ProxyMode::Manual
        }
        FieldRead::Invalid => {
            fields.invalid(
                "proxyMode",
                PersistedSettingsIssueKind::InvalidPolicyField,
                Some("the proxy mode is not supported; the system proxy is used until a proxy mode is configured again"),
            );
            defaults.proxy_mode
        }
    };
    settings.proxy_mode = mode;
    settings.proxy_address = address;
}
