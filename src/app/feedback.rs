#[derive(Debug, Clone)]
pub(crate) enum Feedback {
    Success(String),
    Warning(String),
    Error(String),
}

impl Feedback {
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Success(message) | Self::Warning(message) | Self::Error(message) => message,
        }
    }
}
