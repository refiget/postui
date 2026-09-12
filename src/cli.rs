use anyhow::{Result, bail};
use std::{env, path::PathBuf};

#[derive(Debug)]
pub(crate) enum CliCommand {
    Help,
    Init,
    Run(CliOptions),
}

#[derive(Debug)]
pub(crate) struct CliOptions {
    pub(crate) project_path: Option<PathBuf>,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) scenario: Option<String>,
    pub(crate) debug: bool,
    pub(crate) log_file: Option<PathBuf>,
}

pub(crate) fn parse_args() -> Result<CliCommand> {
    let mut args = env::args().skip(1);
    let mut project = None;
    let mut config_path = None;
    let mut scenario = None;
    let mut debug = false;
    let mut log_file = None;
    let mut init = false;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(CliCommand::Help),
            "init" => {
                if init {
                    bail!("postui init 只能指定一次")
                }
                init = true;
            }
            "--debug" => debug = true,
            "--config" => {
                let path = option_value(&mut args, "--config")?;
                if config_path.replace(PathBuf::from(path)).is_some() {
                    bail!("--config 只能指定一次")
                }
            }
            "--scenario" => {
                let name = option_value(&mut args, "--scenario")?;
                if scenario.replace(name).is_some() {
                    bail!("--scenario 只能指定一次")
                }
            }
            "--log-file" => {
                let path = option_value(&mut args, "--log-file")?;
                if log_file.replace(PathBuf::from(path)).is_some() {
                    bail!("日志文件只能指定一次")
                }
            }
            value if value.starts_with('-') => bail!("未知参数: {value}"),
            path => {
                if project.replace(PathBuf::from(path)).is_some() {
                    bail!("项目路径只能指定一次")
                }
            }
        }
    }

    if log_file.is_some() && !debug {
        bail!("--log-file 只能和 --debug 一起使用")
    }

    if init {
        if config_path.is_some() || scenario.is_some() {
            bail!("postui init 不接受 --config 或 --scenario")
        }
        if project.is_some() {
            bail!("postui init 不接受项目路径")
        }
        if debug {
            bail!("postui init 不接受 --debug")
        }
        if log_file.is_some() {
            bail!("postui init 不接受 --log-file")
        }
        return Ok(CliCommand::Init);
    }

    Ok(CliCommand::Run(CliOptions {
        project_path: project,
        config_path,
        scenario,
        debug,
        log_file,
    }))
}

fn option_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String> {
    args.next()
        .filter(|value| !value.trim().is_empty() && !value.starts_with('-'))
        .ok_or_else(|| anyhow::anyhow!("{option} 缺少值；请在选项后填写路径或名称"))
}

pub(crate) fn print_help() {
    print!(
        "用法:\n\
  postui [项目目录] [--config <路径>] [--scenario <名称>] [--debug] [--log-file <路径>]\n\
  postui init\n\n\
不传项目目录时，从当前目录向上查找 .postui。公共请求位于 .postui/requests，场景差异位于 .postui/scenarios。\n\
个人语言和主题配置位于用户配置目录的 postui/config.yaml。\n\
--config 指定个人界面配置；--scenario 指定启动场景，不修改项目默认配置。\n\
默认 debug 日志: .postui/logs/postui-debug.log\n\
--debug 仅在 debug 构建中可用。\n\
postui init 会在 Linux 更新 ~/.zshrc 或 ~/.bashrc；Windows 更新当前用户 PATH。两者都不会写入系统级配置。\n"
    );
}
