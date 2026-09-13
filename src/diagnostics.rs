use std::{
    error::Error,
    fmt, io,
    path::{Path, PathBuf},
};

use anyhow::Result;

#[derive(Debug, Clone, Copy)]
enum ConfigDiagnosticKind {
    Read(io::ErrorKind),
    Parse,
    Invalid,
}

impl ConfigDiagnosticKind {
    const fn code(self) -> &'static str {
        match self {
            Self::Read(_) => "CONFIG_READ",
            Self::Parse => "CONFIG_PARSE",
            Self::Invalid => "CONFIG_INVALID",
        }
    }

    const fn summary(self) -> &'static str {
        match self {
            Self::Read(_) => "Could not read configuration",
            Self::Parse => "Invalid YAML",
            Self::Invalid => "Invalid configuration",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigDiagnostic {
    kind: ConfigDiagnosticKind,
    path: PathBuf,
    field: String,
    location: Option<(u64, u64)>,
    detail: String,
}

impl ConfigDiagnostic {
    pub(crate) fn read(path: &Path, error: &io::Error) -> Self {
        Self {
            kind: ConfigDiagnosticKind::Read(error.kind()),
            path: path.to_path_buf(),
            field: "configuration file".to_string(),
            location: None,
            detail: error.to_string(),
        }
    }

    pub(crate) fn parse(
        path: &Path,
        field: impl Into<String>,
        detail: impl Into<String>,
        location: Option<(u64, u64)>,
    ) -> Self {
        Self {
            kind: ConfigDiagnosticKind::Parse,
            path: path.to_path_buf(),
            field: field.into(),
            location,
            detail: detail.into(),
        }
    }

    pub(crate) fn invalid(
        path: &Path,
        field: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind: ConfigDiagnosticKind::Invalid,
            path: path.to_path_buf(),
            field: field.into(),
            location: None,
            detail: detail.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn field(&self) -> &str {
        &self.field
    }

    pub fn is_not_found(&self) -> bool {
        matches!(
            self.kind,
            ConfigDiagnosticKind::Read(io::ErrorKind::NotFound)
        )
    }
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "error[{}]: {}",
            self.kind.code(),
            self.kind.summary()
        )?;
        writeln!(formatter, "  file: {}", self.path.display())?;
        writeln!(formatter, "  field: {}", self.field)?;
        if let Some((line, column)) = self.location {
            writeln!(formatter, "  location: line {line}, column {column}")?;
        }
        write!(formatter, "  detail: {}", self.detail)
    }
}

impl Error for ConfigDiagnostic {}

pub fn invalid(path: &Path, field: impl Into<String>, detail: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ConfigDiagnostic::invalid(path, field, detail))
}

pub(crate) fn parse_yaml<'a, T: serde::Deserialize<'a>>(
    path: &Path,
    field: &str,
    text: &'a str,
) -> Result<T> {
    serde_saphyr::from_str(text).map_err(|error| {
        let location = error
            .location()
            .map(|location| (location.line(), location.column()));
        anyhow::Error::new(ConfigDiagnostic::parse(
            path,
            field,
            yaml_detail(&error.to_string()),
            location,
        ))
    })
}

pub(crate) fn read(path: &Path, error: &io::Error) -> anyhow::Error {
    anyhow::Error::new(ConfigDiagnostic::read(path, error))
}

pub fn from_error(error: &anyhow::Error) -> Option<ConfigDiagnostic> {
    error.downcast_ref::<ConfigDiagnostic>().cloned()
}

pub(crate) fn standardize<T>(
    result: Result<T>,
    path: &Path,
    field: impl Into<String>,
) -> Result<T> {
    result.map_err(|error| {
        if is_config_error(&error) {
            error
        } else {
            invalid(path, field, format!("{error:#}"))
        }
    })
}

pub(crate) fn is_config_error(error: &anyhow::Error) -> bool {
    error.is::<ConfigDiagnostic>()
}

pub(crate) fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<ConfigDiagnostic>()
        .is_some_and(ConfigDiagnostic::is_not_found)
}

fn yaml_detail(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or(text).trim();
    let detail = first_line.strip_prefix("error: ").unwrap_or(first_line);
    if let Some((prefix, message)) = detail.split_once(": ")
        && prefix.starts_with("line ")
        && prefix.contains(" column ")
    {
        return message.to_string();
    }
    detail.to_string()
}
