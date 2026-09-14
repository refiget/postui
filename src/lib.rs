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
