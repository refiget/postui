use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::{
    App, DataPart, DataPartSource, EditorOutcome, EditorRequest, Feedback, HeaderRow, HeaderSource,
    InlineTable, ParamSource, ParamsDialog, ParamsDialogRow, PreviewTab,
};

static TEMPORARY_EDITOR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TemporaryParamSource {
    Url,
    Query,
    Form,
    Body,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TemporaryParamEncoding {
    Raw,
    UrlEncoded,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemporaryParam {
    source: TemporaryParamSource,
    name: String,
    #[serde(default)]
    value: String,
    #[serde(default = "default_has_equals")]
    has_equals: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    encoding: Option<TemporaryParamEncoding>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TemporaryHeaderSource {
    Inherited,
    Request,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemporaryHeader {
    name: String,
    #[serde(default)]
    value: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    source: TemporaryHeaderSource,
}

const fn default_has_equals() -> bool {
    true
}

const fn default_enabled() -> bool {
    true
}

impl App {
    pub(super) fn edit_preview_draft(&mut self) {
        if self.view.preview.is_editing()
            || self.view.dialog.as_ref().is_some_and(|dialog| {
                !matches!(dialog, super::Dialog::Params(_) | super::Dialog::Headers(_))
            })
        {
            return;
        }
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            return;
        };
        if self.request_status(&request_id) == super::RequestStatus::Sending {
            self.view.notice = Some(Feedback::Warning(
                self.text().request_in_progress().to_string(),
            ));
            return;
        }

        let result = match self.view.preview.active_tab {
            PreviewTab::Body => self.prepare_body_editor(&request_id),
            PreviewTab::Params => self.prepare_params_editor(&request_id),
            PreviewTab::Headers => self.prepare_headers_editor(&request_id),
        };
        if let Err(error) = result {
            self.view.notice = Some(Feedback::Error(
                self.text().temporary_edit_failed(&format!("{error:#}")),
            ));
        }
    }

    fn prepare_body_editor(&mut self, request_id: &str) -> Result<()> {
        let Some(draft) = self.request_draft(request_id) else {
            return Ok(());
        };
        if draft.body_parts.is_empty() && !draft.form.is_empty() {
            return self.prepare_params_editor(request_id);
        }
        if !draft.body_parts.is_empty()
            && draft
                .body_parts
                .iter()
                .all(|part| matches!(part, DataPart::UrlEncoded(_)))
        {
            return self.prepare_params_editor(request_id);
        }
        if !draft.files.is_empty()
            || draft
                .body_parts
                .iter()
                .any(|part| matches!(part, DataPart::UrlEncoded(_)))
        {
            self.temporary_edit_unavailable();
            return Ok(());
        }

        let body = crate::template::body_parts_text(&draft.body_parts, "&");
        let (contents, json) = match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(value) => (
                serde_json::to_string_pretty(&value)
                    .context("Could not format the request body")?,
                true,
            ),
            Err(_) => (body, false),
        };
        let path = create_temporary_editor_file(if json { "json" } else { "txt" }, &contents)?;
        self.editor_request = Some(EditorRequest {
            path,
            outcome: EditorOutcome::TemporaryBody {
                request_id: request_id.to_string(),
                json,
            },
        });
        Ok(())
    }

    fn prepare_params_editor(&mut self, request_id: &str) -> Result<()> {
        let rows = match self.view.dialog.as_ref() {
            Some(super::Dialog::Params(dialog)) if dialog.request_id == request_id => {
                dialog.rows.clone()
            }
            _ => match self.preview_dialog(PreviewTab::Params) {
                Some(super::Dialog::Params(dialog)) => dialog.rows,
                _ => return Ok(()),
            },
        };
        let rows = rows
            .into_iter()
            .map(TemporaryParam::from)
            .collect::<Vec<_>>();
        let contents = serde_saphyr::to_string(&rows).context("Could not serialize parameters")?;
        let path = create_temporary_editor_file("yaml", &contents)?;
        self.editor_request = Some(EditorRequest {
            path,
            outcome: EditorOutcome::TemporaryParams {
                request_id: request_id.to_string(),
            },
        });
        Ok(())
    }

    fn prepare_headers_editor(&mut self, request_id: &str) -> Result<()> {
        let rows = match self.view.dialog.as_ref() {
            Some(super::Dialog::Headers(dialog)) if dialog.request_id == request_id => {
                dialog.rows.clone()
            }
            _ => match self.preview_dialog(PreviewTab::Headers) {
                Some(super::Dialog::Headers(dialog)) => dialog.rows,
                _ => return Ok(()),
            },
        };
        let rows = rows
            .into_iter()
            .map(TemporaryHeader::from)
            .collect::<Vec<_>>();
        let contents = serde_saphyr::to_string(&rows).context("Could not serialize headers")?;
        let path = create_temporary_editor_file("yaml", &contents)?;
        self.editor_request = Some(EditorRequest {
            path,
            outcome: EditorOutcome::TemporaryHeaders {
                request_id: request_id.to_string(),
            },
        });
        Ok(())
    }

    fn temporary_edit_unavailable(&mut self) {
        self.view.notice = Some(Feedback::Warning(
            self.text().temporary_edit_unavailable().to_string(),
        ));
    }

    pub(super) fn finish_temporary_editor(
        &mut self,
        request: &EditorRequest,
        editor_result: Result<()>,
    ) {
        let result = editor_result.and_then(|()| {
            let contents = fs::read_to_string(&request.path).with_context(|| {
                format!("Could not read temporary file: {}", request.path.display())
            })?;
            match &request.outcome {
                EditorOutcome::TemporaryBody { request_id, json } => {
                    self.apply_temporary_body(request_id, contents, *json)
                }
                EditorOutcome::TemporaryParams { request_id } => {
                    self.apply_temporary_params(request_id, &contents)
                }
                EditorOutcome::TemporaryHeaders { request_id } => {
                    self.apply_temporary_headers(request_id, &contents)
                }
                EditorOutcome::ErrorPage | EditorOutcome::ReloadWorkspace => Ok(()),
            }
        });
        let cleanup_result = remove_temporary_editor_file(&request.path);
        match (result, cleanup_result) {
            (Ok(()), Ok(())) => {
                self.view.notice = Some(Feedback::Success(
                    self.text().temporary_edit_applied().to_string(),
                ));
            }
            (Ok(()), Err(error)) => {
                let detail = format!("{error:#}");
                tracing::error!(error = %detail, "临时请求已更新，临时文件清理失败");
                self.view.notice = Some(Feedback::Warning(
                    self.text().temporary_cleanup_failed(&detail),
                ));
            }
            (Err(error), cleanup_result) => {
                let detail = format!("{error:#}");
                tracing::error!(error = %detail, "临时编辑未应用");
                if let Err(cleanup_error) = cleanup_result {
                    tracing::error!(error = %format!("{cleanup_error:#}"), "临时文件清理失败");
                }
                self.view.notice =
                    Some(Feedback::Error(self.text().temporary_edit_failed(&detail)));
            }
        }
    }

    fn apply_temporary_body(
        &mut self,
        request_id: &str,
        contents: String,
        json: bool,
    ) -> Result<()> {
        let body = if json {
            let value: serde_json::Value =
                serde_json::from_str(&contents).context("Invalid JSON request body")?;
            serde_json::to_string_pretty(&value).context("Could not format the request body")?
        } else {
            contents
        };
        let Some(draft) = self.request_draft_mut(request_id) else {
            bail!("Request is no longer available")
        };
        let next = if body.is_empty() {
            Vec::new()
        } else {
            vec![DataPart::Raw(body)]
        };
        if draft.body_parts != next {
            draft.body_parts = next;
            self.register_request_change();
        }
        Ok(())
    }

    fn apply_temporary_params(&mut self, request_id: &str, contents: &str) -> Result<()> {
        if self.workspace_state.request(request_id).is_none() {
            bail!("Request is no longer available")
        }
        let rows: Vec<TemporaryParam> =
            serde_saphyr::from_str(contents).context("Invalid parameter document")?;
        let rows = rows
            .into_iter()
            .map(ParamsDialogRow::try_from)
            .collect::<Result<Vec<_>>>()?;
        let dialog = ParamsDialog {
            request_id: request_id.to_string(),
            rows,
            table: InlineTable::default(),
        };
        if self.sync_params_dialog(&dialog) {
            self.register_request_change();
        }
        if let Some(super::Dialog::Params(current)) = self.view.dialog.as_mut()
            && current.request_id == request_id
        {
            let selected = current
                .table
                .selected
                .min(dialog.rows.len().saturating_sub(1));
            current.rows = dialog.rows;
            current.table.selected = selected;
            current.table.editor = None;
        }
        Ok(())
    }

    fn apply_temporary_headers(&mut self, request_id: &str, contents: &str) -> Result<()> {
        let current = match self.preview_dialog(PreviewTab::Headers) {
            Some(super::Dialog::Headers(dialog)) if dialog.request_id == request_id => dialog.rows,
            _ => bail!("Request is no longer available"),
        };
        let edited: Vec<TemporaryHeader> =
            serde_saphyr::from_str(contents).context("Invalid header document")?;
        let mut rows = Vec::with_capacity(edited.len());
        for header in edited {
            validate_header(&header)?;
            let inherited = current.iter().any(|row| {
                row.source == HeaderSource::Collection
                    && row.name.eq_ignore_ascii_case(header.name.trim())
                    && row.value == header.value
                    && row.enabled == header.enabled
            });
            rows.push(HeaderRow {
                name: header.name.trim().to_string(),
                value: header.value,
                enabled: header.enabled,
                source: if matches!(header.source, TemporaryHeaderSource::Inherited) && inherited {
                    HeaderSource::Collection
                } else {
                    HeaderSource::Request
                },
            });
        }
        for inherited in current
            .iter()
            .filter(|row| row.source == HeaderSource::Collection)
        {
            if !rows
                .iter()
                .any(|row| row.name.eq_ignore_ascii_case(&inherited.name))
            {
                rows.push(HeaderRow {
                    enabled: false,
                    source: HeaderSource::Suppressed,
                    ..inherited.clone()
                });
            }
        }
        let request_rows = rows
            .iter()
            .filter(|row| row.source != HeaderSource::Collection)
            .cloned()
            .collect::<Vec<_>>();
        let Some(draft) = self.request_draft_mut(request_id) else {
            bail!("Request is no longer available")
        };
        if draft.headers != request_rows {
            draft.headers = request_rows;
            self.register_request_change();
        }
        if let Some(super::Dialog::Headers(dialog)) = self.view.dialog.as_mut()
            && dialog.request_id == request_id
        {
            rows.retain(|row| row.source != HeaderSource::Suppressed);
            dialog.rows = rows;
            dialog.table.selected = dialog
                .table
                .selected
                .min(dialog.rows.len().saturating_sub(1));
            dialog.table.editor = None;
        }
        Ok(())
    }
}

impl From<HeaderRow> for TemporaryHeader {
    fn from(row: HeaderRow) -> Self {
        Self {
            name: row.name,
            value: row.value,
            enabled: row.enabled,
            source: if row.source == HeaderSource::Collection {
                TemporaryHeaderSource::Inherited
            } else {
                TemporaryHeaderSource::Request
            },
        }
    }
}

fn validate_header(header: &TemporaryHeader) -> Result<()> {
    use reqwest::header::{HeaderName, HeaderValue};
    let name = header.name.trim();
    if name.is_empty() {
        bail!("Header name cannot be empty")
    }
    HeaderName::from_bytes(name.as_bytes())
        .with_context(|| format!("Invalid Header name: {name}"))?;
    HeaderValue::from_str(&header.value)
        .with_context(|| format!("Invalid Header value: {name}"))?;
    if header.value.contains(['\n', '\r']) {
        bail!("Header value must not contain line breaks: {name}")
    }
    Ok(())
}

impl From<ParamsDialogRow> for TemporaryParam {
    fn from(row: ParamsDialogRow) -> Self {
        Self {
            source: match row.source {
                ParamSource::Url => TemporaryParamSource::Url,
                ParamSource::Query => TemporaryParamSource::Query,
                ParamSource::Form => TemporaryParamSource::Form,
                ParamSource::Body => TemporaryParamSource::Body,
            },
            name: row.key,
            value: row.value,
            has_equals: row.has_equals,
            encoding: row.part_type.map(|encoding| match encoding {
                DataPartSource::Raw => TemporaryParamEncoding::Raw,
                DataPartSource::UrlEncoded => TemporaryParamEncoding::UrlEncoded,
            }),
        }
    }
}

impl TryFrom<TemporaryParam> for ParamsDialogRow {
    type Error = anyhow::Error;

    fn try_from(row: TemporaryParam) -> Result<Self> {
        let source = match row.source {
            TemporaryParamSource::Url => ParamSource::Url,
            TemporaryParamSource::Query => ParamSource::Query,
            TemporaryParamSource::Form => ParamSource::Form,
            TemporaryParamSource::Body => ParamSource::Body,
        };
        let part_type = row.encoding.map(|encoding| match encoding {
            TemporaryParamEncoding::Raw => DataPartSource::Raw,
            TemporaryParamEncoding::UrlEncoded => DataPartSource::UrlEncoded,
        });
        if matches!(source, ParamSource::Url | ParamSource::Form) && part_type.is_some() {
            bail!("url and form parameters do not accept encoding")
        }
        Ok(Self {
            source,
            key: row.name,
            value: row.value,
            part_type,
            has_equals: row.has_equals,
        })
    }
}

fn create_temporary_editor_file(extension: &str, contents: &str) -> Result<PathBuf> {
    let directory = std::env::temp_dir();
    for _ in 0..100 {
        let sequence = TEMPORARY_EDITOR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            "postui-editor-{}-{sequence}.{extension}",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(contents.as_bytes()) {
                    let _ = fs::remove_file(&path);
                    return Err(error).with_context(|| {
                        format!("Could not write temporary file: {}", path.display())
                    });
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Could not create temporary file: {}", path.display())
                });
            }
        }
    }
    bail!("Could not create a unique temporary file")
}

fn remove_temporary_editor_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("Could not remove temporary file: {}", path.display())),
    }
}
