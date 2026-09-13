#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
compile_error!("postui 仅支持 Linux amd64 (x86_64) 和 Windows x86_64");

mod cache;

pub mod config;
pub mod highlight;
pub mod http;
pub mod request_executor;
pub mod request_file;
pub mod response_document;
pub mod response_format;
pub mod response_output;
pub mod settings;
pub mod template;
