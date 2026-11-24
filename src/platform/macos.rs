#[cfg(target_os = "macos")]
use crate::internal::hanafubuki_process::executor::ExecutorManager;
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(target_os = "macos")]
pub(super) fn detect(executor: &ExecutorManager) -> Result<Option<String>, String> {
    let output = super::command::output(executor, "scutil", &["--proxy"], COMMAND_TIMEOUT)?;
    Ok(parse_scutil(&output))
}

fn parse_scutil(output: &str) -> Option<String> {
    let values = output
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect::<HashMap<_, _>>();
    scutil_proxy(&values, "HTTPSEnable", "HTTPSProxy", "HTTPSPort", "http")
        .or_else(|| scutil_proxy(&values, "HTTPEnable", "HTTPProxy", "HTTPPort", "http"))
        .or_else(|| scutil_proxy(&values, "SOCKSEnable", "SOCKSProxy", "SOCKSPort", "socks"))
}

fn scutil_proxy(
    values: &HashMap<&str, &str>,
    enabled_key: &str,
    host_key: &str,
    port_key: &str,
    scheme: &str,
) -> Option<String> {
    if values.get(enabled_key).copied() != Some("1") {
        return None;
    }
    let host = values.get(host_key)?.trim();
    let port = values
        .get(port_key)?
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)?;
    (!host.is_empty()).then(|| format!("{scheme}://{host}:{port}"))
}

#[cfg(test)]
mod tests;
