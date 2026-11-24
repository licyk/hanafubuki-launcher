#[cfg(target_os = "linux")]
use crate::internal::hanafubuki_process::executor::ExecutorManager;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(target_os = "linux")]
const MAX_KDE_CONFIG_BYTES: u64 = 256 * 1024;

#[cfg(target_os = "linux")]
pub(super) fn detect(executor: &ExecutorManager) -> Result<Option<String>, String> {
    match detect_gnome(executor) {
        Ok(Some(proxy)) => return Ok(Some(proxy)),
        Ok(None) => {}
        Err(error) => log::debug!(
            target: "launcher::network",
            "GNOME system proxy discovery failed: {error}"
        ),
    }
    detect_kde()
}

#[cfg(target_os = "linux")]
fn detect_gnome(executor: &ExecutorManager) -> Result<Option<String>, String> {
    let mode = gsettings(executor, &["get", "org.gnome.system.proxy", "mode"])?;
    if unquote(&mode) != "manual" {
        return Ok(None);
    }
    let http_host = gsettings(executor, &["get", "org.gnome.system.proxy.http", "host"])?;
    let http_port = gsettings(executor, &["get", "org.gnome.system.proxy.http", "port"])?;
    if let Some(proxy) = host_port_proxy("http", &http_host, &http_port) {
        return Ok(Some(proxy));
    }
    let socks_host = gsettings(executor, &["get", "org.gnome.system.proxy.socks", "host"])?;
    let socks_port = gsettings(executor, &["get", "org.gnome.system.proxy.socks", "port"])?;
    Ok(host_port_proxy("socks", &socks_host, &socks_port))
}

#[cfg(target_os = "linux")]
fn gsettings(executor: &ExecutorManager, arguments: &[&str]) -> Result<String, String> {
    super::command::output(executor, "gsettings", arguments, COMMAND_TIMEOUT)
}

fn host_port_proxy(scheme: &str, host: &str, port: &str) -> Option<String> {
    let host = unquote(host);
    let port = unquote(port);
    let port = port.parse::<u16>().ok().filter(|port| *port > 0)?;
    (!host.is_empty()).then(|| format!("{scheme}://{host}:{port}"))
}

fn unquote(value: &str) -> &str {
    value
        .trim()
        .trim_matches(|character| matches!(character, '\'' | '"'))
        .trim()
}

#[cfg(target_os = "linux")]
fn detect_kde() -> Result<Option<String>, String> {
    let Some(home) = std::env::var_os("HOME") else {
        return Ok(None);
    };
    let path = PathBuf::from(home).join(".config").join("kioslaverc");
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Inspect KDE proxy configuration: {error}")),
    };
    if !metadata.is_file() || metadata.len() > MAX_KDE_CONFIG_BYTES {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| format!("Read KDE proxy configuration: {error}"))?;
    Ok(parse_kde_config(&content))
}

fn parse_kde_config(content: &str) -> Option<String> {
    let mut in_proxy_settings = false;
    let mut proxy_type = None;
    let mut http = None;
    let mut https = None;
    let mut socks = None;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_proxy_settings = line[1..line.len() - 1].trim() == "Proxy Settings";
            continue;
        }
        if !in_proxy_settings {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "ProxyType" => proxy_type = Some(value.trim()),
            "httpProxy" => http = Some(value.trim()),
            "httpsProxy" => https = Some(value.trim()),
            "socksProxy" => socks = Some(value.trim()),
            _ => {}
        }
    }
    if proxy_type != Some("1") {
        return None;
    }
    https
        .and_then(|value| normalize_kde_proxy("http", value))
        .or_else(|| http.and_then(|value| normalize_kde_proxy("http", value)))
        .or_else(|| socks.and_then(|value| normalize_kde_proxy("socks", value)))
}

fn normalize_kde_proxy(scheme: &str, raw: &str) -> Option<String> {
    let raw = strip_proxy_scheme(raw.trim());
    let mut parts = raw.split_whitespace();
    let host = parts.next()?;
    let port = parts.next();
    if parts.next().is_some() {
        return None;
    }
    let authority = match port {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };
    (!authority.is_empty()).then(|| format!("{scheme}://{authority}"))
}

fn strip_proxy_scheme(value: &str) -> &str {
    ["http://", "https://", "socks://"]
        .into_iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests;
