use std::{fmt::Debug, marker::PhantomData};

use bytes::{BufMut, BytesMut};
use futures::SinkExt;
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::AsyncWrite;

#[derive(Clone)]
pub struct MsgSender<T: Serialize> {
  sender: tokio::sync::mpsc::UnboundedSender<T>,
}

impl<T: Serialize + Send + Debug + 'static> MsgSender<T> {
  pub fn new(sender: tokio::sync::mpsc::UnboundedSender<T>) -> Self {
    MsgSender { sender }
  }

  pub fn new_write<W: AsyncWrite + Unpin + Send + 'static>(write: W) -> Self {
    let mut framed =
      tokio_util::codec::FramedWrite::new(write, MsgEncoder::<T>::new());

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    tokio::spawn(async move {
      loop {
        let msg = rx.recv().await;
        let msg = match msg {
          Some(msg) => msg,
          None => break,
        };

        // TODO: Use `framed.feed()`
        match framed.send(msg).await {
          Ok(()) => (),
          Err(_) => break,
        }
      }
    });

    MsgSender { sender: tx }
  }
}

impl<T: Serialize + DeserializeOwned + Debug> MsgSender<T> {
  pub fn send(
    &mut self,
    msg: T,
  ) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
    self.sender.send(msg)
  }
}

struct MsgEncoder<T: Serialize> {
  t: PhantomData<T>,
  buf: Vec<u8>,
}

impl<T: Serialize> MsgEncoder<T> {
  pub fn new() -> Self {
    MsgEncoder {
      t: PhantomData,
      buf: Vec::new(),
    }
  }
}

impl<T: Serialize + Debug> tokio_util::codec::Encoder<T> for MsgEncoder<T> {
  type Error = bincode::Error;

  fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
    bincode::serialize_into(&mut self.buf, &item)?;
    dst.put_u32(self.buf.len() as u32);
    dst.extend_from_slice(&self.buf);
    self.buf.clear();
    Ok(())
  }
}
