#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64"),
    all(
        target_os = "macos",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
)))]
compile_error!("postui 仅支持 Linux amd64、Windows x86_64、macOS Intel 和 macOS Apple Silicon");

mod app;
mod cli;
mod clipboard;
mod editor;
mod i18n;
mod logging;
mod paths;
mod response_action;
mod shell;
mod terminal;
mod ui;

pub(crate) use postui_core::{
    config, highlight, http, request_executor, request_file, response_document, response_format,
    response_output, settings, template,
};

use crate::{
    app::App,
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
        CliCommand::Run(options) => run_app(options),
    }
}

fn run_app(options: CliOptions) -> Result<()> {
    let project_path = options
        .project_path
        .as_deref()
        .map(resolve_cli_path)
        .or_else(discover_project_path)
        .ok_or_else(|| {
            anyhow::anyhow!("未找到 PostUI 项目；请在包含 .postui 的目录中运行，或传入项目路径")
        })?;
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
        .with_context(|| format!("初始化 debug 日志失败: {}", log_path.display()))?;
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

    let global_config = match global_config_path.as_deref() {
        Some(path) => match settings::load(path) {
            Ok(config) => config,
            Err(error)
                if !explicit_user_config
                    && error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                settings::default_config()
            }
            Err(error) => {
                tracing::error!(
                    path = %path.display(),
                    error = ?error,
                    "用户界面配置加载失败"
                );
                return Err(error.context(format!("加载用户界面配置失败: {}", path.display())));
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
                "PostUI 工作区加载失败"
            );
            return Err(error.context(format!(
                "加载 PostUI 工作区失败: {}",
                workspace_path.display()
            )));
        }
    };
    if let Some(scenario) = options.scenario {
        if !request_config.configurations.contains_key(&scenario) {
            anyhow::bail!(
                "场景 {scenario} 不存在；可选场景: {}",
                request_config
                    .configurations
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        request_config.default_configuration = scenario;
    }
    let http_client = HttpClient::new().context("初始化 HTTP 客户端失败")?;
    let request_executor = RequestExecutor::new(http_client).context("初始化请求运行时失败")?;
    let mut app = App::new(
        request_config,
        workspace_path,
        global_config,
        request_executor,
        options.debug,
    );

    terminal::run_app(&mut app)
}
