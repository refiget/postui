use super::{App, Feedback, FileValueEditor, Focus, RequestStatus};
use crate::{
    config::{ApiRequest, DataPart},
    editor::{
        BodyValueEditor, EditAction, EditInput, convert_json_scalar, json_scalar_at,
        merge_json_edit, terminal_width, text_position,
    },
    template,
};
use crossterm::event::KeyEvent;

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
        let Some(request) = self.current_request() else {
            return;
        };
        if self.request_status(&request.id) == RequestStatus::Sending {
            return;
        }
        const METHODS: [&str; 2] = ["GET", "POST"];
        let current = self
            .current_effective_request()
            .map(|request| request.method)
            .unwrap_or_else(|| request.method.clone());
        let index = METHODS
            .iter()
            .position(|method| *method == current.as_str())
            .unwrap_or(0);
        if let Some(request) = self.workspace_state.current_mut() {
            request.draft.method = METHODS[(index + 1) % METHODS.len()].to_string();
        }
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
            return request
                .body_parts
                .iter()
                .map(template::data_part_text)
                .collect::<Vec<_>>()
                .join("\n");
        }

        let body = request
            .body_parts
            .iter()
            .map(template::data_part_text)
            .collect::<Vec<_>>()
            .join("&");
        if body.len() > 64 * 1024 {
            return body;
        }
        serde_json::from_str::<serde_json::Value>(&body).map_or(body, |value| {
            serde_json::to_string_pretty(&value).expect("JSON 请求体应可序列化")
        })
    }

    pub(crate) fn start_body_edit(&mut self, line: usize, column: usize) {
        self.start_body_edit_at(line, column, false);
    }

    pub(crate) fn start_body_edit_at(&mut self, line: usize, column: usize, place_cursor: bool) {
        if place_cursor {
            if let Some(editor) = self.view.preview.file_editor.as_mut()
                && editor.line == line
            {
                editor
                    .input
                    .place_cursor(column.saturating_sub(editor.column));
                return;
            }
            if let Some(editor) = self.view.preview.editor.as_mut() {
                let (editor_line, editor_column) = editor.position();
                let offset = text_position(&editor.document, line, column);
                if editor_line == line && editor.span.contains(&offset) {
                    editor
                        .input
                        .place_cursor(column.saturating_sub(editor_column));
                    return;
                }
            }
            self.view.preview.cancel_editor();
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
            self.start_file_edit(line, column, place_cursor);
            return;
        }
        let body = request
            .body_parts
            .iter()
            .map(template::data_part_text)
            .collect::<Vec<_>>()
            .join("&");
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
        self.view.preview.editor = Some(BodyValueEditor {
            document,
            span,
            kind,
            input: EditInput::new(input),
        });
        self.view.focus = Focus::Preview;
    }

    pub(crate) fn body_editor(&self) -> Option<&BodyValueEditor> {
        self.view.preview.editor.as_ref()
    }

    pub(crate) fn file_editor(&self) -> Option<&FileValueEditor> {
        self.view.preview.file_editor.as_ref()
    }

    fn start_file_edit(&mut self, line: usize, column: usize, place_cursor: bool) {
        let Some(request) = self.current_effective_request() else {
            return;
        };
        let mut file_line = if request.form.is_empty() {
            1
        } else {
            request.form.len().saturating_add(3)
        };
        let Some((file_index, file)) = request
            .files
            .iter()
            .enumerate()
            .find(|(index, _)| file_line.saturating_add(*index) == line)
        else {
            return;
        };
        file_line = file_line.saturating_add(file_index);
        let value_column = terminal_width(&file.field).saturating_add(2);
        let value_columns =
            value_column..value_column.saturating_add(terminal_width(&file.path).max(1));
        if !value_columns.contains(&column) {
            return;
        }
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            return;
        };
        let Some(configured_path) = self
            .request_draft(&request_id)
            .and_then(|draft| draft.files.get(file_index))
            .map(|file| file.path.clone())
        else {
            return;
        };
        let mut input = EditInput::new(configured_path);
        if place_cursor {
            input.place_cursor(column.saturating_sub(value_column));
        }
        self.view.preview.file_editor = Some(FileValueEditor {
            file_index,
            line: file_line,
            column: value_column,
            input,
        });
        self.view.focus = Focus::Preview;
    }

    pub(super) fn handle_body_editor_key(&mut self, key: KeyEvent) {
        if self.view.preview.file_editor.is_some() {
            self.handle_file_editor_key(key);
            return;
        }
        let action = self
            .view
            .preview
            .editor
            .as_mut()
            .map(|editor| editor.input.handle_key(key));
        match action {
            Some(EditAction::Confirm) => self.commit_body_value(),
            Some(EditAction::Cancel) => self.view.preview.editor = None,
            Some(EditAction::Continue) | None => {}
        }
    }

    fn handle_file_editor_key(&mut self, key: KeyEvent) {
        let action = self
            .view
            .preview
            .file_editor
            .as_mut()
            .map(|editor| editor.input.handle_key(key));
        match action {
            Some(EditAction::Confirm) => self.commit_file_value(),
            Some(EditAction::Cancel) => self.view.preview.file_editor = None,
            Some(EditAction::Continue) | None => {}
        }
    }

    fn commit_file_value(&mut self) {
        let Some(editor) = self.view.preview.file_editor.take() else {
            return;
        };
        let Some(session) = self.workspace_state.current_mut() else {
            return;
        };
        let Some(file) = session.draft.files.get_mut(editor.file_index) else {
            return;
        };
        let path = editor.input.value().trim();
        if !path.is_empty() && file.path != path {
            file.path = path.to_string();
            self.view.notice = None;
        }
    }

    fn commit_body_value(&mut self) {
        let Some(editor) = self.view.preview.editor.take() else {
            return;
        };
        let Some(replacement) = convert_json_scalar(editor.kind, editor.input.value()) else {
            self.view.notice = Some(Feedback::Warning(
                self.text().invalid_body_value().to_string(),
            ));
            return;
        };
        if editor.document.get(editor.span.clone()) == Some(replacement.as_str()) {
            return;
        }
        let rendered_document = editor.document;
        let mut document = rendered_document.clone();
        document.replace_range(editor.span, &replacement);
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            return;
        };
        let source_document = self
            .request_draft(&request_id)
            .map(|draft| {
                draft
                    .body_parts
                    .iter()
                    .map(template::data_part_text)
                    .collect::<Vec<_>>()
                    .join("&")
            })
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
