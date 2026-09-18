use super::{App, Dialog, Feedback, Focus, PreviewAction, key_kind};
use crate::shortcuts::{self, Command, Context};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

impl App {
    pub(crate) fn handle_paste(&mut self, value: &str) {
        if self.error_page().is_some() || self.view.help_scroll.is_some() {
            return;
        }
        if self.view.curl_import.is_some() {
            if self.view.dialog.is_some() {
                return;
            }
            self.handle_curl_import_paste(value);
            return;
        }
        if self.view.prompt.is_some() || self.view.response.menu.is_open() {
            return;
        }
        let mut truncated = false;
        if let Some(search) = self.view.response.search.as_mut() {
            truncated = search.paste(value);
        } else if self.view.requests.search.is_some() {
            truncated = self.paste_request_search(value);
        } else if let Some(page) = self.view.variables.as_mut() {
            if let Some(editor) = page.editor.as_mut() {
                truncated = editor.paste(value);
            }
        } else if let Some(dialog) = self.view.dialog.as_mut() {
            match dialog {
                Dialog::Configurations(_) => {}
                Dialog::Headers(dialog) => {
                    if let Some(editor) = dialog.editor.as_mut() {
                        truncated = editor.paste(value);
                    }
                }
                Dialog::Params(dialog) => {
                    if let Some(editor) = dialog.editor.as_mut() {
                        truncated = editor.paste(value);
                    }
                }
            }
        } else if let Some(editor) = self.view.preview.temporary_variables.editor_mut() {
            truncated = editor.input.paste(value);
        } else if let Some(editor) = self.view.preview.editor.as_mut() {
            truncated = editor.input.paste(value);
        } else if let Some(editor) = self.view.preview.file_editor.as_mut() {
            truncated = editor.input.paste(value);
        }
        if truncated {
            self.view.notice = Some(Feedback::Warning(self.text().paste_truncated().to_string()));
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        tracing::trace!(
            key_kind = key_kind(key.code),
            modifiers = ?key.modifiers,
            focus = ?self.view.focus,
            "处理键盘操作"
        );

        if self.error_page().is_some() {
            match key.code {
                KeyCode::Esc => self.dismiss_error_page(),
                KeyCode::Char('e' | 'E') => self.request_error_editor(),
                _ => {}
            }
            return;
        }

        if let Some(scroll) = self.view.help_scroll.as_mut() {
            match shortcuts::resolve(Context::Help, key, false) {
                Some(Command::Up) => *scroll = scroll.saturating_sub(1),
                Some(Command::Down) => *scroll = scroll.saturating_add(1),
                Some(Command::Back | Command::Help) => self.view.help_scroll = None,
                _ => {}
            }
            return;
        }

        if self.view.curl_import.is_some() {
            self.handle_curl_import_key(key);
            return;
        }

        let command = shortcuts::resolve(self.key_context(), key, self.debug_mode);
        match command {
            Some(Command::Help) => {
                self.view.help_scroll = Some(0);
                return;
            }
            Some(Command::Theme) => {
                self.load_next_theme();
                return;
            }
            Some(Command::Quit) => {
                self.request_quit();
                return;
            }
            _ => {}
        }

        if self.view.prompt.is_some() {
            self.handle_prompt_key(key);
            return;
        }

        if self.view.response.search.is_some() {
            self.handle_response_search_key(key);
            return;
        }

        if self.view.requests.search.is_some() {
            self.handle_request_search_key(key);
            return;
        }

        if self.view.response.menu.is_open() {
            match command {
                Some(Command::Back) => self.close_response_menu(),
                Some(Command::Up) => self.move_response_menu_selection(-1),
                Some(Command::Down) => self.move_response_menu_selection(1),
                Some(Command::Activate) => self.activate_selected_response_action(),
                _ => {}
            }
            return;
        }

        if self.view.variables.is_some() {
            self.handle_variables_key(key);
            return;
        }

        if self.view.dialog.is_some() {
            let inline_table = self
                .view
                .dialog
                .as_ref()
                .is_some_and(|dialog| dialog.preview_tab().is_some());
            let editing_inline_cell = self.view.dialog.as_ref().is_some_and(Dialog::is_editing);
            let handle_as_global = inline_table
                && !editing_inline_cell
                && (self.view.focus != Focus::Preview
                    || matches!(command, Some(Command::PreviousTab | Command::NextTab))
                    || command.is_some_and(|command| {
                        shortcuts::resolve(Context::Global, key, self.debug_mode) == Some(command)
                            && !matches!(command, Command::Activate | Command::Back)
                    }));
            if !handle_as_global {
                self.handle_dialog_key(key);
                return;
            }
        }

        if self.temporary_variable_editor().is_some() {
            self.handle_temporary_variable_editor_key(key);
            return;
        }

        if self.view.preview.is_editing() {
            self.handle_body_editor_key(key);
            return;
        }

        match command {
            Some(Command::Back) => {
                if self.response_zoomed() {
                    self.restore_standard_view();
                } else if !self.view.requests.filter.is_empty() {
                    self.clear_request_search();
                } else {
                    self.request_quit();
                    tracing::debug!("通过快捷键请求退出");
                }
            }
            Some(Command::FocusNext | Command::FocusPrevious) => {
                self.view.focus = self
                    .view
                    .next_focus(command == Some(Command::FocusPrevious));
                tracing::debug!(focus = ?self.view.focus, "切换 TUI 区域焦点");
            }
            Some(Command::Send) => self.handle_preview_action(PreviewAction::Send),
            Some(Command::Reload) => self.reload_workspace(),
            Some(Command::Workspace) => self.open_configurations(),
            Some(Command::Variables) => self.open_variables(),
            Some(Command::ImportCurl) => self.open_curl_import(),
            Some(Command::ResponseMenu) => self.open_response_menu(),
            Some(Command::Search) if self.view.focus.container() == Focus::Response => {
                self.open_response_search()
            }
            Some(Command::Search) => self.open_request_search(),
            Some(Command::NextMatch) => self.find_response_match(false),
            Some(Command::PreviousMatch) => self.find_response_match(true),
            Some(Command::ResetRequest) => self.restore_current_request(),
            Some(Command::ResetScenario) => self.restore_configuration_requests(),
            Some(Command::Delete) => self.request_delete(),
            Some(Command::Left | Command::PreviousTab)
                if self.view.focus.container() == Focus::Preview =>
            {
                self.move_preview_tab(-1)
            }
            Some(Command::Right | Command::NextTab)
                if self.view.focus.container() == Focus::Preview =>
            {
                self.move_preview_tab(1)
            }
            Some(Command::Left | Command::Right | Command::PreviousTab | Command::NextTab)
                if self.view.focus.container() == Focus::Response =>
            {
                self.move_response_tab(matches!(
                    command,
                    Some(Command::Left | Command::PreviousTab)
                ))
            }
            Some(Command::Up) => self.move_focused(-1),
            Some(Command::Down) => self.move_focused(1),
            Some(Command::Activate) => self.handle_enter(),
            _ => {}
        }
    }

    pub(crate) fn confirm_active_input(&mut self) {
        self.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    pub(crate) fn key_context(&self) -> Context {
        if self.view.prompt.is_some() {
            return Context::Confirm;
        }
        if self.view.curl_import.is_some() {
            return Context::CurlImport;
        }
        if self.view.response.search.is_some() || self.view.requests.search.is_some() {
            return Context::Editor;
        }
        if self.view.response.menu.is_open() {
            return Context::Menu;
        }
        if let Some(page) = &self.view.variables {
            return if page.editor.is_some() {
                Context::Editor
            } else {
                Context::Variables
            };
        }
        if let Some(dialog) = &self.view.dialog {
            if dialog.is_editing() {
                return Context::Editor;
            }
            match dialog {
                Dialog::Configurations(_) => return Context::Menu,
                Dialog::Headers(_) if self.view.focus == Focus::Preview => return Context::Headers,
                Dialog::Params(_) if self.view.focus == Focus::Preview => return Context::Params,
                _ => {}
            }
        }
        if self.view.is_editing() {
            return Context::Editor;
        }
        match self.view.focus.container() {
            Focus::Requests => Context::Requests,
            Focus::Preview => Context::Preview,
            Focus::Response => Context::Response,
            _ => Context::Global,
        }
    }

    fn load_next_theme(&mut self) {
        let current = self.global_config.theme.name.clone();
        match crate::settings::next_theme(&current) {
            Ok(theme) => {
                let name = theme.name.clone();
                self.global_config.theme = theme;
                tracing::debug!(previous_theme = %current, theme = %name, "热加载内置主题");
            }
            Err(error) => {
                self.view.notice = Some(Feedback::Error(
                    self.text().theme_load_failed(&error.to_string()),
                ));
                tracing::error!(error = ?error, "热加载内置主题失败");
            }
        }
    }

    fn handle_enter(&mut self) {
        tracing::debug!(focus = ?self.view.focus, "处理 Enter 操作");
        match self.view.focus {
            Focus::Header => {}
            Focus::Requests => {}
            Focus::Variables => self.open_variables(),
            Focus::Preview => {
                if !self.start_temporary_variable_edit() {
                    let action = PreviewAction::Edit(self.view.preview.active_tab);
                    self.handle_preview_action(action);
                }
            }
            Focus::WorkspaceButton => self.open_configurations(),
            Focus::SendButton => self.handle_preview_action(PreviewAction::Send),
            Focus::ResponseActions => self.open_response_menu(),
            Focus::ResponseZoom => self.toggle_response_zoom(),
            Focus::Response => {}
        }
    }

    fn move_focused(&mut self, direction: isize) {
        let container = self.view.focus.container();
        self.view.focus = container;
        match container {
            Focus::Header => {}
            Focus::Requests => self.move_request(direction),
            Focus::Preview => {
                if !self.move_temporary_variable(direction) {
                    self.view.preview.scroll.move_by(direction);
                }
            }
            Focus::Response => self.scroll_response(direction),
            Focus::WorkspaceButton
            | Focus::Variables
            | Focus::SendButton
            | Focus::ResponseActions
            | Focus::ResponseZoom => {}
        }
    }
}
