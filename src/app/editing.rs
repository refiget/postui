use super::{
    App, ContentEditor, ContentFieldSource, ContentTarget, Feedback, Focus, RequestStatus,
};
use crate::http_method::STANDARD_METHODS;
use crate::{
    config::{ApiRequest, DataPart},
    editor::{
        EditAction, EditInput, JsonScalarKind, convert_json_scalar, cursor_row_column,
        json_scalar_at, merge_json_edit, text_position,
    },
    template,
};
use crossterm::event::KeyEvent;
use std::ops::Range;

impl App {
    pub(crate) fn display_url(&self, request: &ApiRequest) -> String {
        match self.request_draft(&request.id) {
            Some(draft) => template::append_display_query(
                draft.url.as_deref().unwrap_or(&request.url),
                &draft.query_parts,
            ),
            None => template::display_url(request),
        }
    }

    pub(crate) fn cycle_method(&mut self) {
        let Some(request) = self.workspace_state.current_mut() else {
            return;
        };
        if request.status() == RequestStatus::Sending {
            return;
        }
        let next = STANDARD_METHODS
            .iter()
            .position(|method| method.as_str() == request.draft.method)
            .map_or(0, |index| (index + 1) % STANDARD_METHODS.len());
        request.draft.method = STANDARD_METHODS[next].to_string();
        self.register_request_change();
    }

    pub(crate) fn body_preview(&self) -> String {
        let Some(request) = self.current_effective_request() else {
            return "{}".to_string();
        };
        if request.body_parts.is_empty() {
            return "{}".to_string();
        }
        if request
            .body_parts
            .iter()
            .all(|part| matches!(part, DataPart::UrlEncoded(_)))
        {
            return template::body_parts_text(&request.body_parts, "\n");
        }

        let body = template::body_parts_text(&request.body_parts, "&");
        if body.len() > 64 * 1024 {
            return body;
        }
        serde_json::from_str::<serde_json::Value>(&body).map_or(body, |value| {
            serde_json::to_string_pretty(&value).expect("JSON request body must be serializable")
        })
    }

