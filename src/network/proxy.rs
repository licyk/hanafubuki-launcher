#[derive(Clone, Debug)]
pub(super) struct ProxyAddress {
    url: reqwest::Url,
}

impl ProxyAddress {
    pub(super) fn parse(raw: &str) -> Result<Self, String> {
        if raw.contains('\0') {
            return Err("proxy address contains NUL".to_string());
        }
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("proxy address is empty".to_string());
        }
        let (scheme, remainder) = raw
            .split_once("://")
            .ok_or_else(|| "proxy address must include a scheme".to_string())?;
        let scheme = scheme.to_ascii_lowercase();
        let scheme = match scheme.as_str() {
            "socks" => "socks5h",
            "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h" => scheme.as_str(),
            _ => return Err("proxy address uses an unsupported scheme".to_string()),
        };
        let normalized = format!("{scheme}://{remainder}");
        let url = reqwest::Url::parse(&normalized)
            .map_err(|error| format!("proxy address is not a valid URL: {error}"))?;
        if url.host_str().is_none() {
            return Err("proxy address must include a host".to_string());
        }
        if url.port().is_none_or(|port| port == 0) {
            return Err("proxy address must include an explicit nonzero port".to_string());
        }
        if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
            return Err("proxy address must not include a path, query, or fragment".to_string());
        }
        Ok(Self { url })
    }

    pub(super) fn as_str(&self) -> &str {
        self.url.as_str()
    }

    pub(super) fn host(&self) -> &str {
        let host = self
            .url
            .host_str()
            .expect("validated proxy address has a host");
        host.strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(host)
    }

    pub(super) fn port(&self) -> u16 {
        self.url
            .port()
            .expect("validated proxy address has an explicit port")
    }
}
