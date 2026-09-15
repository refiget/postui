use super::App;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(crate) struct ErrorPage {
    pub(crate) message: String,
    pub(crate) path: PathBuf,
    pub(crate) editor_error: Option<String>,
    editor_requested: bool,
}

impl ErrorPage {
    pub(crate) fn from_diagnostics(
        diagnostics: &[crate::diagnostics::ConfigDiagnostic],
    ) -> Option<Self> {
        let first = diagnostics.first()?;
        let mut message = format!("Configuration errors: {}", diagnostics.len());
        for diagnostic in diagnostics {
            message.push_str("\n\n");
            message.push_str(&diagnostic.to_string());
        }
        Some(Self {
            message,
            path: first.path().to_path_buf(),
            editor_error: None,
            editor_requested: false,
        })
    }

    pub(crate) fn append_diagnostics(
        &mut self,
        diagnostics: &[crate::diagnostics::ConfigDiagnostic],
    ) {
        for diagnostic in diagnostics {
            self.message.push_str("\n\n");
            self.message.push_str(&diagnostic.to_string());
        }
    }

    pub(crate) fn from_error(error: &anyhow::Error, fallback_path: PathBuf) -> Self {
        if let Some(diagnostic) = crate::diagnostics::from_error(error) {
            let path = if diagnostic.field() == "workspace" || diagnostic.path().is_dir() {
                fallback_path
            } else {
                diagnostic.path().to_path_buf()
            };
            return Self {
                message: diagnostic.to_string(),
                path,
                editor_error: None,
                editor_requested: false,
            };
        }

        let message =
            crate::diagnostics::invalid(&fallback_path, "configuration", format!("{error:#}"))
                .to_string();
        Self::from_message(fallback_path, message)
    }

    pub(crate) fn from_message(path: PathBuf, message: String) -> Self {
        Self {
            message,
            path,
            editor_error: None,
            editor_requested: false,
        }
    }
}

impl App {
    pub(crate) fn error_page(&self) -> Option<&ErrorPage> {
        self.error_page.as_ref()
    }

    pub(crate) fn dismiss_error_page(&mut self) {
        self.error_page = None;
        tracing::debug!("关闭配置错误页面，继续使用已加载配置");
    }

    pub(crate) fn request_error_editor(&mut self) {
        let Some(error_page) = self.error_page.as_mut() else {
            return;
        };
        error_page.editor_requested = true;
    }

    pub(crate) fn take_editor_request(&mut self) -> Option<PathBuf> {
        let page = self.error_page.as_mut()?;
        std::mem::take(&mut page.editor_requested).then(|| page.path.clone())
    }

    pub(crate) fn report_editor_error(&mut self, error: impl Into<String>) {
        if let Some(error_page) = self.error_page.as_mut() {
            error_page.editor_error = Some(error.into());
        }
    }
}
