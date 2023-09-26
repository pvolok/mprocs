use std::marker::PhantomData;

use bytes::{Buf, BytesMut};
use futures::StreamExt;
use serde::de::DeserializeOwned;
use tokio::io::AsyncRead;

struct MsgDecoder<T: DeserializeOwned> {
  state: DecoderState,
  t: PhantomData<T>,
}

enum DecoderState {
  Header,
  Data(usize),
}

impl<T: DeserializeOwned> MsgDecoder<T> {
  pub fn new() -> MsgDecoder<T> {
    MsgDecoder {
      state: DecoderState::Header,
      t: PhantomData::default(),
    }
  }
}

impl<T: DeserializeOwned> tokio_util::codec::Decoder for MsgDecoder<T> {
  type Item = T;

  type Error = bincode::Error;

  fn decode(
    &mut self,
    src: &mut BytesMut,
  ) -> Result<Option<Self::Item>, Self::Error> {
    if let DecoderState::Header = self.state {
      let len = if src.len() >= 4 {
        src.get_u32() as usize
      } else {
        return Ok(None);
      };
      self.state = DecoderState::Data(len);
    }
    let len = match self.state {
      DecoderState::Header => {
        let len = if src.len() >= 4 {
          src.get_u32() as usize
        } else {
          return Ok(None);
        };
        self.state = DecoderState::Data(len);
        len
      }
      DecoderState::Data(len) => len,
    };

    if src.len() >= len {
      let msg: T = bincode::deserialize(&src[..len])?;
      if src.len() == len {
        src.clear();
      } else {
        src.advance(len);
      }
      self.state = DecoderState::Header;
      Ok(Some(msg))
    } else {
      return Ok(None);
    }
  }
}

pub struct MsgReceiver<T: DeserializeOwned> {
  receiver: tokio::sync::mpsc::UnboundedReceiver<T>,
}

impl<T: DeserializeOwned + Send + 'static> MsgReceiver<T> {
  pub fn new<R: AsyncRead + Unpin + Send + 'static>(read: R) -> Self {
    let mut framed =
      tokio_util::codec::FramedRead::new(read, MsgDecoder::<T>::new());

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
      loop {
        let msg = framed.next().await;
        let msg = match msg {
          Some(Ok(msg)) => msg,
          _ => break,
        };
        match tx.send(msg) {
          Ok(()) => (),
          Err(_) => break,
        };
      }
    });

    MsgReceiver { receiver: rx }
  }
}

impl<T: DeserializeOwned> MsgReceiver<T> {
  pub async fn recv(&mut self) -> Option<Result<T, bincode::Error>> {
    let msg = self.receiver.recv().await;
    msg.map(|x| Ok(x))
  }
}
