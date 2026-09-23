use super::App;
use crate::editor::sanitize_paste;
use crate::shortcuts::{self, Command, Context};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::sync::mpsc::{self, Receiver, TryRecvError};

enum ImportProgress {
    Parsed,
    Finished(Result<(), String>),
}

#[derive(Debug, Clone, Copy)]
enum ImportStage {
    Parsing,
    Saving,
}

struct RegisteredVariable {
    name: String,
    default: Option<Value>,
}

#[derive(Debug, Default)]
enum CurlImportState {
    #[default]
    Idle,
    Running {
        stage: ImportStage,
        receiver: Receiver<ImportProgress>,
    },
    Failed(String),
}

enum ImportPoll {
    Pending,
    Changed,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CurlImportFocus {
    #[default]
    Name,
    Description,
    RegisteredVariables,
    Workspace,
    Command,
}

impl CurlImportFocus {
    fn is_input(self) -> bool {
        matches!(
            self,
            Self::Name | Self::Description | Self::RegisteredVariables | Self::Command
        )
    }

    fn next(self, reverse: bool) -> Self {
        match (self, reverse) {
            (Self::Name, false) | (Self::Command, true) => Self::Description,
            (Self::Description, false) | (Self::Name, true) => Self::RegisteredVariables,
            (Self::RegisteredVariables, false) | (Self::Description, true) => Self::Workspace,
            (Self::Workspace, false) | (Self::RegisteredVariables, true) => Self::Command,
            (Self::Command, false) | (Self::Workspace, true) => Self::Name,
        }
    }
}

#[derive(Debug, Default)]
struct CurlImportField {
    value: String,
    cursor: usize,
}

impl CurlImportField {
    fn place_cursor(&mut self, line: usize, column: usize) {
        self.cursor = crate::editor::text_position(&self.value, line, column);
    }

    fn insert(&mut self, value: &str) {
        self.value.insert_str(self.cursor, value);
        self.cursor += value.len();
    }

    fn backspace(&mut self) {
        let Some((index, _)) = self.value[..self.cursor].char_indices().next_back() else {
            return;
        };
        self.value.drain(index..self.cursor);
        self.cursor = index;
    }

    fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    fn delete(&mut self) {
        let Some((offset, _)) = self.value[self.cursor..].char_indices().nth(1) else {
            self.value.truncate(self.cursor);
            return;
        };
        self.value.drain(self.cursor..self.cursor + offset);
    }

    fn move_left(&mut self) {
        if let Some((index, _)) = self.value[..self.cursor].char_indices().next_back() {
            self.cursor = index;
        }
    }

