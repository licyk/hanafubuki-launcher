use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::internal::hanafubuki_python_runtime::PythonVersion;
use serde::Serialize;

use super::environment::ProcessEnvironment;

/// Which of a declarative type's step lists an install operation runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepOperation {
    /// A first install and a retry: the `install` steps.
    Install,
    /// A forced reinstall of a ready instance: the `install` steps too.
    Reinstall,
    /// A reinstall of a ready instance whose type declares `update`: its
    /// `update` steps, or its `install` steps when it declares none.
    Update,
}

/// A location inside one of the instance's components: `relative` holds
/// only ordinary names, so the location cannot leave `root` by its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentPath {
    pub component: String,
    pub root: PathBuf,
    pub relative: PathBuf,
    /// The component is a folder the instance does not own: a step may
    /// write files in it, never remove or replace what is there.
    pub external: bool,
}

impl ComponentPath {
    pub fn path(&self) -> PathBuf {
        if self.relative.as_os_str().is_empty() {
            self.root.clone()
        } else {
            self.root.join(&self.relative)
        }
    }
}

/// Where a download step puts what it fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadTarget {
    File(ComponentPath),
    /// An archive, unpacked into `into` and not kept, the first `strip`
    /// components of each entry's path dropped.
    UnpackInto {
        into: ComponentPath,
        strip: u32,
        /// `into` is replaced whole, atomically, instead of merged into.
        replace: bool,
    },
}

/// One step of a declarative type, its placeholders resolved for one
/// instance. Every path is inside one of the instance's components.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlannedStep {
    Download {
        url: String,
        /// Lowercase hex.
        sha256: String,
        target: DownloadTarget,
    },
    Unpack {
        archive: ComponentPath,
        into: ComponentPath,
        strip: u32,
        replace: bool,
    },
    GitClone {
        url: String,
        into: ComponentPath,
        branch: Option<String>,
    },
    GitPull {
        repository: ComponentPath,
    },
    Run {
        command: Vec<String>,
        cwd: ComponentPath,
        env: ProcessEnvironment,
    },
    PythonEnv {
        python_dir: PathBuf,
        version: PythonVersion,
    },
    WriteFile {
        path: ComponentPath,
        content: String,
    },
    /// A 2xx answer to `GET url`, a loopback `http://` URL, within `timeout`.
    Probe {
        url: String,
        timeout: Duration,
    },
}

impl PlannedStep {
    pub fn kind(&self) -> InstallStepKind {
        match self {
            Self::Download { .. } => InstallStepKind::Download,
            Self::Unpack { .. } => InstallStepKind::Unpack,
            Self::GitClone { .. } => InstallStepKind::GitClone,
            Self::GitPull { .. } => InstallStepKind::GitPull,
            Self::Run { .. } => InstallStepKind::Run,
            Self::PythonEnv { .. } => InstallStepKind::PythonEnv,
            Self::WriteFile { .. } => InstallStepKind::WriteFile,
            Self::Probe { .. } => InstallStepKind::Probe,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallStepKind {
    Download,
    Unpack,
    GitClone,
    GitPull,
    Run,
    PythonEnv,
    WriteFile,
    Probe,
}

/// The steps one install operation runs, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepPlan {
    pub steps: Vec<PlannedStep>,
    /// The Python environment a `python` runtime's `run` steps use.
    pub python_dir: Option<PathBuf>,
}

/// The step an install operation is running, for its snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStepProgress {
    /// From 1.
    pub index: usize,
    pub count: usize,
    pub kind: InstallStepKind,
}

/// What the application gives a step plan to run with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepEnvironment {
    /// The instance's own variables, over a step's
    /// ([`ProcessEnvironment::compose`]). The Instance task a step runs as
    /// applies the application's managed variables over both.
    pub env: BTreeMap<String, String>,
    /// The proxy downloads go through.
    pub proxy: Option<String>,
    /// A GitHub mirror replacing `https://github.com` in a download URL, tried
    /// before the URL itself (`https://mirror.example/https://github.com`).
    pub github_mirror: Option<String>,
}

impl StepEnvironment {
    /// The URLs a download tries, the mirror's first.
    pub fn download_urls(&self, url: &str) -> Vec<String> {
        const GITHUB: &str = "https://github.com";
        let mirrored = self.github_mirror.as_deref().and_then(|mirror| {
            url.strip_prefix(GITHUB)
                .filter(|rest| rest.starts_with('/'))
                .map(|rest| format!("{}{rest}", mirror.trim_end_matches('/')))
        });
        mirrored.into_iter().chain([url.to_string()]).collect()
    }
}
