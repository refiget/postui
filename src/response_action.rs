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
}

impl ResponseActionExecutor {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }

    pub(crate) fn copy(&self, body: Bytes) {
        let sender = self.sender.clone();
        thread::spawn(move || {
            let text = String::from_utf8_lossy(&body);
            let result = ClipboardService::new().copy_text(&text);
            let _ = sender.send(FinishedResponseAction::Copied(result));
        });
    }

    pub(crate) fn download(
        &self,
        body: Bytes,
        headers: Vec<(String, String)>,
        request_id: String,
        directory: PathBuf,
    ) {
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = response_output::save_response(&body, &headers, &request_id, &directory);
            let _ = sender.send(FinishedResponseAction::Downloaded(result));
        });
    }

    pub(crate) fn try_recv(&self) -> Result<FinishedResponseAction, TryRecvError> {
        self.receiver.try_recv()
    }
}