    /// 按渲染行号和列号定位内容页签的字段并开始编辑；`place_cursor` 为真时把光标落在点击的列上。
    pub(crate) fn start_content_edit_at(&mut self, line: usize, column: usize, place_cursor: bool) {
        if place_cursor && self.place_content_editor_cursor(line, column) {
            return;
        }
        let Some(request) = self.current_request() else {
            return;
        };
        if self.request_status(&request.id) == RequestStatus::Sending
            || self.view.preview.is_editing()
        {
            return;
        }
        let Some(request) = self.current_effective_request() else {
            return;
        };
        if request.body_parts.is_empty() {
            self.start_content_field_edit(line, column, place_cursor);
            return;
        }
        let body = template::body_parts_text(&request.body_parts, "&");
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
            return;
        };
        let Ok(document) = serde_json::to_string_pretty(&value) else {
            return;
        };
        let offset = text_position(&document, line, column);
        let Some((span, kind, input)) = json_scalar_at(&document, offset) else {
            return;
        };
        let (line, column) = cursor_row_column(&document, span.start);
        self.view.preview.editor = Some(ContentEditor {
            target: ContentTarget::Body {
                document,
                span,
                kind,
            },
            line,
            column,
            input: EditInput::new(input),
        });
        self.view.focus = Focus::Preview;
    }

    /// 点击落在编辑器的值范围内时把光标移到该列，返回是否处理。
    pub(crate) fn place_content_editor_cursor(&mut self, line: usize, column: usize) -> bool {
        let Some(editor) = self.view.preview.editor.as_mut() else {
            return false;
        };
        if !editor.covers(line, column) {
            return false;
        }
        editor.input.place_cursor(column - editor.column);
        true
    }

    /// 内容页签里正在编辑的对象；没有编辑时为 None。
    pub(crate) fn content_editor(&self) -> Option<&ContentEditor> {
        self.view.preview.editor.as_ref()
    }

    /// 内容页签渲染的请求体文本；无请求体时为 None。
    pub(crate) fn body_document(&self) -> Option<String> {
        let request = self.current_effective_request()?;
        let document = self
            .content_editor()
            .and_then(ContentEditor::display_document);
        if request.body_parts.is_empty() && document.is_none() {
            return None;
        }
        Some(document.unwrap_or_else(|| self.body_preview()))
    }

    /// 按渲染行号和列号定位表单字段或文件路径，并开始编辑。
    fn start_content_field_edit(&mut self, line: usize, column: usize, place_cursor: bool) {
        let Some(field) = self
            .content_fields()
            .into_iter()
            .find(|field| field.line == line && column >= field.column)
        else {
            return;
        };
        let Some((target, value)) = self.content_field_target(field.source) else {
            return;
        };
        let mut input = EditInput::new(value);
        if place_cursor {
            input.place_cursor(column.saturating_sub(field.column));
        }
        self.view.preview.editor = Some(ContentEditor {
            target,
            line: field.line,
            column: field.column,
            input,
        });
        self.view.focus = Focus::Preview;
    }

    /// 表单字段或文件路径的编辑对象和当前值；请求体 JSON 的值按渲染位置重新定位。
    fn content_field_target(&self, source: ContentFieldSource) -> Option<(ContentTarget, String)> {
        let session = self.workspace_state.current()?;
        match source {
            ContentFieldSource::Body => None,
            ContentFieldSource::Form(index) => Some((
                ContentTarget::Form(index),
                session.draft.form.get(index)?.value.clone(),
            )),
            ContentFieldSource::File(index) => Some((
                ContentTarget::File(index),
                session.draft.files.get(index)?.path.clone(),
            )),
        }
    }

    pub(super) fn handle_content_editor_key(&mut self, key: KeyEvent) {
        let action = self
            .view
            .preview
            .editor
            .as_mut()
            .map(|editor| editor.input.handle_key(key));
        match action {
            Some(EditAction::Confirm) => self.commit_content_edit(),
            Some(EditAction::Cancel) => self.view.preview.editor = None,
            Some(EditAction::Continue) | None => {}
        }
    }

    fn commit_content_edit(&mut self) {
        let Some(editor) = self.view.preview.editor.take() else {
            return;
        };
        let value = editor.input.confirmed_value();
        match editor.target {
            ContentTarget::Body {
                document,
                span,
                kind,
            } => self.commit_body_value(document, span, kind, &value),
            ContentTarget::Form(index) => {
                if self.apply_form_value(index, &value) {
                    self.register_request_change();
                }
            }
            ContentTarget::File(index) => {
                if self.apply_file_value(index, &value) {
                    self.view.notice = None;
                }
            }
        }
    }

    fn apply_form_value(&mut self, index: usize, value: &str) -> bool {
        let Some(field) = self
            .workspace_state
            .current_mut()
            .and_then(|session| session.draft.form.get_mut(index))
        else {
            return false;
        };
        if field.value == value {
            return false;
        }
        field.value = value.to_string();
        true
    }

    fn apply_file_value(&mut self, index: usize, value: &str) -> bool {
        let path = value.trim();
        let Some(file) = self
            .workspace_state
            .current_mut()
            .and_then(|session| session.draft.files.get_mut(index))
        else {
            return false;
        };
        if path.is_empty() || file.path == path {
            return false;
        }
        file.path = path.to_string();
        true
    }

    fn commit_body_value(
        &mut self,
        rendered_document: String,
        span: Range<usize>,
        kind: JsonScalarKind,
        value: &str,
    ) {
        let Some(replacement) = convert_json_scalar(kind, value) else {
            self.view.notice = Some(Feedback::Warning(
                self.text().invalid_body_value().to_string(),
            ));
            return;
        };
        if rendered_document.get(span.clone()) == Some(replacement.as_str()) {
            return;
        }
        let mut document = rendered_document.clone();
        document.replace_range(span, &replacement);
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            return;
        };
        let source_document = self
            .request_draft(&request_id)
            .map(|draft| template::body_parts_text(&draft.body_parts, "&"))
            .unwrap_or_default();
        let document =
            merge_json_edit(&source_document, &rendered_document, &document).unwrap_or(document);
        let next_body = vec![DataPart::Raw(document)];
        let changed = match self.request_draft_mut(&request_id) {
            Some(draft) if draft.body_parts != next_body => {
                draft.body_parts = next_body;
                true
            }
            _ => false,
        };
        if changed {
            self.register_request_change();
        }
    }
}
