use super::{App, Dialog, Feedback, Focus, PreviewAction, key_kind};
use crate::shortcuts::{self, Command, Context};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// 接收按键的层；按从上层到下层的顺序判断，最上层接收按键。
enum Layer {
    ErrorPage,
    Help,
    CurlImport,
    Prompt,
    ResponseSearch,
    RequestSearch,
    ResponseMenu,
    TableEditor,
    Table,
    ContentEditor,
    Main,
}

impl App {
    fn layer(&self) -> Layer {
        if self.error_page().is_some() {
            return Layer::ErrorPage;
        }
        if self.view.help_scroll.is_some() {
            return Layer::Help;
        }
        if self.view.curl_import.is_some() {
            return Layer::CurlImport;
        }
        if self.view.prompt.is_some() {
            return Layer::Prompt;
        }
        if self.view.response.search.is_some() {
            return Layer::ResponseSearch;
        }
        if self.view.requests.search.is_some() {
            return Layer::RequestSearch;
        }
        if self.view.response.menu.is_open() {
            return Layer::ResponseMenu;
        }
        if self.view.dialog.as_ref().is_some_and(Dialog::is_editing) {
            return Layer::TableEditor;
        }
        if self.view.dialog.is_some() {
            return Layer::Table;
        }
        if self.view.preview.is_editing() {
            return Layer::ContentEditor;
        }
        Layer::Main
    }

    /// 是否有文本输入在接收按键。
    pub(crate) fn is_editing(&self) -> bool {
        matches!(
            self.layer(),
            Layer::CurlImport
                | Layer::ResponseSearch
                | Layer::RequestSearch
                | Layer::TableEditor
                | Layer::ContentEditor
        )
    }

    pub(crate) fn handle_paste(&mut self, value: &str) {
        let truncated = match self.layer() {
            Layer::CurlImport => {
                self.handle_curl_import_paste(value);
                return;
            }
            Layer::ResponseSearch => self
                .view
                .response
                .search
                .as_mut()
                .is_some_and(|search| search.paste(value)),
            Layer::RequestSearch => self.paste_request_search(value),
            Layer::Table | Layer::TableEditor => self
                .view
                .dialog
                .as_mut()
                .is_some_and(|dialog| dialog.paste(value)),
            Layer::ContentEditor => self
                .view
                .preview
                .editor
                .as_mut()
                .is_some_and(|editor| editor.input.paste(value)),
            Layer::ErrorPage | Layer::Help | Layer::Prompt | Layer::ResponseMenu | Layer::Main => {
                return;
            }
        };
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

        let layer = self.layer();
        match layer {
            Layer::ErrorPage => {
                self.handle_error_page_key(key);
                return;
            }
            Layer::Help => {
                self.handle_help_key(key);
                return;
            }
            Layer::CurlImport => {
                self.handle_curl_import_key(key);
                return;
            }
            _ => {}
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

        match layer {
            Layer::Prompt => {
                self.handle_prompt_key(key);
                return;
            }
            Layer::ResponseSearch => {
                self.handle_response_search_key(key);
                return;
            }
            Layer::RequestSearch => {
                self.handle_request_search_key(key);
                return;
            }
            Layer::ResponseMenu => {
                self.handle_response_menu_key(command);
                return;
            }
            Layer::TableEditor => {
                self.handle_dialog_key(key);
                return;
            }
            Layer::Table if !self.table_yields_to_global(key, command) => {
                self.handle_dialog_key(key);
                return;
            }
            Layer::ContentEditor => {
                self.handle_content_editor_key(key);
                return;
            }
            _ => {}
        }

        self.handle_main_key(command);
    }

    fn handle_error_page_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.dismiss_error_page(),
            KeyCode::Char('e' | 'E') => self.request_error_editor(),
            _ => {}
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        let Some(scroll) = self.view.help_scroll.as_mut() else {
            return;
        };
        match shortcuts::resolve(Context::Help, key, false) {
            Some(Command::Up) => *scroll = scroll.saturating_sub(1),
            Some(Command::Down) => *scroll = scroll.saturating_add(1),
            Some(Command::Back | Command::Help) => self.view.help_scroll = None,
            _ => {}
        }
    }

    fn handle_response_menu_key(&mut self, command: Option<Command>) {
        match command {
            Some(Command::Back) => self.close_response_menu(),
            Some(Command::Up) => self.move_response_menu_selection(-1),
            Some(Command::Down) => self.move_response_menu_selection(1),
            Some(Command::Activate) => self.activate_selected_response_action(),
            _ => {}
        }
    }

