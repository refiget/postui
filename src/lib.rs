#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64"),
    all(
        target_os = "macos",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
)))]
compile_error!("postui 仅支持 Linux amd64、Windows x86_64、macOS Intel 和 macOS Apple Silicon");

pub mod diagnostics;

pub mod config;
pub mod curl;
pub mod highlight;
pub mod http;
pub mod http_method;
pub mod request_executor;
pub mod request_file;
pub mod response_document;
pub mod response_format;
pub mod response_output;
pub mod settings;
pub mod template;

mod app;
mod cli;
mod clipboard;
mod editor;
mod i18n;
mod launch;
mod logging;
mod paths;
mod recent_workspaces;
mod response_action;
mod shell;
mod shortcuts;
mod terminal;
mod ui;
mod workspace_picker;

#[path = "../design-lab/preview/mod.rs"]
pub mod preview;

pub fn run() -> anyhow::Result<()> {
    launch::run()
}
