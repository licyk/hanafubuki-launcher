//! The placeholders a declarative type's strings hold: `{component.<name>}`,
//! `{param.<name>}`, `{port}`, `{instance.id}`, `{instance.root}`, and for
//! an extension's type `{extension.root}` and `{host.python}`, with `{{`
//! and `}}` for a literal brace. There is no expression language: a
//! placeholder is replaced by its value, as text.

use std::cell::OnceCell;
use std::path::{Component, Path, PathBuf};

use crate::internal::hanafubuki_webui::install::steps::ComponentPath;

use super::InstanceLocation;
use super::extension::{ExtensionContext, HostPythonPaths};
use super::parameters::DeclarativeSpec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Placeholder {
    Component(String),
    Param(String),
    Port,
    InstanceId,
    InstanceRoot,
    ExtensionRoot,
    HostPython,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    Text(String),
    Value(Placeholder),
}

/// A string with placeholders, parsed once at registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Template(Vec<Part>);

/// What a template may name where it is used.
pub(super) struct Scope<'a> {
    pub components: &'a [String],
    pub parameters: &'a [String],
    /// Only a server's command and environment know their port.
    pub port: bool,
}

/// The values one instance gives the placeholders.
pub(super) struct Values<'a> {
    location: &'a InstanceLocation,
    spec: &'a DeclarativeSpec,
    port: Option<u16>,
    extension: Option<&'a ExtensionContext>,
    /// `{host.python}`'s, asked of the runtime once, when first named.
    host_python: OnceCell<HostPythonPaths>,
    /// Ends a wait for `{host.python}`.
    abandon: &'a dyn Fn() -> bool,
}

impl<'a> Values<'a> {
    pub fn new(
        location: &'a InstanceLocation,
        spec: &'a DeclarativeSpec,
        port: Option<u16>,
        extension: Option<&'a ExtensionContext>,
        abandon: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            location,
            spec,
            port,
            extension,
            host_python: OnceCell::new(),
            abandon,
        }
    }

    pub fn location(&self) -> &InstanceLocation {
        self.location
    }

    pub fn spec(&self) -> &DeclarativeSpec {
        self.spec
    }

    pub fn host_python(&self) -> Result<&HostPythonPaths, String> {
        if let Some(paths) = self.host_python.get() {
            return Ok(paths);
        }
        let paths = self
            .extension()?
            .host_python
            .resolve(self.abandon)
            .map_err(|error| format!("`{{host.python}}` cannot be resolved: {error}"))?;
        Ok(self.host_python.get_or_init(|| paths))
    }

    fn extension(&self) -> Result<&ExtensionContext, String> {
        self.extension
            .ok_or_else(|| "the type has no extension to resolve its placeholders".to_string())
    }
}

impl Template {
    pub fn parse(text: &str, scope: &Scope<'_>) -> Result<Self, String> {
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut chars = text.chars().peekable();
        while let Some(char) = chars.next() {
            match char {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    literal.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    literal.push('}');
                }
                '{' => {
                    let mut name = String::new();
                    let mut closed = false;
                    for char in chars.by_ref() {
                        if char == '}' {
                            closed = true;
                            break;
                        }
                        name.push(char);
                    }
                    if !closed {
                        return Err(format!("`{{{name}` is not closed"));
                    }
                    if !literal.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut literal)));
                    }
                    parts.push(Part::Value(placeholder(&name, scope)?));
                }
                '}' => {
                    return Err("a `}` closes no placeholder; write `}}` for a brace".to_string());
                }
                char => literal.push(char),
            }
        }
        if !literal.is_empty() {
            parts.push(Part::Text(literal));
        }
        Ok(Self(parts))
    }

    pub fn render(&self, values: &Values<'_>) -> Result<String, String> {
        let mut rendered = String::new();
        for part in &self.0 {
            match part {
                Part::Text(text) => rendered.push_str(text),
                Part::Value(placeholder) => rendered.push_str(&value(placeholder, values)?),
            }
        }
        Ok(rendered)
    }

    pub fn names_param(&self) -> bool {
        self.0
            .iter()
            .any(|part| matches!(part, Part::Value(Placeholder::Param(_))))
    }

    /// Whether the whole text is `{host.python}`: a command's program that
    /// runs with the launcher code on its import path.
    pub fn is_host_python(&self) -> bool {
        matches!(self.0.as_slice(), [Part::Value(Placeholder::HostPython)])
    }
}

/// A path template: `{component.<name>}`, then the path inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PathTemplate {
    component: String,
    rest: Template,
}

impl PathTemplate {
    pub fn parse(text: &str, scope: &Scope<'_>) -> Result<Self, String> {
        let template = Template::parse(text, scope)?;
        let mut parts = template.0.into_iter();
        let Some(Part::Value(Placeholder::Component(component))) = parts.next() else {
            return Err(format!(
                "path `{text}` does not start with a `{{component.<name>}}`"
            ));
        };
        Ok(Self {
            component,
            rest: separated_rest(parts.collect())?,
        })
    }

