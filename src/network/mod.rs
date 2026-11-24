use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::internal::hanafubuki_process::executor::ExecutorManager;
use reqwest::blocking::Client;

mod proxy;

use proxy::ProxyAddress;

const PROXY_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// Once one source has answered, how long the others may still take. A release
/// published only to the slower source is picked up on a later start.
pub const SOURCE_GRACE: Duration = Duration::from_secs(3);

/// Requests every source at once and returns their results in source order.
/// After the first successful answer the rest get [`SOURCE_GRACE`]; one still
/// running then is `None`, and its thread ends on its own request timeout.
pub fn fetch_sources<S, T, E, F>(sources: Vec<S>, fetch: F) -> Vec<Option<Result<T, E>>>
where
    S: Send + 'static,
    T: Send + 'static,
    E: Send + 'static,
    F: Fn(S) -> Result<T, E> + Send + Sync + 'static,
{
    fetch_sources_with_grace(sources, fetch, SOURCE_GRACE)
}

fn fetch_sources_with_grace<S, T, E, F>(
    sources: Vec<S>,
    fetch: F,
    grace: Duration,
) -> Vec<Option<Result<T, E>>>
where
    S: Send + 'static,
    T: Send + 'static,
    E: Send + 'static,
    F: Fn(S) -> Result<T, E> + Send + Sync + 'static,
{
    let fetch = Arc::new(fetch);
    let (sender, receiver) = mpsc::channel();
    let mut results = sources.iter().map(|_| None).collect::<Vec<_>>();
    for (index, source) in sources.into_iter().enumerate() {
        let sender = sender.clone();
        let fetch = fetch.clone();
        let spawned = thread::Builder::new()
            .name("launcher-source".into())
            .spawn(move || {
                let _ = sender.send((index, fetch(source)));
            });
        if let Err(error) = spawned {
            log::warn!(target: "launcher::network", "source request thread failed: {error}");
        }
    }
    drop(sender);

    let mut deadline: Option<Instant> = None;
    for _ in 0..results.len() {
        let received = match deadline {
            None => receiver.recv().ok(),
            Some(deadline) => receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .ok(),
        };
        let Some((index, result)) = received else {
            break;
        };
        if result.is_ok() && deadline.is_none() {
            deadline = Some(Instant::now() + grace);
        }
        results[index] = Some(result);
    }
    results
}

#[derive(Clone, Debug)]
pub struct NetworkContext {
    proxy: Option<ProxyAddress>,
}

impl NetworkContext {
    pub fn detect(executor: &ExecutorManager) -> Self {
        let candidate = match crate::platform::system_proxy_address(executor) {
            Ok(candidate) => candidate,
            Err(error) => {
                log::warn!(
                    target: "launcher::network",
                    "system proxy discovery failed; using direct networking: {error}"
                );
                return Self { proxy: None };
            }
        };
        let Some(candidate) = candidate else {
            log::info!(
                target: "launcher::network",
                "no system proxy is configured; using direct networking"
            );
            return Self { proxy: None };
        };
        let proxy = match ProxyAddress::parse(&candidate) {
            Ok(proxy) => proxy,
            Err(error) => {
                log::warn!(
                    target: "launcher::network",
                    "system proxy configuration is invalid; using direct networking: {error}"
                );
                return Self { proxy: None };
            }
        };
        if !endpoint_is_reachable(&proxy, PROXY_CONNECT_TIMEOUT) {
            log::warn!(
                target: "launcher::network",
                "system proxy is unreachable; using direct networking"
            );
            return Self { proxy: None };
        }
        log::info!(
            target: "launcher::network",
            "system proxy is reachable; applying it to launcher requests"
        );
        Self { proxy: Some(proxy) }
    }

    pub fn client(&self, user_agent: &'static str) -> Result<Client, String> {
        let mut builder = Client::builder()
            .no_proxy()
            .connect_timeout(HTTP_CONNECT_TIMEOUT)
            .user_agent(user_agent);
        if let Some(proxy) = &self.proxy {
            builder = builder.proxy(
                reqwest::Proxy::all(proxy.as_str())
                    .map_err(|error| format!("Configure launcher proxy: {error}"))?,
            );
        }
        builder
            .build()
            .map_err(|error| format!("Create launcher HTTP client: {error}"))
    }
}

fn endpoint_is_reachable(proxy: &ProxyAddress, timeout: Duration) -> bool {
    let Ok(addresses) = (proxy.host(), proxy.port()).to_socket_addrs() else {
        return false;
    };
    let started = Instant::now();
    for address in addresses {
        let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
            return false;
        };
        if remaining.is_zero() {
            return false;
        }
        if TcpStream::connect_timeout(&address, remaining).is_ok() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
