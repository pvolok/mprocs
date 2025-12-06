use std::fmt::Debug;

use crossterm::event::Event;
use serde::{Deserialize, Serialize};
use termwiz::{
  cell::{AttributeChange, Blink, Intensity, Underline},
  color::{ColorAttribute, SrgbaTuple},
};
use tui::{backend::Backend, style::Modifier};

use crate::{error::ResultLogger, host::sender::MsgSender};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SrvToClt {
  Print(String),
  SetAttr(AttributeChange),
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

impl From<Color> for termwiz::color::ColorSpec {
  fn from(value: Color) -> Self {
    use termwiz::color::ColorSpec as C;
    match value {
      Color::Reset => C::Default,
      Color::Black => C::PaletteIndex(0),
      Color::Red => C::PaletteIndex(1),
      Color::Green => C::PaletteIndex(2),
      Color::Yellow => C::PaletteIndex(3),
      Color::Blue => C::PaletteIndex(4),
      Color::Magenta => C::PaletteIndex(6),
      Color::Cyan => C::PaletteIndex(6),
      Color::Gray => C::PaletteIndex(7),
      Color::DarkGray => C::PaletteIndex(8),
      Color::LightRed => C::PaletteIndex(9),
      Color::LightGreen => C::PaletteIndex(10),
      Color::LightYellow => C::PaletteIndex(11),
      Color::LightBlue => C::PaletteIndex(12),
      Color::LightMagenta => C::PaletteIndex(13),
      Color::LightCyan => C::PaletteIndex(14),
      Color::White => C::PaletteIndex(15),
      Color::Rgb(r, g, b) => C::TrueColor(termwiz::color::SrgbaTuple(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        1.0,
      )),
      Color::Indexed(idx) => C::PaletteIndex(idx),
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
          self.send(SrvToClt::SetAttr(AttributeChange::Reverse(false)));
        }
        if removed.contains(Modifier::BOLD) || removed.contains(Modifier::DIM) {
          // Bold and Dim are both reset by applying the Normal intensity
          self.send(SrvToClt::SetAttr(AttributeChange::Intensity(
            Intensity::Normal,
          )));

          // The remaining Bold and Dim attributes must be
          // reapplied after the intensity reset above.
          if cell.modifier.contains(Modifier::DIM) {
            self.send(SrvToClt::SetAttr(AttributeChange::Intensity(
              Intensity::Half,
            )));
          }

          if cell.modifier.contains(Modifier::BOLD) {
            self.send(SrvToClt::SetAttr(AttributeChange::Intensity(
              Intensity::Bold,
            )));
          }
        }
        if removed.contains(Modifier::ITALIC) {
          self.send(SrvToClt::SetAttr(AttributeChange::Italic(false)));
        }
        if removed.contains(Modifier::UNDERLINED) {
          self.send(SrvToClt::SetAttr(AttributeChange::Underline(
            Underline::None,
          )));
        }
        if removed.contains(Modifier::CROSSED_OUT) {
          self.send(SrvToClt::SetAttr(AttributeChange::StrikeThrough(false)));
        }
        if removed.contains(Modifier::SLOW_BLINK)
          || removed.contains(Modifier::RAPID_BLINK)
        {
          self.send(SrvToClt::SetAttr(AttributeChange::Blink(Blink::None)));
        }

        if added.contains(Modifier::REVERSED) {
          self.send(SrvToClt::SetAttr(AttributeChange::Reverse(true)));
        }
        if added.contains(Modifier::BOLD) {
          self.send(SrvToClt::SetAttr(AttributeChange::Intensity(
            Intensity::Bold,
          )));
        }
        if added.contains(Modifier::ITALIC) {
          self.send(SrvToClt::SetAttr(AttributeChange::Italic(true)));
        }
        if added.contains(Modifier::UNDERLINED) {
          self.send(SrvToClt::SetAttr(AttributeChange::Underline(
            Underline::Single,
          )));
        }
        if added.contains(Modifier::DIM) {
          self.send(SrvToClt::SetAttr(AttributeChange::Intensity(
            Intensity::Half,
          )));
        }
        if added.contains(Modifier::CROSSED_OUT) {
          self.send(SrvToClt::SetAttr(AttributeChange::StrikeThrough(true)));
        }
        if added.contains(Modifier::SLOW_BLINK) {
          self.send(SrvToClt::SetAttr(AttributeChange::Blink(Blink::Slow)));
        }
        if added.contains(Modifier::RAPID_BLINK) {
          self.send(SrvToClt::SetAttr(AttributeChange::Blink(Blink::Rapid)));
        }

        modifier = cell.modifier;
      }
      if cell.fg != fg || cell.bg != bg {
        self.send(SrvToClt::SetAttr(AttributeChange::Foreground(
          tui_color_to_termwiz_color_attr(cell.fg),
        )));
        self.send(SrvToClt::SetAttr(AttributeChange::Background(
          tui_color_to_termwiz_color_attr(cell.bg),
        )));
        fg = cell.fg;
        bg = cell.bg;
      }

      self.send(SrvToClt::Print(cell.symbol().to_string()));
    }

    self.send(SrvToClt::SetAttr(AttributeChange::Foreground(
      ColorAttribute::Default,
    )));
    self.send(SrvToClt::SetAttr(AttributeChange::Background(
      ColorAttribute::Default,
    )));

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

fn tui_color_to_termwiz_color_attr(
  color: tui::style::Color,
) -> termwiz::color::ColorAttribute {
  match color {
    tui::style::Color::Reset => termwiz::color::ColorAttribute::Default,
    tui::style::Color::Black => termwiz::color::ColorAttribute::PaletteIndex(0),
    tui::style::Color::Red => termwiz::color::ColorAttribute::PaletteIndex(1),
    tui::style::Color::Green => termwiz::color::ColorAttribute::PaletteIndex(2),
    tui::style::Color::Yellow => {
      termwiz::color::ColorAttribute::PaletteIndex(3)
    }
    tui::style::Color::Blue => termwiz::color::ColorAttribute::PaletteIndex(4),
    tui::style::Color::Magenta => {
      termwiz::color::ColorAttribute::PaletteIndex(5)
    }
    tui::style::Color::Cyan => termwiz::color::ColorAttribute::PaletteIndex(6),
    tui::style::Color::Gray => termwiz::color::ColorAttribute::PaletteIndex(7),
    tui::style::Color::DarkGray => {
      termwiz::color::ColorAttribute::PaletteIndex(8)
    }
    tui::style::Color::LightRed => {
      termwiz::color::ColorAttribute::PaletteIndex(9)
    }
    tui::style::Color::LightGreen => {
      termwiz::color::ColorAttribute::PaletteIndex(10)
    }
    tui::style::Color::LightYellow => {
      termwiz::color::ColorAttribute::PaletteIndex(11)
    }
    tui::style::Color::LightBlue => {
      termwiz::color::ColorAttribute::PaletteIndex(12)
    }
    tui::style::Color::LightMagenta => {
      termwiz::color::ColorAttribute::PaletteIndex(13)
    }
    tui::style::Color::LightCyan => {
      termwiz::color::ColorAttribute::PaletteIndex(14)
    }
    tui::style::Color::White => {
      termwiz::color::ColorAttribute::PaletteIndex(15)
    }
    tui::style::Color::Rgb(r, g, b) => {
      termwiz::color::ColorAttribute::TrueColorWithDefaultFallback(SrgbaTuple(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        1.0,
      ))
    }
    tui::style::Color::Indexed(idx) => {
      termwiz::color::ColorAttribute::PaletteIndex(idx)
    }
  }
}
