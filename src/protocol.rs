use std::{fmt::Debug, marker::PhantomData};

use bytes::{Buf, BufMut, BytesMut};
use crossterm::event::Event;
use futures::{SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tui::{
  backend::Backend,
  style::{Color, Modifier},
};

use crate::error::ResultLogger;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SrvToClt {
  Draw { cells: Vec<(u16, u16, Cell)> },
  SetCursor { x: u16, y: u16 },
  ShowCursor,
  HideCursor,
  CursorShape(CursorStyle),
  Clear,
  Flush,
  Quit,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CursorStyle {
  Default = 0,
  BlinkingBlock = 1,
  SteadyBlock = 2,
  BlinkingUnderline = 3,
  SteadyUnderline = 4,
  BlinkingBar = 5,
  SteadyBar = 6,
}

impl From<termwiz::escape::csi::CursorStyle> for CursorStyle {
  fn from(value: termwiz::escape::csi::CursorStyle) -> Self {
    use termwiz::escape::csi::CursorStyle as CS;

    match value {
      CS::Default => Self::Default,
      CS::BlinkingBlock => Self::BlinkingBlock,
      CS::SteadyBlock => Self::SteadyBlock,
      CS::BlinkingUnderline => Self::BlinkingUnderline,
      CS::SteadyUnderline => Self::SteadyUnderline,
      CS::BlinkingBar => Self::BlinkingBar,
      CS::SteadyBar => Self::SteadyBar,
    }
  }
}

impl Default for CursorStyle {
  fn default() -> CursorStyle {
    CursorStyle::Default
  }
}

#[derive(Debug, Deserialize, Serialize)]
pub enum CltToSrv {
  Init { width: u16, height: u16 },
  Key(Event),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Cell {
  str: String,
  fg: Color,
  bg: Color,
  mods: Modifier,
}

impl From<&Cell> for tui::buffer::Cell {
  fn from(value: &Cell) -> Self {
    tui::buffer::Cell {
      symbol: value.str.clone(),
      fg: value.fg,
      bg: value.bg,
      underline_color: value.fg,
      modifier: value.mods,
      skip: false,
    }
  }
}

impl From<&tui::buffer::Cell> for Cell {
  fn from(value: &tui::buffer::Cell) -> Self {
    Cell {
      str: value.symbol.clone(),
      fg: value.fg,
      bg: value.bg,
      mods: value.modifier,
    }
  }
}

pub struct ProxyBackend {
  pub tx: MsgSender<SrvToClt>,
  pub height: u16,
  pub width: u16,
}

impl ProxyBackend {
  fn send(&mut self, msg: SrvToClt) {
    self.tx.send(msg).log_ignore()
  }

  pub fn set_size(&mut self, width: u16, height: u16) {
    self.width = width;
    self.height = height;
  }
}

impl Backend for ProxyBackend {
  fn draw<'a, I>(&mut self, content: I) -> Result<(), std::io::Error>
  where
    I: Iterator<Item = (u16, u16, &'a tui::buffer::Cell)>,
  {
    let msg = SrvToClt::Draw {
      cells: content
        .map(|(a, b, cell)| (a, b, Cell::from(cell)))
        .collect(),
    };
    self.send(msg);
    Ok(())
  }

  fn hide_cursor(&mut self) -> Result<(), std::io::Error> {
    self.send(SrvToClt::HideCursor);
    Ok(())
  }

  fn show_cursor(&mut self) -> Result<(), std::io::Error> {
    self.send(SrvToClt::ShowCursor);
    Ok(())
  }

  fn get_cursor(&mut self) -> Result<(u16, u16), std::io::Error> {
    Ok((1, 1))
  }

  fn set_cursor(&mut self, x: u16, y: u16) -> Result<(), std::io::Error> {
    self.send(SrvToClt::SetCursor { x, y });
    Ok(())
  }

  fn clear(&mut self) -> Result<(), std::io::Error> {
    self.send(SrvToClt::Clear);
    Ok(())
  }

  fn size(&self) -> Result<tui::layout::Rect, std::io::Error> {
    let rect = tui::layout::Rect::new(0, 0, self.width, self.height);
    Ok(rect)
  }

  fn flush(&mut self) -> Result<(), std::io::Error> {
    self.send(SrvToClt::Flush);
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
      t: PhantomData::default(),
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

#[derive(Clone)]
pub struct MsgSender<T: Serialize> {
  sender: tokio::sync::mpsc::UnboundedSender<T>,
}

impl<T: Serialize + Send + Debug + 'static> MsgSender<T> {
  pub fn new(write: tokio::net::unix::OwnedWriteHalf) -> Self {
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

pub struct MsgReceiver<T: DeserializeOwned> {
  receiver: tokio::sync::mpsc::UnboundedReceiver<T>,
}

impl<T: DeserializeOwned + Send + 'static> MsgReceiver<T> {
  pub fn new(read: tokio::net::unix::OwnedReadHalf) -> Self {
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
