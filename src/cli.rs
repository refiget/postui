use anyhow::{Result, bail};
use std::{env, path::PathBuf};

#[derive(Debug)]
pub(crate) enum CliCommand {
    Help,
    Version,
    Init,
    Run(CliOptions),
}

#[derive(Debug)]
pub(crate) struct CliOptions {
    pub(crate) project_path: Option<PathBuf>,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) scenario: Option<String>,
    pub(crate) debug: bool,
    pub(crate) perf: bool,
    pub(crate) log_file: Option<PathBuf>,
}

pub(crate) fn parse_args() -> Result<CliCommand> {
    let mut args = env::args().skip(1);
    let mut project = None;
    let mut config_path = None;
    let mut scenario = None;
    let mut debug = false;
    let mut perf = false;
    let mut log_file = None;
    let mut init = false;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(CliCommand::Help),
            "-V" | "--version" => return Ok(CliCommand::Version),
            "init" => {
                if init {
                    bail!("postui init may be specified only once")
                }
                init = true;
            }
            "--debug" => debug = true,
            "--perf" => {
                perf = true;
                debug = true;
            }
            "--config" => {
                let path = option_value(&mut args, "--config")?;
                if config_path.replace(PathBuf::from(path)).is_some() {
                    bail!("--config may be specified only once")
                }
            }
            "--scenario" => {
                let name = option_value(&mut args, "--scenario")?;
                if scenario.replace(name).is_some() {
                    bail!("--scenario may be specified only once")
                }
            }
            "--log-file" => {
                let path = option_value(&mut args, "--log-file")?;
                if log_file.replace(PathBuf::from(path)).is_some() {
                    bail!("--log-file may be specified only once")
                }
            }
            value if value.starts_with('-') => bail!("Unknown argument: {value}"),
            path => {
                if project.replace(PathBuf::from(path)).is_some() {
                    bail!("Project path may be specified only once")
                }
            }
        }
    }

    if log_file.is_some() && !debug {
        bail!("--log-file requires --debug")
    }

    if init {
        if config_path.is_some() || scenario.is_some() {
            bail!("postui init does not accept --config or --scenario")
        }
        if project.is_some() {
            bail!("postui init does not accept a project path")
        }
        if debug {
            bail!("postui init does not accept --debug")
        }
        if log_file.is_some() {
            bail!("postui init does not accept --log-file")
        }
        return Ok(CliCommand::Init);
    }

    Ok(CliCommand::Run(CliOptions {
        project_path: project,
        config_path,
        scenario,
        debug,
        perf,
        log_file,
    }))
}

fn option_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String> {
    args.next()
        .filter(|value| !value.trim().is_empty() && !value.starts_with('-'))
        .ok_or_else(|| {
            anyhow::anyhow!("{option} requires a value; provide a path or name after it")
        })
}

pub(crate) fn print_help() {
    print!(
        "用法:\n\
  postui [项目目录] [--config <路径>] [--scenario <名称>] [--debug | --perf] [--log-file <路径>]\n\
  postui init\n\
  postui --version\n\n\
不传项目目录时，从当前目录向上查找 .postui。公共请求位于 .postui/requests，场景差异位于 .postui/scenarios。\n\
个人语言和主题配置位于用户配置目录的 postui/config.yaml。\n\
--config 指定个人界面配置；--scenario 指定启动场景，不修改项目默认配置。\n\
默认 debug 日志: .postui/logs/postui-debug.log\n\
--debug 仅在 debug 构建中可用。\n\
--perf 仅记录性能指标，不记录请求/响应内容；同样需要 debug 构建。\n\
postui init 会在 Linux 或 macOS 更新 ~/.zshrc 或 ~/.bashrc；Windows 更新当前用户 PATH。这些操作都不会写入系统级配置。\n"
    );
}
