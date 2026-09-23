use super::{
    ApiRequest, App, ContentField, ContentFieldSource, DataPart, Dialog, DialogAction, Feedback,
    Focus, HeaderRow, HeaderSource, KeyValueField, PreviewAction, PreviewTab, RequestStatus,
};
use crate::{editor, http_method, template};
use crossterm::event::KeyEvent;

mod draft;

/// 内容页签里字段名与值之间的列间隔。
const VALUE_GAP: usize = 2;

impl App {
    /// 内容页签里可编辑字段的位置；不是内容页签或没有可编辑对象时为空。
    pub(crate) fn content_fields(&self) -> Vec<ContentField> {
        if self.view.preview.active_tab != PreviewTab::Body {
            return Vec::new();
        }
        let Some(request) = self.current_effective_request() else {
            return Vec::new();
        };
        if request.body_parts.is_empty() {
            return content_value_fields(&request);
        }
        self.body_document()
            .map_or_else(Vec::new, |document| json_fields(&document))
    }

    pub(crate) fn selected_content_field(&self) -> Option<ContentField> {
        let fields = self.content_fields();
        let last = fields.len().checked_sub(1)?;
        fields
            .get(self.view.preview.field_cursor.min(last))
            .cloned()
    }

    pub(crate) fn move_content_field(&mut self, direction: isize) -> bool {
        let fields = self.content_fields();
        let Some(last) = fields.len().checked_sub(1) else {
            return false;
        };
        let current = self.view.preview.field_cursor.min(last);
        let next = match direction {
            value if value < 0 => current.saturating_sub(1),
            value if value > 0 => (current + 1).min(last),
            _ => current,
        };
        self.view.preview.field_cursor = next;
        self.view.preview.scroll.reveal(fields[next].line);
        true
    }

    /// 把内容页签的光标移到 `line` 行；该行没有字段时保持不变。
    pub(crate) fn select_content_field_at(&mut self, line: usize) {
        let Some(index) = self
            .content_fields()
            .iter()
            .position(|field| field.line == line)
        else {
            return;
        };
        self.view.preview.field_cursor = index;
    }

    pub(crate) fn current_header_count(&self) -> usize {
        let Some(session) = self.workspace_state.current() else {
            return 0;
        };
        let Some(configuration) = self.config.configurations.get(self.active_configuration())
        else {
            return 0;
        };
        session
            .draft
            .inherited_headers(&self.config, configuration)
            .count()
            + session
                .draft
                .headers
                .iter()
                .filter(|row| row.enabled && !row.name.trim().is_empty())
                .count()
    }

    pub(crate) fn current_param_count(&self) -> usize {
        let Some(session) = self.workspace_state.current() else {
            return 0;
        };
        let draft = &session.draft;
        let url_parts =
            template::split_url_query(draft.url.as_deref().unwrap_or(&session.source.url));
        let url_count = template::parse_query_params(&url_parts.query).len();
        let body_count = draft
            .body_parts
            .iter()
            .filter(|part| matches!(part, DataPart::UrlEncoded(_)))
            .count();
        url_count + draft.query_parts.len() + draft.form.len() + body_count
    }

