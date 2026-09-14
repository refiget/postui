use super::*;

mod draft;

impl App {
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

    pub(crate) fn open_headers(&mut self) {
        let Some(request) = self.current_request() else {
            return;
        };
        if self.request_status(&request.id) == RequestStatus::Sending {
            tracing::debug!("请求执行中，忽略打开 Header 编辑窗口");
            self.view.notice = Some(Feedback::Warning(
                self.text().request_in_progress().to_string(),
            ));
            return;
        }
        self.view.preview.active_tab = PreviewTab::Headers;
        self.view.dialog = self.preview_dialog(PreviewTab::Headers);
        self.view.focus = Focus::Preview;
        tracing::debug!(
            header_count = self.current_header_count(),
            "打开请求 Header 窗口"
        );
    }

    pub(crate) fn open_params(&mut self) {
        let Some(request_id) = self.current_request().map(|request| request.id.clone()) else {
            return;
        };
        if self.request_status(&request_id) == RequestStatus::Sending {
            tracing::debug!("请求执行中，忽略打开参数编辑窗口");
            self.view.notice = Some(Feedback::Warning(
                self.text().request_in_progress().to_string(),
            ));
            return;
        }
        self.view.preview.active_tab = PreviewTab::Params;
        self.view.dialog = self.preview_dialog(PreviewTab::Params);
        self.view.focus = Focus::Preview;
        let (query_row_count, form_field_count) = self
            .request_draft(&request_id)
            .map(|draft| (draft.query_parts.len(), draft.form.len()))
            .unwrap_or_default();
        tracing::debug!(query_row_count, form_field_count, "打开参数窗口");
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
                } else {
                    self.start_body_edit(0, 0);
                }
            }
            PreviewAction::Edit(PreviewTab::Params) => self.open_params(),
            PreviewAction::Edit(PreviewTab::Headers) => self.open_headers(),
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
                    && self.view.variables.is_none()
                    && !self.view.preview.is_editing()
            }),
            PreviewAction::Edit(tab) => self.current_request().is_some_and(|request| {
                self.editing_preview_tab()
                    .is_none_or(|editing_tab| editing_tab == tab)
                    && self.request_status(&request.id) != RequestStatus::Sending
            }),
        }
    }

    pub(crate) fn focused_preview_action(&self) -> Option<PreviewAction> {
        match self.view.focus {
            Focus::SendButton => Some(PreviewAction::Send),
            _ => None,
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
                if let Some(configuration) = dialog.rows.get(dialog.selected).cloned() {
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

    pub(crate) fn move_dialog_selection(&mut self, direction: isize) {
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.move_selection(direction);
            tracing::debug!(direction, "移动配置窗口列表选择");
        }
        if self.sync_dialog_draft() {
            self.register_request_change();
        }
    }

    pub(crate) fn click_configuration_row(&mut self, index: usize) {
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.click_configuration_row(index);
        }
    }

    pub(crate) fn click_param_row(
        &mut self,
        index: usize,
        field: KeyValueField,
        edit: bool,
        cursor: Option<usize>,
    ) {
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.click_param_row(index, field, edit, cursor);
        }
    }

    pub(crate) fn click_header_row(
        &mut self,
        index: usize,
        field: KeyValueField,
        edit: bool,
        cursor: Option<usize>,
    ) {
        if let Some(dialog) = self.view.dialog.as_mut() {
            dialog.click_header_row(index, field, edit, cursor);
        }
    }

    pub(crate) fn toggle_header_row(&mut self, index: usize) {
        if self.sync_dialog_draft() {
            self.register_request_change();
        }
        if let Some(Dialog::Headers(dialog)) = self.view.dialog.as_mut() {
            dialog.selected = index;
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
        self.view.preview.scroll.reset();
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
        match tab {
            PreviewTab::Body => {}
            PreviewTab::Params => self.open_params(),
            PreviewTab::Headers => self.open_headers(),
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
            Some(Dialog::Headers(dialog)) if tab == PreviewTab::Headers => dialog.add_row(),
            Some(Dialog::Params(dialog)) if tab == PreviewTab::Params => dialog.add_row(),
            _ => return,
        }
        self.register_request_change();
        tracing::debug!(tab = ?tab, "通过请求标签新增字段");
    }
}
