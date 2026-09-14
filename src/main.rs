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
mod recent_workspaces;
mod response_action;
mod shell;
mod shortcuts;
mod terminal;
mod ui;
mod workspace_picker;

pub(crate) use postui_core::{
    config, curl, diagnostics, highlight, http, http_method, request_executor, request_file,
    response_document, response_format, response_output, settings, template,
};

use crate::{
    app::{App, ErrorPage},
    cli::{CliCommand, CliOptions, parse_args, print_help},
    config::load as load_request_config,
    http::HttpClient,
    paths::{discover_user_config_path, discover_workspace, resolve_cli_path, resolve_workspace},
    request_executor::RequestExecutor,
    shell::{init_shell_integration, uninstall},
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
        CliCommand::Uninstall => uninstall(),
        CliCommand::Version => {
            println!("postui {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        CliCommand::Run(options) => run_app(options),
    }
}

fn run_app(options: CliOptions) -> Result<()> {
    let workspace = match options.project_path.as_deref() {
        Some(path) => Some(resolve_workspace(path)?),
        None => discover_workspace()?,
    };
    let explicit_user_config = options.config_path.is_some();
    let global_config_path = options
        .config_path
        .as_deref()
        .map(resolve_cli_path)
        .transpose()?
        .or_else(discover_user_config_path);
    let (mut global_config, mut error_page) = load_global_config(
        global_config_path.as_deref(),
        explicit_user_config,
        workspace.is_some(),
    )?;
    let workspace = match workspace {
        Some(workspace) => workspace,
        None => match workspace_picker::run(&mut global_config, options.debug)? {
            Some(path) => paths::WorkspaceLocation {
                path,
                source: paths::WorkspaceSource::Selected,
            },
            None => return Ok(()),
        },
    };
    let workspace_path = workspace.path;
    let log_path = options
        .log_file
        .as_deref()
        .map(resolve_cli_path)
        .transpose()?
        .unwrap_or_else(|| workspace_path.join("logs/postui-debug.log"));
    logging::init(options.debug, options.perf, &log_path)
        .with_context(|| format!("Failed to initialize debug logging: {}", log_path.display()))?;
    tracing::debug!(
        config_path = global_config_path
            .as_deref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<内置界面配置>".to_string()),
        workspace_path = %workspace_path.display(),
        workspace_source = ?workspace.source,
        debug = options.debug,
        log_file = %log_path.display(),
        "启动 PostUI"
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
    if error_page.is_none() {
        recent_workspaces::RecentWorkspaces::remember(
            workspace_path.clone(),
            request_config.name.clone(),
        )?;
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

fn load_global_config(
    path: Option<&std::path::Path>,
    explicit_path: bool,
    workspace_available: bool,
) -> Result<(settings::GlobalConfig, Option<ErrorPage>)> {
    let Some(path) = path else {
        return Ok((settings::default_config(), None));
    };
    match settings::load(path) {
        Ok(config) => Ok((config, None)),
        Err(error) if !explicit_path && settings::is_not_found(&error) => {
            Ok((settings::default_config(), None))
        }
        Err(error) if !workspace_available => Err(error),
        Err(error) => {
            tracing::error!(
                path = %path.display(),
                error = ?error,
                "User interface configuration failed to load"
            );
            Ok((
                settings::default_config(),
                Some(ErrorPage::from_error(&error, path.to_path_buf())),
            ))
        }
    }
}