    /// 打开参数或请求头表格；请求执行中时只提示。
    pub(crate) fn open_inline_table(&mut self, tab: PreviewTab) {
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            return;
        };
        if self.request_status(&request_id) == RequestStatus::Sending {
            tracing::debug!(tab = ?tab, "请求执行中，忽略打开表格窗口");
            self.view.notice = Some(Feedback::Warning(
                self.text().request_in_progress().to_string(),
            ));
            return;
        }
        self.view.preview.active_tab = tab;
        self.view.dialog = self.preview_dialog(tab);
        self.view.focus = Focus::Preview;
        tracing::debug!(tab = ?tab, "打开请求表格窗口");
    }

    pub(crate) fn handle_preview_action(&mut self, action: PreviewAction) {
        match action {
            PreviewAction::Send => self.send_current_request(),
            PreviewAction::Edit(tab) if self.editing_preview_tab() == Some(tab) => {
                if let Some(dialog) = self.view.dialog.as_mut() {
                    dialog.cancel_editor();
                }
                self.apply_dialog();
            }
            PreviewAction::Edit(PreviewTab::Body) => {
                if self.view.preview.is_editing() {
                    self.view.preview.cancel_editor();
                } else if let Some(field) = self.selected_content_field() {
                    self.start_content_edit_at(field.line, field.column, false);
                }
            }
            PreviewAction::Edit(tab) => self.open_inline_table(tab),
        }
    }

    pub(crate) fn editing_preview_tab(&self) -> Option<PreviewTab> {
        self.view.dialog.as_ref().and_then(Dialog::preview_tab)
    }

    pub(crate) fn can_execute_preview_action(&self, action: PreviewAction) -> bool {
        match action {
            PreviewAction::Send => self.workspace_state.current().is_some_and(|session| {
                !session
                    .draft
                    .url
                    .as_deref()
                    .unwrap_or(&session.source.url)
                    .trim()
                    .is_empty()
                    && http_method::parse(&session.draft.method).is_ok()
                    && !matches!(self.view.dialog, Some(Dialog::Configurations(_)))
                    && !self.view.preview.is_editing()
            }),
            PreviewAction::Edit(tab) => self.current_request().is_some_and(|request| {
                self.editing_preview_tab()
                    .is_none_or(|editing_tab| editing_tab == tab)
                    && self.request_status(&request.id) != RequestStatus::Sending
            }),
        }
    }

    pub(crate) fn close_dialog(&mut self) {
        if self.view.dialog.take().is_some() {
            tracing::debug!("关闭配置编辑窗口");
        }
    }

    pub(crate) fn apply_dialog(&mut self) {
        if self.view.dialog.is_none() {
            return;
        }
        let changed = self.sync_dialog_draft();

        let Some(dialog) = self.view.dialog.take() else {
            return;
        };
        match dialog {
            Dialog::Configurations(dialog) => {
                if let Some(configuration) = dialog.rows.get(dialog.state.active()).cloned() {
                    self.switch_configuration(&configuration);
                }
            }
            Dialog::Headers(dialog) => {
                if changed {
                    self.register_request_change();
                }
                let header_count = self
                    .request_draft(&dialog.request_id)
                    .map(|draft| draft.headers.iter())
                    .into_iter()
                    .flatten()
                    .filter(|row| row.enabled)
                    .count();
                tracing::debug!(header_count, "应用请求 Header 修改");
            }
            Dialog::Params(dialog) => {
                if changed {
                    self.register_request_change();
                }
                let (query_part_count, form_field_count) = {
                    self.request_draft(&dialog.request_id)
                        .map(|draft| (draft.query_parts.len(), draft.form.len()))
                        .unwrap_or_default()
                };
                tracing::debug!(query_part_count, form_field_count, "应用请求参数修改");
            }
        }
    }

    pub(crate) fn handle_dialog_key(&mut self, key: KeyEvent) {
        let Some(dialog) = self.view.dialog.as_mut() else {
            return;
        };
        let action = dialog.handle_key(key);
        match action {
            DialogAction::None => {}
            DialogAction::RemoveRow { tab, index } => self.remove_preview_row(tab, index),
            DialogAction::Changed => {
                if self.sync_dialog_draft() {
                    self.register_request_change();
                }
            }
            DialogAction::Apply => self.apply_dialog(),
            DialogAction::Cancel => {
                if self.sync_dialog_draft() {
                    self.register_request_change();
                }
                self.close_dialog();
            }
        }
    }

    pub(crate) fn click_preview_row(
        &mut self,
        tab: PreviewTab,
        index: usize,
        field: KeyValueField,
        edit: bool,
        cursor: Option<usize>,
    ) {
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.click_row(tab, index, field, edit, cursor);
        }
    }

    pub(crate) fn toggle_header_row(&mut self, index: usize) {
        if self.sync_dialog_draft() {
            self.register_request_change();
        }
        if let Some(Dialog::Headers(dialog)) = self.view.dialog.as_mut() {
            dialog.table.selected = index;
            dialog.toggle_selected();
        }
        if self.sync_dialog_draft() {
            self.register_request_change();
        }
    }

    pub(crate) fn remove_preview_row(&mut self, tab: PreviewTab, index: usize) {
        let mut changed = self.sync_dialog_draft();
        let suppressed_header = match self.view.dialog.as_mut() {
            Some(Dialog::Headers(dialog)) if tab == PreviewTab::Headers => dialog
                .remove_row(index)
                .filter(|row| !row.name.trim().is_empty())
                .map(|row| HeaderRow {
                    enabled: false,
                    source: HeaderSource::Suppressed,
                    ..row
                }),
            Some(Dialog::Params(dialog)) if tab == PreviewTab::Params => {
                dialog.remove_row(index);
                None
            }
            _ => return,
        };
        if let Some(header) = suppressed_header {
            let request_id = match self.view.dialog.as_ref() {
                Some(Dialog::Headers(dialog)) => dialog.request_id.clone(),
                _ => return,
            };
            if let Some(session) = self.workspace_state.request_mut(&request_id) {
                session
                    .draft
                    .headers
                    .retain(|row| !row.name.eq_ignore_ascii_case(&header.name));
                session.draft.headers.push(header);
                changed = true;
            }
        }
        changed |= self.sync_dialog_draft();
        if changed {
            self.register_request_change();
        }
    }

    pub(crate) fn move_preview_tab(&mut self, direction: isize) {
        if !self.has_current_request() {
            return;
        }
        let tab = match direction {
            value if value < 0 => self.view.preview.active_tab.previous(),
            value if value > 0 => self.view.preview.active_tab.next(),
            _ => self.view.preview.active_tab,
        };
        self.activate_preview_tab(tab);
        self.view.preview.reset_content();
        tracing::debug!(
            tab = ?self.view.preview.active_tab,
            direction,
            "切换请求预览标签"
        );
    }

    pub(crate) fn activate_preview_tab(&mut self, tab: PreviewTab) {
        if !self.has_current_request() {
            return;
        }
        if self.editing_preview_tab() == Some(tab) {
            return;
        }
        if self.editing_preview_tab().is_some() {
            if let Some(dialog) = self.view.dialog.as_mut() {
                dialog.cancel_editor();
            }
            if self.sync_dialog_draft() {
                self.register_request_change();
            }
            self.view.dialog = None;
        }
        self.view.preview.active_tab = tab;
        if tab != PreviewTab::Body {
            self.open_inline_table(tab);
        }
    }

    pub(crate) fn add_preview_row(&mut self, tab: PreviewTab) {
        self.activate_preview_tab(tab);
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.cancel_editor();
        }
        if self.sync_dialog_draft() {
            self.register_request_change();
        }
        match self.view.dialog.as_mut() {
            Some(Dialog::Headers(dialog)) if tab == PreviewTab::Headers => {
                dialog.open_preset_menu()
            }
            Some(Dialog::Params(dialog)) if tab == PreviewTab::Params => dialog.add_row(),
            _ => return,
        }
        tracing::debug!(tab = ?tab, "通过请求标签新增字段");
    }
}

