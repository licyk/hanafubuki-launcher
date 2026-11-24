pub(crate) mod elevation;
pub(crate) mod registry;
mod system_proxy;

pub(crate) fn system_proxy_address() -> Result<Option<String>, String> {
    system_proxy::detect()
}
