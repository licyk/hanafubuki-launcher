use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

/// The name of a process an instance runs and the application supervises:
/// `ui`, `api`, `control`, `worker`, or a type's own name, of `[a-z0-9-]`.
/// Unique within one instance type. Validated once, where it is parsed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ServerRole(String);

impl ServerRole {
    /// The `sd_webui_all_in_one self-manager api serve` server on the
    /// instance's Python, through which the built-in types are managed.
    pub const CONTROL: &'static str = "control";
    /// The WebUI itself.
    pub const UI: &'static str = "ui";

    pub fn parse(value: &str) -> Result<Self, String> {
        let valid = !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'));
        if valid {
            Ok(Self(value.to_string()))
        } else {
            Err(format!(
                "server role `{value}` is not 1 to 64 characters of [a-z0-9-]"
            ))
        }
    }

    pub fn control() -> Self {
        Self(Self::CONTROL.to_string())
    }

    pub fn ui() -> Self {
        Self(Self::UI.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_control(&self) -> bool {
        self.0 == Self::CONTROL
    }
}

impl fmt::Display for ServerRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ServerRole {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// What starts a role's process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ServerStart {
    /// The control server's launcher, on the instance's Python, started by the
    /// server supervisor.
    ControlServer,
    /// The launch workflow's WebUI child, after its preflight stages.
    LaunchWorkflow,
    /// A declared command, started by the server supervisor. `argv` and the
    /// values of `env` may hold the type's placeholders (`{port}`,
    /// `{component.<name>}`, ...), which its provider resolves.
    Command {
        argv: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    /// A command the type's extension backend plans for each start
    /// (`provider.launchPlan`), started by the server supervisor as a
    /// declared command is.
    Backend,
}

/// Where a role listens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ServerPort {
    /// A free loopback port chosen for each start.
    Allocate,
    Fixed {
        port: u16,
    },
    /// The instance's configuration chooses it; for a built-in WebUI, its
    /// launch arguments.
    Config {
        key: String,
    },
}

/// When a role counts as ready. A role that is not ready within its timeout
/// fails with the probe's reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ServerReadiness {
    /// The control server's own probes: its `/health` to start, its
    /// authenticated status to stay healthy.
    Control,
    /// A 2xx answer to `GET <path>` on the role's loopback port.
    Http {
        path: String,
        #[serde(default = "default_timeout_seconds")]
        timeout_seconds: u64,
    },
    /// A TCP connection to the role's loopback port.
    Tcp {
        #[serde(default = "default_timeout_seconds")]
        timeout_seconds: u64,
    },
    /// A line of the role's output matching `pattern` (a regular
    /// expression), read from a bounded tail of that output.
    Stdout {
        pattern: String,
        #[serde(default = "default_timeout_seconds")]
        timeout_seconds: u64,
    },
    /// The address the role's program asks a browser to open, reported over
    /// a runtime broker session: the hotpatcher's for the launch workflow's
    /// WebUI, the program's own for a declared role, which the supervisor
    /// gives a session at each start. Only a declared role is held to
    /// `timeout_seconds`; the launch workflow keeps its own deadline.
    Captured {
        #[serde(default = "default_timeout_seconds")]
        timeout_seconds: u64,
    },
}

fn default_timeout_seconds() -> u64 {
    120
}

/// Whether the supervisor restarts a role whose process exits or stops
/// answering its probe. A restart is bounded by the supervisor's recovery
/// budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServerRestart {
    Never,
    OnFailure,
}

/// One server role an instance type declares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRoleDescriptor {
    pub role: ServerRole,
    pub start: ServerStart,
    pub port: ServerPort,
    pub readiness: ServerReadiness,
    /// The roles that must be ready before this one starts.
    #[serde(default)]
    pub after: Vec<ServerRole>,
    pub restart: ServerRestart,
    /// Whether the embedded webview may show this role's endpoint.
    #[serde(default)]
    pub webview: bool,
    /// Whether the instance fails to start when this role does; an optional
    /// role that fails leaves the instance degraded.
    #[serde(default = "default_required")]
    pub required: bool,
}

fn default_required() -> bool {
    true
}

/// The roles in the order they start: each after every role it names in
/// `after`, otherwise in declaration order. A duplicate role, an `after`
/// naming an undeclared role, a cycle, a readiness its start cannot
/// provide, or a second captured role is an invalid descriptor.
pub fn server_start_order(
    roles: &[ServerRoleDescriptor],
) -> Result<Vec<&ServerRoleDescriptor>, String> {
    let mut declared = BTreeSet::new();
    for role in roles {
        if !declared.insert(&role.role) {
            return Err(format!("server role `{}` is declared twice", role.role));
        }
        check_start_and_readiness(role)?;
    }
    // The runtime broker holds one active session per instance.
    let mut captured = roles
        .iter()
        .filter(|role| matches!(role.readiness, ServerReadiness::Captured { .. }));
    if let (Some(first), Some(second)) = (captured.next(), captured.next()) {
        return Err(format!(
            "server roles `{}` and `{}` are both captured; an instance has one captured role",
            first.role, second.role
        ));
    }
    for role in roles {
        if let Some(missing) = role.after.iter().find(|after| !declared.contains(after)) {
            return Err(format!(
                "server role `{}` starts after `{missing}`, which is not declared",
                role.role
            ));
        }
    }
    let mut ordered: Vec<&ServerRoleDescriptor> = Vec::with_capacity(roles.len());
    while ordered.len() < roles.len() {
        let next = roles.iter().find(|role| {
            !ordered.iter().any(|done| done.role == role.role)
                && role
                    .after
                    .iter()
                    .all(|after| ordered.iter().any(|done| &done.role == after))
        });
        match next {
            Some(role) => ordered.push(role),
            None => {
                let waiting: Vec<&str> = roles
                    .iter()
                    .filter(|role| !ordered.iter().any(|done| done.role == role.role))
                    .map(|role| role.role.as_str())
                    .collect();
                return Err(format!(
                    "server roles {} wait for each other in a cycle",
                    waiting.join(", ")
                ));
            }
        }
    }
    Ok(ordered)
}

/// The control server's probes need its token and it reports no address;
/// the launch workflow's WebUI and a declared command may report the
/// address they open, and a declared command is otherwise probed on its
/// port or its output.
fn check_start_and_readiness(role: &ServerRoleDescriptor) -> Result<(), String> {
    let paired = match role.readiness {
        ServerReadiness::Control => role.start == ServerStart::ControlServer,
        ServerReadiness::Captured { .. } => role.start != ServerStart::ControlServer,
        ServerReadiness::Http { .. }
        | ServerReadiness::Tcp { .. }
        | ServerReadiness::Stdout { .. } => true,
    };
    if let ServerStart::Command { argv, .. } = &role.start {
        if argv.first().is_none_or(|program| program.trim().is_empty()) {
            return Err(format!(
                "server role `{}`: its command names no program",
                role.role
            ));
        }
    }
    if paired {
        Ok(())
    } else {
        Err(format!(
            "server role `{}`: a {:?} start cannot be probed by {:?}",
            role.role, role.start, role.readiness
        ))
    }
}

#[cfg(test)]
mod tests;