/// 请求体 JSON 每个值的渲染行号与列。
fn json_fields(document: &str) -> Vec<ContentField> {
    if serde_json::from_str::<serde_json::Value>(document).is_err() {
        return Vec::new();
    }
    let mut fields = Vec::new();
    let mut line = 0;
    let mut line_start = 0;
    for span in editor::json_scalar_ranges(document) {
        while let Some(index) = document[line_start..span.start].find('\n') {
            line += 1;
            line_start += index + 1;
        }
        fields.push(ContentField {
            line,
            column: editor::terminal_width(&document[line_start..span.start]),
            source: ContentFieldSource::Body,
        });
    }
    fields
}

/// 表单字段和文件在内容页签里的行号与列，和 `request_content_lines` 的分段一致。
fn content_value_fields(request: &ApiRequest) -> Vec<ContentField> {
    let mut fields = Vec::new();
    let mut line = 0;
    if !request.form.is_empty() {
        line += 1;
        for (index, field) in request.form.iter().enumerate() {
            fields.push(ContentField {
                line,
                column: editor::terminal_width(&field.name) + VALUE_GAP,
                source: ContentFieldSource::Form(index),
            });
            line += 1;
        }
    }
    if !request.files.is_empty() {
        line += if request.form.is_empty() { 1 } else { 2 };
        for (index, file) in request.files.iter().enumerate() {
            fields.push(ContentField {
                line,
                column: editor::terminal_width(&file.field) + VALUE_GAP,
                source: ContentFieldSource::File(index),
            });
            line += 1;
        }
    }
    fields
}
