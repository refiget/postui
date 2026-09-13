use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
};

use bytes::Bytes;

use crate::{clipboard::ClipboardService, response_output};

#[derive(Debug)]
pub(crate) enum FinishedResponseAction {
    Copied(Result<(), String>),
    Downloaded(Result<PathBuf, String>),
}

pub(crate) struct ResponseActionExecutor {
    sender: Sender<FinishedResponseAction>,
    receiver: Receiver<FinishedResponseAction>,
    running: bool,
}

impl ResponseActionExecutor {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            running: false,
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    pub(crate) fn copy(&mut self, body: Bytes) {
        if self.running {
            return;
        }
        self.running = true;
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = std::str::from_utf8(&body)
                .map_err(|_| "响应不是有效 UTF-8，请下载原始内容".to_string())
                .and_then(|text| ClipboardService::new().copy_text(text));
            let _ = sender.send(FinishedResponseAction::Copied(result));
        });
    }

    pub(crate) fn download(
        &mut self,
        body: Bytes,
        headers: Vec<(String, String)>,
        request_id: String,
        directory: PathBuf,
    ) {
        if self.running {
            return;
        }
        self.running = true;
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = response_output::save_response(&body, &headers, &request_id, &directory);
            let _ = sender.send(FinishedResponseAction::Downloaded(result));
        });
    }

    pub(crate) fn try_recv(&mut self) -> Result<FinishedResponseAction, TryRecvError> {
        let result = self.receiver.try_recv()?;
        self.running = false;
        Ok(result)
    }
}
