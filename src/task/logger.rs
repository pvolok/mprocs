use std::path::PathBuf;

use bytes::Bytes;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::error::ResultLogger;

const CHANNEL_CAP: usize = 256;

pub struct LogSink {
  pub path: PathBuf,
  pub append: bool,
}

pub type LogResolver = Box<dyn FnMut(u32) -> Option<LogSink> + Send>;

pub fn spawn_logger(sink: LogSink) -> Sender<Bytes> {
  let (tx, rx) = mpsc::channel(CHANNEL_CAP);
  tokio::spawn(logger_main(rx, sink));
  tx
}

async fn logger_main(mut rx: Receiver<Bytes>, sink: LogSink) {
  let mut file = match open_log(&sink).await {
    Some(file) => file,
    None => return,
  };
  while let Some(bytes) = rx.recv().await {
    file.write_all(&bytes).await.log_ignore();
  }
}

async fn open_log(sink: &LogSink) -> Option<tokio::fs::File> {
  if let Some(parent) = sink.path.parent() {
    tokio::fs::create_dir_all(parent).await.log_ignore();
  }
  let mut options = tokio::fs::OpenOptions::new();
  options.create(true).write(true).append(sink.append);
  if !sink.append {
    options.truncate(true);
  }
  options
    .open(&sink.path)
    .await
    .map_err(|e| log::warn!("Failed to open log file {:?}: {}", sink.path, e))
    .ok()
}
