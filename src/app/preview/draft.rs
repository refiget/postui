use super::App;
use crate::{
    app::{
        DataPart, DataPartSource, Dialog, HeaderRow, HeaderSource, HeadersDialog, KeyValueField,
        ParamSource, ParamsDialog, ParamsDialogRow, PreviewTab, RequestParam,
    },
    template,
};

impl App {
    pub(crate) fn preview_dialog(&self, tab: PreviewTab) -> Option<Dialog> {
        let request = self.current_request()?;
        let request_id = request.id.clone();
        let draft = self.request_draft(&request_id)?;
        match tab {
            PreviewTab::Body => None,
            PreviewTab::Headers => {
                let request_rows = &draft.headers;
                let configuration = self
                    .config
                    .configurations
                    .get(self.active_configuration())?;
                let mut rows = draft
                    .inherited_headers(&self.config, configuration)
                    .map(|header| HeaderRow {
                        name: header.name.clone(),
                        value: header.value.clone(),
                        enabled: true,
                        source: HeaderSource::Collection,
                    })
                    .collect::<Vec<_>>();
                rows.extend(
                    request_rows
                        .iter()
                        .filter(|row| row.source != HeaderSource::Suppressed)
                        .cloned(),
                );
                Some(Dialog::Headers(HeadersDialog {
                    request_id,
                    rows,
                    selected: 0,
                    scroll: Default::default(),
                    field: KeyValueField::Value,
                    editor: None,
                }))
            }
            PreviewTab::Params => {
                let mut rows = Vec::new();
                let effective_url = draft.url.as_deref().unwrap_or(request.url.as_str());
                let url_parts = template::split_url_query(effective_url);
                for parameter in template::parse_query_params(&url_parts.query) {
                    rows.push(ParamsDialogRow {
                        source: ParamSource::Url,
                        key: parameter.name,
                        value: parameter.value,
                        part_type: None,
                        has_equals: parameter.has_equals,
                    });
                }
                for part in &draft.query_parts {
                    let (part_type, parameter) = match part {
                        DataPart::Raw(part) => (DataPartSource::Raw, RequestParam::from_text(part)),
                        DataPart::UrlEncoded(parameter) => {
                            (DataPartSource::UrlEncoded, parameter.clone())
                        }
                    };
                    rows.push(ParamsDialogRow {
                        source: ParamSource::Query,
                        key: parameter.name,
                        value: parameter.value,
                        part_type: Some(part_type),
                        has_equals: parameter.has_equals,
                    });
                }
                for field in &draft.form {
                    rows.push(ParamsDialogRow {
                        source: ParamSource::Form,
                        key: field.name.clone(),
                        value: field.value.clone(),
                        part_type: None,
                        has_equals: true,
                    });
                }
                if draft
                    .body_parts
                    .iter()
                    .all(|part| matches!(part, DataPart::UrlEncoded(_)))
                {
                    for part in &draft.body_parts {
                        let DataPart::UrlEncoded(parameter) = part else {
                            continue;
                        };
                        rows.push(ParamsDialogRow {
                            source: ParamSource::Body,
                            key: parameter.name.clone(),
                            value: parameter.value.clone(),
                            part_type: Some(DataPartSource::UrlEncoded),
                            has_equals: parameter.has_equals,
                        });
                    }
                }

                Some(Dialog::Params(ParamsDialog {
                    request_id,
                    rows,
                    selected: 0,
                    scroll: Default::default(),
                    field: KeyValueField::Name,
                    editor: None,
                }))
            }
        }
    }

    pub(in crate::app) fn sync_dialog_draft(&mut self) -> bool {
        let Some(dialog_state) = self.view.dialog.take() else {
            return false;
        };
        let changed = match &dialog_state {
            Dialog::Headers(dialog) => self.sync_header_dialog(dialog),
            Dialog::Params(dialog) => self.sync_params_dialog(dialog),
            Dialog::Configurations(_) => false,
        };
        self.view.dialog = Some(dialog_state);
        changed
    }

    fn sync_header_dialog(&mut self, dialog: &HeadersDialog) -> bool {
        let mut rows = dialog
            .rows
            .iter()
            .filter(|row| row.source == HeaderSource::Request && !row.name.trim().is_empty())
            .map(|row| HeaderRow {
                name: row.name.trim().to_string(),
                ..row.clone()
            })
            .collect::<Vec<_>>();
        if let Some(session) = self.workspace_state.request(&dialog.request_id) {
            let suppressed = session
                .draft
                .headers
                .iter()
                .filter(|row| {
                    row.source == HeaderSource::Suppressed
                        && !rows
                            .iter()
                            .any(|visible| visible.name.eq_ignore_ascii_case(&row.name))
                })
                .cloned()
                .collect::<Vec<_>>();
            rows.extend(suppressed);
        }
        match self.workspace_state.request_mut(&dialog.request_id) {
            Some(session) if session.draft.headers != rows => {
                session.draft.headers = rows;
                true
            }
            _ => false,
        }
    }

    fn sync_params_dialog(&mut self, dialog: &ParamsDialog) -> bool {
        let Some(session) = self.workspace_state.request_mut(&dialog.request_id) else {
            return false;
        };
        let draft = &mut session.draft;
        let effective_url = draft.url.as_deref().unwrap_or(&session.source.url);
        let url_location = template::split_url_query(effective_url);
        let mut url_parts = Vec::new();
        let mut query_parts = Vec::new();
        let mut form = Vec::new();
        let mut body_parts = Vec::new();
        for row in &dialog.rows {
            let key = row.key.trim();
            let value = row.value.trim();
            match row.source {
                ParamSource::Url if !key.is_empty() || !value.is_empty() => {
                    url_parts.push(RequestParam::new(
                        key.to_string(),
                        value.to_string(),
                        row.has_equals,
                    ));
                }
                ParamSource::Query if !key.is_empty() || !value.is_empty() => {
                    query_parts.push(row.part_type.unwrap_or(DataPartSource::Raw).to_part(
                        RequestParam::new(key.to_string(), row.value.clone(), row.has_equals),
                    ));
                }
                ParamSource::Form if !key.is_empty() => {
                    form.push(RequestParam::new(key.to_string(), row.value.clone(), true));
                }
                ParamSource::Body if !key.is_empty() || !value.is_empty() => {
                    body_parts.push(row.part_type.unwrap_or(DataPartSource::UrlEncoded).to_part(
                        RequestParam::new(key.to_string(), row.value.clone(), row.has_equals),
                    ));
                }
                _ => {}
            }
        }

        let url = Some(template::rebuild_url(
            &url_location.base,
            &url_parts,
            &url_location.fragment,
        ));
        let replace_body = !body_parts.is_empty()
            || draft
                .body_parts
                .iter()
                .all(|part| matches!(part, DataPart::UrlEncoded(_)));
        let changed = draft.url != url
            || draft.query_parts != query_parts
            || draft.form != form
            || (replace_body && draft.body_parts != body_parts);
        if changed {
            draft.url = url;
            draft.query_parts = query_parts;
            draft.form = form;
            if replace_body {
                draft.body_parts = body_parts;
            }
        }
        changed
    }
}