    fn move_right(&mut self) {
        if let Some((offset, character)) = self.value[self.cursor..].char_indices().next() {
            self.cursor += offset + character.len_utf8();
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct CurlImportPage {
    name: CurlImportField,
    description: CurlImportField,
    registered_variables: CurlImportField,
    command: CurlImportField,
    focus: CurlImportFocus,
    state: CurlImportState,
}

impl CurlImportPage {
    pub(crate) fn command(&self) -> &str {
        &self.command.value
    }

    pub(crate) fn focused(&self, focus: CurlImportFocus) -> bool {
        self.focus == focus
    }

    pub(crate) fn place_cursor(&mut self, focus: CurlImportFocus, line: usize, column: usize) {
        self.focus = focus;
        if let Some(field) = self.active_field_mut() {
            field.place_cursor(line, column);
        }
    }

    fn is_running(&self) -> bool {
        matches!(self.state, CurlImportState::Running { .. })
    }

    pub(crate) fn status(&self, text: crate::i18n::UiText) -> Option<(&str, bool)> {
        match &self.state {
            CurlImportState::Idle => None,
            CurlImportState::Running {
                stage: ImportStage::Parsing,
                ..
            } => Some((text.curl_import_parsing(), false)),
            CurlImportState::Running {
                stage: ImportStage::Saving,
                ..
            } => Some((text.curl_import_saving(), false)),
            CurlImportState::Failed(error) => Some((error, true)),
        }
    }

    /// 聚焦位置对应的输入字段值。
    pub(crate) fn field_value(&self, focus: CurlImportFocus) -> &str {
        self.field(focus).map_or("", |field| field.value.as_str())
    }

    pub(crate) fn cursor(&self, focus: CurlImportFocus) -> usize {
        self.field(focus).map_or(0, |field| field.cursor)
    }

    /// 聚焦位置对应的输入字段。
    fn field(&self, focus: CurlImportFocus) -> Option<&CurlImportField> {
        match focus {
            CurlImportFocus::Name => Some(&self.name),
            CurlImportFocus::Description => Some(&self.description),
            CurlImportFocus::RegisteredVariables => Some(&self.registered_variables),
            CurlImportFocus::Command => Some(&self.command),
            CurlImportFocus::Workspace => None,
        }
    }

    fn field_mut(&mut self, focus: CurlImportFocus) -> Option<&mut CurlImportField> {
        match focus {
            CurlImportFocus::Name => Some(&mut self.name),
            CurlImportFocus::Description => Some(&mut self.description),
            CurlImportFocus::RegisteredVariables => Some(&mut self.registered_variables),
            CurlImportFocus::Command => Some(&mut self.command),
            CurlImportFocus::Workspace => None,
        }
    }

    fn active_field_mut(&mut self) -> Option<&mut CurlImportField> {
        let focus = self.focus;
        self.field_mut(focus)
    }

    /// 对当前聚焦的输入字段执行编辑操作。
    fn edit_active_field(&mut self, edit: impl FnOnce(&mut CurlImportField)) {
        if let Some(field) = self.active_field_mut() {
            edit(field);
        }
    }

    fn import_fields(&mut self) -> Option<(String, String, String)> {
        if self.is_running() {
            return None;
        }
        if self.name.value.trim().is_empty() {
            self.focus = CurlImportFocus::Name;
            return None;
        } else if self.command.value.trim().is_empty() {
            self.focus = CurlImportFocus::Command;
            return None;
        }
        Some((
            self.name.value.trim().to_string(),
            self.description.value.trim().to_string(),
            self.command.value.clone(),
        ))
    }

    fn mark_editing(&mut self) {
        if matches!(self.state, CurlImportState::Failed(_)) {
            self.state = CurlImportState::Idle;
        }
    }

    fn poll_import(&mut self, text: crate::i18n::UiText) -> ImportPoll {
        let CurlImportState::Running { receiver, .. } = &self.state else {
            return ImportPoll::Pending;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return ImportPoll::Pending,
            Err(TryRecvError::Disconnected) => {
                self.state = CurlImportState::Failed(text.curl_import_stopped().to_string());
                self.focus = CurlImportFocus::Command;
                return ImportPoll::Changed;
            }
        };
        match result {
            ImportProgress::Parsed => {
                let CurlImportState::Running { stage, .. } = &mut self.state else {
                    unreachable!();
                };
                *stage = ImportStage::Saving;
                ImportPoll::Changed
            }
            ImportProgress::Finished(Ok(())) => ImportPoll::Complete,
            ImportProgress::Finished(Err(error)) => {
                self.state = CurlImportState::Failed(error);
                self.focus = CurlImportFocus::Command;
                ImportPoll::Changed
            }
        }
    }
}

impl App {
    pub(crate) fn poll_curl_import(&mut self) -> bool {
        let text = self.text();
        let poll = self
            .view
            .curl_import
            .as_mut()
            .map_or(ImportPoll::Pending, |page| page.poll_import(text));
        if matches!(poll, ImportPoll::Complete) {
            self.view.curl_import = None;
            self.reload_workspace();
        }
        !matches!(poll, ImportPoll::Pending)
    }

    pub(crate) fn curl_import_is_parsing(&self) -> bool {
        self.view
            .curl_import
            .as_ref()
            .is_some_and(CurlImportPage::is_running)
    }

    pub(crate) fn open_curl_import(&mut self) {
        self.view.cancel_active_editors();
        self.view.dialog = None;
        self.view.curl_import = Some(CurlImportPage::default());
    }

    fn start_curl_import(&mut self) {
        let text = self.text();
        let (name, description, command, variables) = {
            let Some(page) = self.view.curl_import.as_mut() else {
                return;
            };
            let Some((name, description, command)) = page.import_fields() else {
                return;
            };
            let variables = match parse_registered_variables(&page.registered_variables.value, text)
            {
                Ok(variables) => variables,
                Err(error) => {
                    page.state = CurlImportState::Failed(error);
                    page.focus = CurlImportFocus::RegisteredVariables;
                    return;
                }
            };
            (name, description, command, variables)
        };

        let request_files = self.request_files.clone();
        let active_configuration = self.active_configuration().to_string();
        let mut configuration = self.config.configurations[&active_configuration].clone();
        let has_registered_variables = !variables.is_empty();
        merge_registered_variables(&mut configuration.variables, variables);
        let configuration_path = configuration.path.clone().unwrap_or_else(|| {
            request_files
                .workspace_path()
                .join("scenarios")
                .join(format!("{active_configuration}.yaml"))
        });
        let (sender, receiver) = mpsc::channel();
        self.view.curl_import.as_mut().unwrap().state = CurlImportState::Running {
            stage: ImportStage::Parsing,
            receiver,
        };
        std::thread::spawn(move || {
            let imported = match crate::curl::parse(&command) {
                Ok(request) => request,
                Err(error) => {
                    let _ = sender.send(ImportProgress::Finished(Err(error.to_string())));
                    return;
                }
            };
            if sender.send(ImportProgress::Parsed).is_err() {
                return;
            }
            let document = imported_request_document(imported, name.clone(), description);
            let result = if has_registered_variables {
                let configuration_document =
                    crate::config::ConfigurationDocument::from(&configuration);
                request_files.save_configuration(&configuration_path, &configuration_document)
            } else {
                Ok(())
            }
            .and_then(|()| request_files.create(&name, &document).map(|_| ()))
            .map_err(|error| error.to_string());
            let _ = sender.send(ImportProgress::Finished(result));
        });
    }

    pub(crate) fn focus_curl_import(&mut self, focus: CurlImportFocus) {
        if let Some(page) = self.view.curl_import.as_mut() {
            page.focus = focus;
        }
    }

    pub(crate) fn place_curl_import_cursor(
        &mut self,
        focus: CurlImportFocus,
        line: usize,
        column: usize,
    ) {
        if let Some(page) = self.view.curl_import.as_mut() {
            page.place_cursor(focus, line, column);
        }
    }

    pub(super) fn handle_curl_import_key(&mut self, key: KeyEvent) {
        let key = shortcuts::normalize(key);
        match shortcuts::resolve(Context::CurlImport, key, false) {
            Some(Command::Quit) => {
                self.request_quit();
                return;
            }
            Some(Command::Help) => {
                self.view.help_scroll = Some(0);
                return;
            }
            _ => {}
        }
        if self.view.dialog.is_some() {
            self.handle_dialog_key(key);
            return;
        }
        let Some(page) = self.view.curl_import.as_mut() else {
            return;
        };
        if page.is_running() {
            return;
        }
        let command = shortcuts::resolve(Context::CurlImport, key, false);
        match command {
            Some(Command::Back) if key.code == KeyCode::Esc || !page.focus.is_input() => {
                self.view.curl_import = None;
            }
            Some(Command::Send) => self.start_curl_import(),
            Some(Command::FocusNext) => page.focus = page.focus.next(false),
            Some(Command::FocusPrevious) => page.focus = page.focus.next(true),
            Some(Command::Clear) => {
                page.mark_editing();
                page.edit_active_field(CurlImportField::clear);
            }
            _ if key.code == KeyCode::Enter && page.focus == CurlImportFocus::Workspace => {
                self.open_configurations();
            }
            _ if key.code == KeyCode::Enter && page.focus == CurlImportFocus::Command => {
                page.command.insert("\n");
            }
            _ if key.code == KeyCode::Enter
                && page.focus == CurlImportFocus::RegisteredVariables =>
            {
                page.mark_editing();
                page.registered_variables.insert("\n");
            }
            _ if key.code == KeyCode::Enter => page.focus = page.focus.next(false),
            _ if key.code == KeyCode::Backspace => {
                page.mark_editing();
                page.edit_active_field(CurlImportField::backspace);
            }
            _ if key.code == KeyCode::Delete => {
                page.mark_editing();
                page.edit_active_field(CurlImportField::delete);
            }
            _ if key.code == KeyCode::Left => page.edit_active_field(CurlImportField::move_left),
            _ if key.code == KeyCode::Right => page.edit_active_field(CurlImportField::move_right),
            _ if key.code == KeyCode::Home => page.edit_active_field(|field| field.cursor = 0),
            _ if key.code == KeyCode::End => {
                page.edit_active_field(|field| field.cursor = field.value.len());
            }
            _ if key.modifiers == KeyModifiers::NONE => {
                let KeyCode::Char(character) = key.code else {
                    return;
                };
                page.mark_editing();
                page.edit_active_field(|field| field.insert(&character.to_string()));
            }
            _ => {}
        }
    }

    pub(crate) fn handle_curl_import_paste(&mut self, value: &str) {
        let Some(page) = self.view.curl_import.as_mut() else {
            return;
        };
        if page.is_running() {
            return;
        }
        page.mark_editing();
        let multiline = matches!(
            page.focus,
            CurlImportFocus::RegisteredVariables | CurlImportFocus::Command
        );
        let (value, truncated) = sanitize_paste(value, multiline);
        page.edit_active_field(|field| field.insert(&value));
        if truncated {
            self.view.notice = Some(super::Feedback::Warning(
                self.text().paste_truncated().to_string(),
            ));
        }
    }
}

fn imported_request_document(
    request: crate::curl::ImportedRequest,
    name: String,
    description: String,
) -> crate::config::RequestDocument {
    crate::config::RequestDocument {
        name,
        description,
        method: request.method,
        url: request.url,
        timeout: request.timeout.map(|timeout| timeout.as_secs()),
        skip_ssl_verification: request.skip_ssl_verification.then_some(true),
        headers: request.headers,
        params: request
            .query_parts
            .into_iter()
            .map(|part| match part {
                crate::config::DataPart::Raw(value) => {
                    crate::config::RequestParam::from_text(&value)
                }
                crate::config::DataPart::UrlEncoded(parameter) => parameter,
            })
            .collect(),
        body: imported_body(request.body_parts),
        form: request.form,
        files: request.files,
    }
}

fn imported_body(parts: Vec<crate::config::DataPart>) -> Option<String> {
    (!parts.is_empty()).then(|| {
        parts
            .into_iter()
            .map(|part| match part {
                crate::config::DataPart::Raw(value) => value,
                crate::config::DataPart::UrlEncoded(parameter) => parameter.to_text(),
            })
            .collect::<Vec<_>>()
            .join("&")
    })
}

fn parse_registered_variables(
    input: &str,
    text: crate::i18n::UiText,
) -> Result<Vec<RegisteredVariable>, String> {
    let mut variables = Vec::new();
    let mut names = BTreeSet::new();
    for entry in split_variable_entries(input, text)? {
        let (placeholder, default) = split_variable_default(entry, text)?;
        let name = placeholder
            .strip_prefix("{{")
            .and_then(|value| value.strip_suffix("}}"))
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.contains(['{', '}']))
            .ok_or_else(|| text.curl_import_invalid_variable(placeholder))?;
        if !names.insert(name.to_string()) {
            return Err(text.curl_import_duplicate_variable(name));
        }
        variables.push(RegisteredVariable {
            name: name.to_string(),
            default: default
                .map(|value| parse_variable_default(value, text))
                .transpose()?,
        });
    }
    Ok(variables)
}

fn split_variable_entries(input: &str, text: crate::i18n::UiText) -> Result<Vec<&str>, String> {
    let mut entries = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut comma = false;
    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, character) {
            (Some(_), '\\') => escaped = true,
            (Some(active), value) if active == value => quote = None,
            (None, '\'' | '"') => quote = Some(character),
            (None, separator @ (',' | '\n' | '\r')) => {
                let entry = input[start..index].trim();
                if entry.is_empty() {
                    if separator == ',' {
                        return Err(text.curl_import_empty_variable().to_string());
                    }
                } else {
                    entries.push(entry);
                }
                start = index + character.len_utf8();
                comma = separator == ',';
            }
            _ => {}
        }
    }
    if quote.is_some() || escaped {
        return Err(text.curl_import_unclosed_default().to_string());
    }
    let tail = input[start..].trim();
    if tail.is_empty() {
        if input.trim().is_empty() {
            return Ok(Vec::new());
        }
        if comma {
            return Err(text.curl_import_empty_variable().to_string());
        }
        return Ok(entries);
    }
    entries.push(tail);
    Ok(entries)
}

fn split_variable_default(
    entry: &str,
    text: crate::i18n::UiText,
) -> Result<(&str, Option<&str>), String> {
    let close = entry
        .find("}}")
        .ok_or_else(|| text.curl_import_invalid_variable(entry))?;
    let end = close + 2;
    let placeholder = entry[..end].trim();
    let rest = entry[end..].trim();
    if rest.is_empty() {
        return Ok((placeholder, None));
    }
    let default = rest
        .strip_prefix('=')
        .ok_or_else(|| text.curl_import_invalid_variable(entry))?
        .trim();
    Ok((placeholder, Some(default)))
}

fn parse_variable_default(value: &str, text: crate::i18n::UiText) -> Result<Value, String> {
    if value.is_empty() {
        return Ok(Value::String(String::new()));
    }
    if value.starts_with('"') {
        return serde_json::from_str(value).map_err(|_| text.curl_import_invalid_default(value));
    }
    if value.starts_with('\'') {
        if !value.ends_with('\'') || value.len() < 2 {
            return Err(text.curl_import_invalid_default(value));
        }
        return Ok(Value::String(value[1..value.len() - 1].to_string()));
    }
    Ok(serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string())))
}

fn merge_registered_variables(
    target: &mut BTreeMap<String, crate::config::VariableDefinition>,
    variables: Vec<RegisteredVariable>,
) {
    for variable in variables {
        match target.entry(variable.name) {
            Entry::Occupied(mut entry) => {
                if variable.default.is_some() {
                    entry.get_mut().default = variable.default;
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(crate::config::VariableDefinition {
                    default: variable.default,
                    secret: false,
                });
            }
        }
    }
}
