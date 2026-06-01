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
    loop {
      let len = match self.state {
        DecoderState::Header => {
          if src.len() < 4 {
            return Ok(None);
          }
          let len = src.get_u32() as usize;
          self.state = DecoderState::Data(len);
          len
        }
        DecoderState::Data(len) => len,
      };

      if src.len() < len {
        return Ok(None);
      }

      let frame = src.split_to(len);
      self.state = DecoderState::Header;

      match bincode::deserialize::<T>(&frame) {
        Ok(msg) => return Ok(Some(msg)),
        Err(err) => {
          log::warn!("ipc: skipping undecodable frame ({len} bytes): {err}");
          continue;
        }
      }
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

#[cfg(test)]
mod tests {
  use bytes::{BufMut, BytesMut};
  use tokio_util::codec::Decoder;

  use super::MsgDecoder;

  fn frame(payload: &[u8]) -> BytesMut {
    let mut b = BytesMut::new();
    b.put_u32(payload.len() as u32);
    b.extend_from_slice(payload);
    b
  }

  #[test]
  fn decode_skips_undecodable_frame() {
    let mut decoder = MsgDecoder::<bool>::new();
    let mut buf = BytesMut::new();
    buf.unsplit(frame(&bincode::serialize(&true).unwrap())); // good
    buf.unsplit(frame(&[0x05])); // bad: not a valid bool
    buf.unsplit(frame(&bincode::serialize(&false).unwrap())); // good

    assert_eq!(decoder.decode(&mut buf).unwrap(), Some(true));
    assert_eq!(decoder.decode(&mut buf).unwrap(), Some(false));
    assert_eq!(decoder.decode(&mut buf).unwrap(), None);
  }
}
