use windows_sys::Win32::System::Registry::KEY_QUERY_VALUE;

use super::registry;

const INTERNET_SETTINGS_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

pub(super) fn detect() -> Result<Option<String>, String> {
    if registry::read_user_dword(
        INTERNET_SETTINGS_KEY,
        "ProxyEnable",
        KEY_QUERY_VALUE,
        "Windows system proxy",
    )? != Some(1)
    {
        return Ok(None);
    }
    let Some(server) = registry::read_user_string(
        INTERNET_SETTINGS_KEY,
        "ProxyServer",
        KEY_QUERY_VALUE,
        "Windows system proxy",
    )?
    else {
        return Ok(None);
    };
    Ok(parse_proxy_server(&server))
}

fn parse_proxy_server(raw: &str) -> Option<String> {
    let mut https = None;
    let mut http = None;
    let mut socks = None;
    let mut bare = None;
    for entry in raw
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        let Some((kind, value)) = entry.split_once('=') else {
            bare.get_or_insert(entry);
            continue;
        };
        match kind.trim().to_ascii_lowercase().as_str() {
            "https" => https = Some(value.trim()),
            "http" => http = Some(value.trim()),
            "socks" => socks = Some(value.trim()),
            _ => {}
        }
    }
    https
        .and_then(|value| format_proxy("http", value))
        .or_else(|| http.and_then(|value| format_proxy("http", value)))
        .or_else(|| socks.and_then(|value| format_proxy("socks", value)))
        .or_else(|| bare.and_then(|value| format_proxy("http", value)))
}

fn format_proxy(scheme: &str, raw: &str) -> Option<String> {
    let value = ["http://", "https://", "socks://"]
        .into_iter()
        .find_map(|prefix| raw.strip_prefix(prefix))
        .unwrap_or(raw)
        .trim();
    (!value.is_empty()).then(|| format!("{scheme}://{value}"))
}

#[cfg(test)]
mod tests;