    /// The location inside the component. A value that would leave it
    /// (`..`, a root) is refused here, where the values are known.
    pub fn render(&self, values: &Values<'_>) -> Result<ComponentPath, String> {
        let root = values
            .location
            .components
            .require(&self.component)?
            .to_path_buf();
        let rest = self.rest.render(values)?;
        Ok(ComponentPath {
            component: self.component.clone(),
            root,
            relative: relative_inside(&rest)?,
            external: values.location.components.is_external(&self.component),
        })
    }
}

/// A `PATH` entry of the type's `[environment]`: a path template, or
/// `{instance.root}` and the path below it. An entry is never written, so
/// it may name the instance's root, which no step writes in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PathEntryTemplate {
    Component(PathTemplate),
    InstanceRoot(Template),
}

impl PathEntryTemplate {
    pub fn parse(text: &str, scope: &Scope<'_>) -> Result<Self, String> {
        let template = Template::parse(text, scope)?;
        match template.0.split_first() {
            Some((Part::Value(Placeholder::InstanceRoot), rest)) => {
                Ok(Self::InstanceRoot(separated_rest(rest.to_vec())?))
            }
            _ => PathTemplate::parse(text, scope)
                .map(Self::Component)
                .map_err(|_| {
                    format!(
                        "path `{text}` does not start with a `{{component.<name>}}` or `{{instance.root}}`"
                    )
                }),
        }
    }

    pub fn render(&self, values: &Values<'_>) -> Result<PathBuf, String> {
        match self {
            Self::Component(path) => path.render(values).map(|path| path.path()),
            Self::InstanceRoot(rest) => {
                let relative = relative_inside(&rest.render(values)?)?;
                Ok(values.location.instance_root.join(relative))
            }
        }
    }
}

/// What follows a path's leading placeholder: nothing, or a separator and
/// the path below it.
fn separated_rest(rest: Vec<Part>) -> Result<Template, String> {
    if let Some(Part::Text(text)) = rest.first() {
        if !text.starts_with(['/', '\\']) {
            return Err(format!(
                "path `{text}` continues its placeholder's name; separate it with `/`"
            ));
        }
    }
    Ok(Template(rest))
}

fn relative_inside(rest: &str) -> Result<PathBuf, String> {
    let mut relative = PathBuf::new();
    for component in Path::new(rest.trim_start_matches(['/', '\\'])).components() {
        match component {
            Component::Normal(name) => relative.push(name),
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "`{rest}` leaves its component; a path stays inside it"
                ));
            }
        }
    }
    Ok(relative)
}

fn placeholder(name: &str, scope: &Scope<'_>) -> Result<Placeholder, String> {
    let known = |list: &[String], value: &str| list.iter().any(|item| item == value);
    match name.split_once('.') {
        Some(("component", component)) if known(scope.components, component) => {
            Ok(Placeholder::Component(component.to_string()))
        }
        Some(("component", component)) => Err(format!(
            "`{{{name}}}` names `{component}`, which is not a declared component"
        )),
        Some(("param", parameter)) if known(scope.parameters, parameter) => {
            Ok(Placeholder::Param(parameter.to_string()))
        }
        Some(("param", parameter)) => Err(format!(
            "`{{{name}}}` names `{parameter}`, which is not a declared install parameter"
        )),
        Some(("instance", "id")) => Ok(Placeholder::InstanceId),
        Some(("instance", "root")) => Ok(Placeholder::InstanceRoot),
        Some(("extension", "root")) => Ok(Placeholder::ExtensionRoot),
        Some(("host", "python")) => Ok(Placeholder::HostPython),
        None if name == "port" && scope.port => Ok(Placeholder::Port),
        None if name == "port" => Err(super::documentation::PORT_SCOPE.message.to_string()),
        _ => Err(format!("`{{{name}}}` is not a placeholder")),
    }
}

fn value(placeholder: &Placeholder, values: &Values<'_>) -> Result<String, String> {
    Ok(match placeholder {
        Placeholder::Component(name) => path_text(values.location.components.require(name)?)?,
        Placeholder::Param(name) => values.spec.text(name).unwrap_or_default(),
        Placeholder::Port => values
            .port
            .ok_or_else(|| "`{port}` has no value here".to_string())?
            .to_string(),
        Placeholder::InstanceId => values.location.instance_id.clone(),
        Placeholder::InstanceRoot => path_text(&values.location.instance_root)?,
        Placeholder::ExtensionRoot => path_text(&values.extension()?.root)?,
        Placeholder::HostPython => path_text(&values.host_python()?.python)?,
    })
}

fn path_text(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{} is not valid UTF-8", path.display()))
}
