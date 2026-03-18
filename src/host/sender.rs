use std::{fmt::Debug, marker::PhantomData};

use bytes::{BufMut, BytesMut};
use futures::SinkExt;
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::AsyncWrite;
use tokio_util::codec::FramedWrite;

type DynWrite = dyn AsyncWrite + Unpin + Send + 'static;

pub struct MsgSender<T: Serialize> {
  writer: FramedWrite<Box<DynWrite>, MsgEncoder<T>>,
}

impl<T: Serialize + Send + Debug + 'static> MsgSender<T> {
  pub fn new<W: AsyncWrite + Unpin + Send + 'static>(write: W) -> Self {
    let write: Box<DynWrite> = Box::new(write);
    let framed =
      tokio_util::codec::FramedWrite::new(write, MsgEncoder::<T>::new());

    MsgSender { writer: framed }
  }
}

impl<T: Serialize + DeserializeOwned + Debug> MsgSender<T> {
  pub async fn send(&mut self, msg: T) -> anyhow::Result<()> {
    self.writer.send(msg).await?;
    Ok(())
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
    // Reclaim excess capacity from large messages to prevent long-term growth.
    // After clear(), len is always 0 so the ratio check is always true, but
    // we include it for consistency with the shrink pattern used elsewhere.
    const ENCODE_BUF_SHRINK_THRESHOLD: usize = 8 * 1024;
    if self.buf.capacity() > ENCODE_BUF_SHRINK_THRESHOLD
      && self.buf.len() < self.buf.capacity() / 4
    {
      self.buf.shrink_to(ENCODE_BUF_SHRINK_THRESHOLD);
    }
    Ok(())
  }
}
