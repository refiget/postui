use super::App;

impl App {
    pub(crate) fn current_request_variables(&self) -> &std::collections::BTreeMap<String, String> {
        &self.workspace_state.variables
    }

    pub(super) fn variable_is_secret(&self, variable: &str) -> bool {
        self.config
            .variables
            .get(variable)
            .is_some_and(|definition| definition.secret)
            || self
                .config
                .configurations
                .get(self.active_configuration())
                .and_then(|configuration| configuration.variables.get(variable))
                .is_some_and(|definition| definition.secret)
    }

    pub(crate) fn secret_variable_values(&self) -> Vec<String> {
        self.workspace_state
            .variables
            .iter()
            .filter(|(name, value)| self.variable_is_secret(name) && !value.is_empty())
            .map(|(_, value)| value.clone())
            .collect()
    }

    pub(crate) fn open_variables(&mut self) {
        let path = self.variable_config_path();
        tracing::debug!(path = %path.display(), "打开变量配置文件的外部编辑器");
        self.editor_request = Some(super::EditorRequest {
            path,
            outcome: super::EditorOutcome::ReloadWorkspace,
        });
    }

    fn variable_config_path(&self) -> std::path::PathBuf {
        self.config
            .configurations
            .get(self.active_configuration())
            .and_then(|configuration| configuration.path.clone())
            .unwrap_or_else(|| {
                self.workspace_path()
                    .join(crate::config::WORKSPACE_FILE_NAME)
            })
    }
}
