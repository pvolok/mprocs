use std::marker::PhantomData;

use bytes::{Buf, BytesMut};
use futures::StreamExt;
use serde::de::DeserializeOwned;
use tokio::io::AsyncRead;
use tokio_util::codec::FramedRead;

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
      t: PhantomData,
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
      Ok(None)
    }
  }
}

type DynRead = dyn AsyncRead + Unpin + Send + 'static;

pub struct MsgReceiver<T: DeserializeOwned> {
  reader: FramedRead<Box<DynRead>, MsgDecoder<T>>,
}

impl<T: DeserializeOwned + Send + 'static> MsgReceiver<T> {
  pub fn new<R: AsyncRead + Unpin + Send + 'static>(read: R) -> Self {
    let reader = tokio_util::codec::FramedRead::new(
      Box::new(read) as Box<DynRead>,
      MsgDecoder::<T>::new(),
    );

    MsgReceiver { reader }
  }
}

impl<T: DeserializeOwned> MsgReceiver<T> {
  pub async fn recv(&mut self) -> Option<Result<T, bincode::Error>> {
    self.reader.next().await
  }
}
