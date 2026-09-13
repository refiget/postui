#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64"),
    all(
        target_os = "macos",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
)))]
compile_error!(
    "postui supports only Linux amd64, Windows x86_64, macOS Intel, and macOS Apple Silicon"
);

mod app;
mod cli;
mod clipboard;
mod editor;
mod i18n;
mod logging;
mod paths;
mod response_action;
mod shell;
mod shortcuts;
mod terminal;
mod ui;

pub(crate) use postui_core::{
    config, diagnostics, highlight, http, request_executor, request_file, response_document,
    response_format, response_output, settings, template,
};

use crate::{
    app::{App, ErrorPage},
    cli::{CliCommand, CliOptions, parse_args, print_help},
    config::load as load_request_config,
    http::HttpClient,
    paths::{discover_project_path, discover_user_config_path, resolve_cli_path},
    request_executor::RequestExecutor,
    shell::init_shell_integration,
};
use anyhow::{Context, Result};

fn main() -> Result<()> {
    let command = parse_args()?;
    match command {
        CliCommand::Help => {
            print_help();
            Ok(())
        }
        CliCommand::Init => init_shell_integration(),
        CliCommand::Version => {
            println!("postui {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        CliCommand::Run(options) => run_app(options),
    }
}

fn run_app(options: CliOptions) -> Result<()> {
    let project_path = options
        .project_path
        .as_deref()
        .map(resolve_cli_path)
        .or_else(discover_project_path)
        .unwrap_or_else(|| {
            let path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            tracing::debug!(
                path = %path.display(),
                "No PostUI project found; opening the default workspace"
            );
            path
        });
    let workspace_path = project_path.join(".postui");
    let explicit_user_config = options.config_path.is_some();
    let global_config_path = options
        .config_path
        .as_deref()
        .map(resolve_cli_path)
        .or_else(discover_user_config_path);
    let log_path = options
        .log_file
        .unwrap_or_else(|| workspace_path.join("logs/postui-debug.log"));
    logging::init(options.debug, options.perf, &log_path)
        .with_context(|| format!("Failed to initialize debug logging: {}", log_path.display()))?;
    tracing::debug!(
        config_path = global_config_path
            .as_deref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<内置界面配置>".to_string()),
        project_path = %project_path.display(),
        debug = options.debug,
        log_file = %log_path.display(),
        "启动 PostUI"
    );

    let mut error_page = None;
    let mut global_config = match global_config_path.as_deref() {
        Some(path) => match settings::load(path) {
            Ok(config) => config,
            Err(error) if !explicit_user_config && settings::is_not_found(&error) => {
                settings::default_config()
            }
            Err(error) => {
                tracing::error!(
                    path = %path.display(),
                    error = ?error,
                    "User interface configuration failed to load"
                );
                error_page = Some(ErrorPage::from_error(&error, path.to_path_buf()));
                settings::default_config()
            }
        },
        None => settings::default_config(),
    };
    tracing::debug!(
        path = %workspace_path.display(),
        "选择 PostUI 工作区"
    );
    let mut request_config = match load_request_config(&workspace_path) {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(
                path = %workspace_path.display(),
                error = ?error,
                "PostUI workspace failed to load"
            );
            if error_page.is_none() {
                error_page = Some(ErrorPage::from_error(
                    &error,
                    workspace_path.join("postui.yaml"),
                ));
            }
            config::RequestConfig::default_for_workspace(&workspace_path)
        }
    };
    if let Some(scenario) = options.scenario {
        if !request_config.configurations.contains_key(&scenario) {
            if error_page.is_none() {
                let available = request_config
                    .configurations
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                let error = diagnostics::invalid(
                    &workspace_path.join("postui.yaml"),
                    "scenario",
                    format!("Scenario {scenario} does not exist; available scenarios: {available}"),
                );
                error_page = Some(ErrorPage::from_error(
                    &error,
                    workspace_path.join("postui.yaml"),
                ));
            }
        } else {
            request_config.default_configuration = scenario;
        }
    }
    if error_page.is_some() {
        global_config = settings::default_config();
    }
    let http_client = HttpClient::new().context("Failed to initialize the HTTP client")?;
    let request_executor =
        RequestExecutor::new(http_client).context("Failed to initialize the request runtime")?;
    let mut app = App::new(
        request_config,
        workspace_path,
        global_config,
        request_executor,
        options.debug,
        error_page,
    );

    terminal::run_app(&mut app)
}