    fn handle_main_key(&mut self, command: Option<Command>) {
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
            Some(Command::EditRequest) if self.view.focus == Focus::Requests => {
                self.edit_current_request()
            }
            Some(Command::EditDraft) if self.view.focus == Focus::Preview => {
                self.edit_preview_draft()
            }
            Some(Command::ImportCurl) => self.open_curl_import(),
            Some(Command::ResponseMenu) => self.open_response_menu(),
            Some(Command::ResponseFormat) => self.toggle_response_format_tab(),
            Some(Command::ResponseZoom) => self.toggle_response_zoom(),
            Some(Command::Search) if self.view.focus == Focus::Response => {
                self.open_response_search()
            }
            Some(Command::Search) => self.open_request_search(),
            Some(Command::NextMatch) => self.find_response_match(false),
            Some(Command::PreviousMatch) => self.find_response_match(true),
            Some(Command::ResetRequest) => self.restore_current_request(),
            Some(Command::ResetScenario) => self.restore_configuration_requests(),
            Some(Command::Delete) => self.request_delete(),
            Some(Command::PreviousTab | Command::NextTab) if self.view.focus == Focus::Preview => {
                self.move_preview_tab(if command == Some(Command::PreviousTab) {
                    -1
                } else {
                    1
                })
            }
            Some(Command::PreviousTab | Command::NextTab) if self.view.focus == Focus::Response => {
                self.move_response_tab(command == Some(Command::PreviousTab))
            }
            Some(Command::Up) => self.move_focused(-1),
            Some(Command::Down) => self.move_focused(1),
            Some(Command::Activate) => self.handle_enter(),
            _ => {}
        }
    }

    /// Params/Headers 表格打开时，焦点切换、页签切换和主界面已绑定的按键交给主界面处理。
    fn table_yields_to_global(&self, key: KeyEvent, command: Option<Command>) -> bool {
        let table_open = self
            .view
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.preview_tab().is_some());
        if !table_open || self.view.dialog.as_ref().is_some_and(Dialog::is_editing) {
            return false;
        }
        if self.view.focus != Focus::Preview {
            return true;
        }
        if matches!(command, Some(Command::PreviousTab | Command::NextTab)) {
            return true;
        }
        if command == Some(Command::EditDraft) {
            return true;
        }
        command.is_some_and(|command| {
            !matches!(command, Command::Activate | Command::Back)
                && shortcuts::resolve(Context::Global, key, self.debug_mode) == Some(command)
        })
    }

    pub(crate) fn confirm_active_input(&mut self) {
        self.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    /// 当前上下文的按键提示和按键表。
    pub(crate) fn key_context(&self) -> Context {
        match self.layer() {
            Layer::CurlImport => Context::CurlImport,
            Layer::Prompt => Context::Confirm,
            Layer::ResponseSearch
            | Layer::RequestSearch
            | Layer::TableEditor
            | Layer::ContentEditor => Context::Editor,
            Layer::ResponseMenu => Context::Menu,
            Layer::Table => match self.view.dialog.as_ref() {
                Some(Dialog::Configurations(_)) => Context::Menu,
                Some(Dialog::Headers(_)) if self.view.focus == Focus::Preview => Context::Headers,
                Some(Dialog::Params(_)) if self.view.focus == Focus::Preview => Context::Params,
                _ => self.focus_context(),
            },
            Layer::ErrorPage | Layer::Help | Layer::Main => self.focus_context(),
        }
    }

    fn focus_context(&self) -> Context {
        match self.view.focus {
            Focus::Requests => Context::Requests,
            Focus::Preview => Context::Preview,
            Focus::Response => Context::Response,
            Focus::Header => Context::Global,
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
        if self.view.focus != Focus::Preview {
            return;
        }
        let action = PreviewAction::Edit(self.view.preview.active_tab);
        self.handle_preview_action(action);
    }

    fn move_focused(&mut self, direction: isize) {
        match self.view.focus {
            Focus::Requests => self.move_request(direction),
            Focus::Preview => {
                if !self.move_content_field(direction) {
                    self.view.preview.scroll.move_by(direction);
                }
            }
            Focus::Response => self.scroll_response(direction),
            Focus::Header => {}
        }
    }
}
