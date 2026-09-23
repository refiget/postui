use super::{App, EditorOutcome, EditorRequest};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(crate) struct ErrorPage {
    pub(crate) message: String,
    pub(crate) path: PathBuf,
    pub(crate) editor_error: Option<String>,
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

    /// 用外部编辑器打开配置错误页指向的文件。
    pub(crate) fn request_error_editor(&mut self) {
        let Some(path) = self.error_page.as_ref().map(|page| page.path.clone()) else {
            return;
        };
        self.editor_request = Some(EditorRequest {
            path,
            outcome: EditorOutcome::ErrorPage,
        });
    }
}
