use std::fmt::Debug;

use crossterm::{event::Event, style::Attribute};
use serde::{Deserialize, Serialize};
use tui::{backend::Backend, style::Modifier};

use crate::{error::ResultLogger, host::sender::MsgSender};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SrvToClt {
  Print(String),
  SetAttr(Attribute),
  SetFg(Color),
  SetBg(Color),
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

impl From<Color> for crossterm::style::Color {
  fn from(value: Color) -> Self {
    use crossterm::style::Color as C;
    match value {
      Color::Reset => C::Reset,
      Color::Black => C::Black,
      Color::Red => C::DarkRed,
      Color::Green => C::DarkGreen,
      Color::Yellow => C::DarkYellow,
      Color::Blue => C::DarkBlue,
      Color::Magenta => C::DarkMagenta,
      Color::Cyan => C::DarkCyan,
      Color::Gray => C::DarkGrey,
      Color::DarkGray => C::DarkGrey,
      Color::LightRed => C::Red,
      Color::LightGreen => C::Green,
      Color::LightYellow => C::Yellow,
      Color::LightBlue => C::Blue,
      Color::LightMagenta => C::Magenta,
      Color::LightCyan => C::Cyan,
      Color::White => C::White,
      Color::Rgb(r, g, b) => C::Rgb { r, g, b },
      Color::Indexed(idx) => C::AnsiValue(idx),
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
    let mut fg = tui::style::Color::Reset;
    let mut bg = tui::style::Color::Reset;
    let mut modifier = Modifier::empty();
    let mut last_pos: Option<tui::layout::Position> = None;
    for (x, y, cell) in content {
      // Move the cursor if the previous location was not (x - 1, y)
      if !matches!(last_pos, Some(p) if x == p.x + 1 && y == p.y) {
        self.send(SrvToClt::SetCursor { x, y });
      }
      last_pos = Some(tui::layout::Position { x, y });
      if cell.modifier != modifier {
        let removed = modifier - cell.modifier;
        let added = cell.modifier - modifier;

        if removed.contains(Modifier::REVERSED) {
          self.send(SrvToClt::SetAttr(Attribute::NoReverse));
        }
        if removed.contains(Modifier::BOLD) || removed.contains(Modifier::DIM) {
          // Bold and Dim are both reset by applying the Normal intensity
          self.send(SrvToClt::SetAttr(Attribute::NormalIntensity));

          // The remaining Bold and Dim attributes must be
          // reapplied after the intensity reset above.
          if cell.modifier.contains(Modifier::DIM) {
            self.send(SrvToClt::SetAttr(Attribute::Dim));
          }

          if cell.modifier.contains(Modifier::BOLD) {
            self.send(SrvToClt::SetAttr(Attribute::Bold));
          }
        }
        if removed.contains(Modifier::ITALIC) {
          self.send(SrvToClt::SetAttr(Attribute::NoItalic));
        }
        if removed.contains(Modifier::UNDERLINED) {
          self.send(SrvToClt::SetAttr(Attribute::NoUnderline));
        }
        if removed.contains(Modifier::CROSSED_OUT) {
          self.send(SrvToClt::SetAttr(Attribute::NotCrossedOut));
        }
        if removed.contains(Modifier::SLOW_BLINK)
          || removed.contains(Modifier::RAPID_BLINK)
        {
          self.send(SrvToClt::SetAttr(Attribute::NoBlink));
        }

        if added.contains(Modifier::REVERSED) {
          self.send(SrvToClt::SetAttr(Attribute::Reverse));
        }
        if added.contains(Modifier::BOLD) {
          self.send(SrvToClt::SetAttr(Attribute::Bold));
        }
        if added.contains(Modifier::ITALIC) {
          self.send(SrvToClt::SetAttr(Attribute::Italic));
        }
        if added.contains(Modifier::UNDERLINED) {
          self.send(SrvToClt::SetAttr(Attribute::Underlined));
        }
        if added.contains(Modifier::DIM) {
          self.send(SrvToClt::SetAttr(Attribute::Dim));
        }
        if added.contains(Modifier::CROSSED_OUT) {
          self.send(SrvToClt::SetAttr(Attribute::CrossedOut));
        }
        if added.contains(Modifier::SLOW_BLINK) {
          self.send(SrvToClt::SetAttr(Attribute::SlowBlink));
        }
        if added.contains(Modifier::RAPID_BLINK) {
          self.send(SrvToClt::SetAttr(Attribute::RapidBlink));
        }

        modifier = cell.modifier;
      }
      if cell.fg != fg || cell.bg != bg {
        self.send(SrvToClt::SetFg(cell.fg.into()));
        self.send(SrvToClt::SetBg(cell.bg.into()));
        fg = cell.fg;
        bg = cell.bg;
      }

      self.send(SrvToClt::Print(cell.symbol().to_string()));
    }

    self.send(SrvToClt::SetFg(Color::Reset));
    self.send(SrvToClt::SetBg(Color::Reset));
    self.send(SrvToClt::SetAttr(Attribute::Reset));

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

  fn size(&self) -> Result<tui::layout::Size, std::io::Error> {
    let rect = tui::layout::Size::new(self.width, self.height);
    Ok(rect)
  }

  fn flush(&mut self) -> Result<(), std::io::Error> {
    self.send(SrvToClt::Flush);
    Ok(())
  }

  fn get_cursor_position(&mut self) -> std::io::Result<tui::prelude::Position> {
    // Only called for Viewport::Inline
    log::error!("ProxyBackend::get_cursor_position() should not be called.");
    Ok(Default::default())
  }

  fn set_cursor_position<P: Into<tui::prelude::Position>>(
    &mut self,
    position: P,
  ) -> std::io::Result<()> {
    let pos: tui::prelude::Position = position.into();
    self.send(SrvToClt::SetCursor { x: pos.x, y: pos.y });
    Ok(())
  }
}
