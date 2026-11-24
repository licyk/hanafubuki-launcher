//! The extension a declarative type comes from, as its placeholders reach
//! it: `{extension.root}`, the package's directory, and `{host.python}`, the
//! managed runtime's Python. The root gives a type its extension when the
//! extension registers it; a type without one resolves neither.

use std::path::PathBuf;
use std::sync::Arc;

/// The managed runtime's Python, which extension backends also run on.
/// Asked when a step plan or a server's command is resolved, never at
/// registration: the runtime may not be ready then.
pub trait HostPython: Send + Sync {
    /// May wait for the runtime; `abandon` tells it the caller no longer
    /// wants the answer (a server's start that a stop superseded).
    fn resolve(&self, abandon: &dyn Fn() -> bool) -> Result<HostPythonPaths, String>;
}

/// The interpreter, and the verified launcher code it imports
/// `sd_webui_all_in_one` from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPythonPaths {
    pub python: PathBuf,
    pub launcher_code: PathBuf,
}

/// The extension that registered a type.
#[derive(Clone)]
pub struct ExtensionContext {
    /// Its package directory (a development link's folder).
    pub root: PathBuf,
    pub host_python: Arc<dyn HostPython>,
}
