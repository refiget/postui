use std::{
    env, fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};

use crate::{
    app::App, config, http::HttpClient, request_executor::RequestExecutor, settings, terminal,
};

mod server;
mod workspace;

struct Options {
    theme: String,
    language: String,
}

pub fn run() -> Result<()> {
    let Some(options) = Options::parse()? else {
        return Ok(());
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("读取预览启动时间失败")?
        .as_nanos();
    let directory =
        env::temp_dir().join(format!("postui-preview-{}-{timestamp}", std::process::id()));
    fs::create_dir(&directory)
        .with_context(|| format!("创建预览目录失败：{}", directory.display()))?;
    let result = run_in_directory(&directory, &options);
    let cleanup = fs::remove_dir_all(&directory)
        .with_context(|| format!("删除预览目录失败：{}", directory.display()));
    finish(result, cleanup)
}

fn run_in_directory(directory: &Path, options: &Options) -> Result<()> {
    let server = server::PreviewServer::start()?;
    let result = run_workspace(directory, options, &server.base_url());
    finish(result, server.stop())
}

fn run_workspace(directory: &Path, options: &Options, base_url: &str) -> Result<()> {
    let workspace_path = workspace::create(directory, base_url, &options.theme, &options.language)?;
    let global_config = settings::load(&directory.join("config.yaml"))?;
    let request_config = config::load(&workspace_path)?;
    let client = HttpClient::new().context("初始化预览 HTTP 客户端失败")?;
    let executor = RequestExecutor::new(client).context("初始化预览请求运行时失败")?;
    let mut app = App::new(
        request_config,
        workspace_path,
        global_config,
        executor,
        true,
        None,
    );
    app.send_current_request();
    terminal::run_app(&mut app)
}

fn finish(result: Result<()>, cleanup: Result<()>) -> Result<()> {
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(anyhow!("{error:#}\n{cleanup:#}")),
    }
}

impl Options {
    fn parse() -> Result<Option<Self>> {
        let mut options = Self {
            theme: "gruvbox-dark".to_string(),
            language: "zh".to_string(),
        };
        let mut arguments = env::args().skip(1);
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--theme" => {
                    options.theme = arguments.next().context("--theme 缺少主题名称")?;
                    if !settings::BUILT_IN_THEME_NAMES.contains(&options.theme.as_str()) {
                        bail!("主题不存在：{}", options.theme);
                    }
                }
                "--language" => {
                    options.language = arguments.next().context("--language 缺少语言代码")?;
                    if !matches!(options.language.as_str(), "zh" | "en") {
                        bail!("语言代码：zh、en");
                    }
                }
                "--help" | "-h" => {
                    println!(
                        "PostUI 前端预览\n\n\
                         cargo run --example frontend_preview -- [选项]\n\n\
                         --theme <名称>     主题，默认 gruvbox-dark\n\
                         --language <代码>  语言，默认 zh；可选 zh、en\n\n\
                         内置示例接口：127.0.0.1 临时端口\n\
                         工作区、上传文件、下载文件：本次运行的临时目录\n\
                         退出后删除本次运行的临时目录"
                    );
                    return Ok(None);
                }
                _ => bail!("未知选项：{argument}"),
            }
        }
        Ok(Some(options))
    }
}
