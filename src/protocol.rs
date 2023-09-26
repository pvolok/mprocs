use std::fmt::Debug;

use crossterm::event::Event;
use serde::{Deserialize, Serialize};
use tui::{
  backend::Backend,
  style::{Color, Modifier},
};

use crate::{error::ResultLogger, host::sender::MsgSender};

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

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
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
