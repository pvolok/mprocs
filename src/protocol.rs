use std::fmt::Debug;

use crossterm::event::Event;
use serde::{Deserialize, Serialize};
use tui::{backend::Backend, style::Modifier};

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

#[derive(
  Debug, Default, Deserialize, Clone, Copy, PartialEq, Eq, Serialize,
)]
pub enum CursorStyle {
  #[default]
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
  underline_color: Color,
  mods: Modifier,
  skip: bool,
}

impl From<&Cell> for tui::buffer::Cell {
  fn from(value: &Cell) -> Self {
    let mut cell = tui::buffer::Cell::default();
    cell.set_symbol(&value.str);
    cell.set_style(
      tui::style::Style::new()
        .fg(value.fg.into())
        .bg(value.bg.into())
        .underline_color(value.underline_color.into())
        .add_modifier(value.mods),
    );
    cell.set_skip(value.skip);
    cell
  }
}

impl From<&tui::buffer::Cell> for Cell {
  fn from(value: &tui::buffer::Cell) -> Self {
    Cell {
      str: value.symbol().to_string(),
      fg: value.fg.into(),
      bg: value.bg.into(),
      underline_color: value.underline_color.into(),
      mods: value.modifier,
      skip: value.skip,
    }
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Color {
  Reset,
  Black,
  Red,
  Green,
  Yellow,
  Blue,
  Magenta,
  Cyan,
  Gray,
  DarkGray,
  LightRed,
  LightGreen,
  LightYellow,
  LightBlue,
  LightMagenta,
  LightCyan,
  White,
  Rgb(u8, u8, u8),
  Indexed(u8),
}

impl From<Color> for tui::style::Color {
  fn from(value: Color) -> Self {
    match value {
      Color::Reset => tui::style::Color::Reset,
      Color::Black => tui::style::Color::Black,
      Color::Red => tui::style::Color::Red,
      Color::Green => tui::style::Color::Green,
      Color::Yellow => tui::style::Color::Yellow,
      Color::Blue => tui::style::Color::Blue,
      Color::Magenta => tui::style::Color::Magenta,
      Color::Cyan => tui::style::Color::Cyan,
      Color::Gray => tui::style::Color::Gray,
      Color::DarkGray => tui::style::Color::DarkGray,
      Color::LightRed => tui::style::Color::LightRed,
      Color::LightGreen => tui::style::Color::LightGreen,
      Color::LightYellow => tui::style::Color::LightYellow,
      Color::LightBlue => tui::style::Color::LightBlue,
      Color::LightMagenta => tui::style::Color::LightMagenta,
      Color::LightCyan => tui::style::Color::LightCyan,
      Color::White => tui::style::Color::White,
      Color::Rgb(r, g, b) => tui::style::Color::Rgb(r, g, b),
      Color::Indexed(idx) => tui::style::Color::Indexed(idx),
    }
  }
}

impl From<tui::style::Color> for Color {
  fn from(value: tui::style::Color) -> Self {
    match value {
      tui::style::Color::Reset => Color::Reset,
      tui::style::Color::Black => Color::Black,
      tui::style::Color::Red => Color::Red,
      tui::style::Color::Green => Color::Green,
      tui::style::Color::Yellow => Color::Yellow,
      tui::style::Color::Blue => Color::Blue,
      tui::style::Color::Magenta => Color::Magenta,
      tui::style::Color::Cyan => Color::Cyan,
      tui::style::Color::Gray => Color::Gray,
      tui::style::Color::DarkGray => Color::DarkGray,
      tui::style::Color::LightRed => Color::LightRed,
      tui::style::Color::LightGreen => Color::LightGreen,
      tui::style::Color::LightYellow => Color::LightYellow,
      tui::style::Color::LightBlue => Color::LightBlue,
      tui::style::Color::LightMagenta => Color::LightMagenta,
      tui::style::Color::LightCyan => Color::LightCyan,
      tui::style::Color::White => Color::White,
      tui::style::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
      tui::style::Color::Indexed(idx) => Color::Indexed(idx),
    }
  }
}

pub struct ProxyBackend {
  pub tx: MsgSender<SrvToClt>,
  pub height: u16,
  pub width: u16,
  pub x: u16,
  pub y: u16,
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
    Ok((self.x, self.y))
  }

  fn set_cursor(&mut self, x: u16, y: u16) -> Result<(), std::io::Error> {
    self.x = x;
    self.y = y;
    self.send(SrvToClt::SetCursor { x, y });
    Ok(())
  }

  fn clear(&mut self) -> Result<(), std::io::Error> {
    self.send(SrvToClt::Clear);
    Ok(())
  }

  fn window_size(&mut self) -> std::io::Result<tui::backend::WindowSize> {
    let win_size = tui::backend::WindowSize {
      columns_rows: tui::layout::Size {
        width: self.width,
        height: self.height,
      },
      pixels: tui::layout::Size {
        width: 0,
        height: 0,
      },
    };
    Ok(win_size)
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
