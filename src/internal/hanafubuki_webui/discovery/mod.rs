//! Reusable, persistence-free WebUI directory discovery: the built-in
//! families' layouts, and the signatures registered types declare.

mod models;
mod scanner;
mod signatures;

pub use models::{
    BuiltinDiscoveryCandidate, DeclaredDiscoveryCandidate, MAX_DISCOVERY_WARNINGS,
    WebUiDiscoveryCandidate, WebUiDiscoveryProgress, WebUiDiscoveryReport, WebUiDiscoveryWarning,
};
pub use scanner::scan_webui_roots;
pub use signatures::{DiscoverySignature, RelativePath};

#[cfg(test)]
mod tests;
