use super::{App, Dialog, Feedback, Focus, PreviewAction, key_kind};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == crossterm::event::KeyEventKind::Release {
            return;
        }
        tracing::trace!(
            key_kind = key_kind(key.code),
            modifiers = ?key.modifiers,
            focus = ?self.view.focus,
            "处理键盘操作"
        );

        if key.code == KeyCode::F(5) && self.debug_mode {
            self.load_next_theme();
            return;
        }

        if self.view.help_visible {
            self.view.help_visible = false;
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

        if self.view.prompt.is_some() {
            self.handle_prompt_key(key);
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.request_quit();
            tracing::debug!("通过 Ctrl+C 请求退出");
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
                    || matches!(
                        key.code,
                        KeyCode::Tab
                            | KeyCode::BackTab
                            | KeyCode::Char('r' | 'R' | 'w' | 'v' | 'o' | 'q' | '/' | '?')
                    ));
            if !handle_as_global {
                self.handle_dialog_key(key);
                return;
            }
        }

        if self.view.preview.is_editing() {
            self.handle_body_editor_key(key);
            return;
        }

        if self.view.response.menu_selection.is_some() {
            match key.code {
                KeyCode::Esc => self.close_response_menu(),
                KeyCode::Char('q') if self.response_zoomed() => self.restore_standard_view(),
                KeyCode::Up | KeyCode::Char('k') => self.move_response_menu_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_response_menu_selection(1),
                KeyCode::Enter | KeyCode::Char(' ') => self.activate_selected_response_action(),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.response_zoomed() {
                    self.restore_standard_view();
                } else if !self.view.requests.filter.is_empty() {
                    self.clear_request_search();
                } else {
                    self.request_quit();
                    tracing::debug!("通过快捷键请求退出");
                }
            }
            KeyCode::Tab => {
                self.view.focus = self
                    .view
                    .next_focus(key.modifiers.contains(KeyModifiers::SHIFT));
                tracing::debug!(focus = ?self.view.focus, "切换 TUI 区域焦点");
            }
            KeyCode::BackTab => self.view.focus = self.view.next_focus(true),
            KeyCode::Char('r') => self.handle_preview_action(PreviewAction::Send),
            KeyCode::Char('R') => self.reload_workspace(),
            KeyCode::Char('w') => self.open_configurations(),
            KeyCode::Char('v') => self.open_variables(),
            KeyCode::Char('o') => self.open_response_menu(),
            KeyCode::Char('/') if self.view.focus == Focus::Response => self.open_response_search(),
            KeyCode::Char('/') => self.open_request_search(),
            KeyCode::Char('n') if self.view.focus == Focus::Response => {
                self.find_response_match(false)
            }
            KeyCode::Char('N') if self.view.focus == Focus::Response => {
                self.find_response_match(true)
            }
            KeyCode::Char('?') => self.view.help_visible = true,
            KeyCode::Char('u') if self.view.focus == Focus::Requests => {
                self.restore_current_request()
            }
            KeyCode::Char('X') if self.view.focus == Focus::Requests => {
                self.restore_configuration_requests()
            }
            KeyCode::Delete if self.view.focus == Focus::Requests => self.request_delete(),
            KeyCode::Left if self.view.focus == Focus::Preview => self.move_preview_tab(-1),
            KeyCode::Right if self.view.focus == Focus::Preview => self.move_preview_tab(1),
            KeyCode::Left | KeyCode::Right if self.view.focus == Focus::Response => {
                self.move_response_tab(key.code == KeyCode::Left)
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_focused(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_focused(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.handle_enter(),
            _ => {}
        }
    }

    fn load_next_theme(&mut self) {
        let current = self.global_config.theme.name.clone();
        match crate::settings::next_theme(&current) {
            Ok(theme) => {
                let name = theme.name.clone();
                self.global_config.theme = theme;
                self.view.notice = Some(Feedback::Info(self.text().theme_loaded(&name)));
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
                let action = PreviewAction::Edit(self.view.preview.active_tab);
                self.handle_preview_action(action);
            }
            Focus::WorkspaceButton => self.open_configurations(),
            Focus::SendButton => self.handle_preview_action(PreviewAction::Send),
            Focus::ResponseActions => self.open_response_menu(),
            Focus::ResponseZoom => self.toggle_response_zoom(),
            Focus::Response => {}
        }
    }

    fn move_focused(&mut self, direction: isize) {
        match self.view.focus {
            Focus::Header => {}
            Focus::Requests => self.move_request(direction),
            Focus::Preview => {
                self.view.preview.scroll.move_by(direction);
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
