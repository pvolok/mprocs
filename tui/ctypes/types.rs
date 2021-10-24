#[derive(Clone, Copy)]
#[repr(C)]
pub struct Rect {
  pub x: u16,
  pub y: u16,
  pub w: u16,
  pub h: u16,
}

impl From<tui::layout::Rect> for Rect {
   fn from (x: tui::layout::Rect) -> Self {
      Rect {
         x: x.x.into(),
         y: x.y.into(),
         w: x.width.into(),
         h: x.height.into(),
      }
   }
}

impl From<Rect> for tui::layout::Rect {
   fn from (x: Rect) -> Self {
      tui::layout::Rect {
         x: x.x.into(),
         y: x.y.into(),
         width: x.w.into(),
         height: x.h.into(),
      }
   }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub enum Constraint {
  Percentage(u16),
  Ratio(u32, u32),
  Length(u16),
  Min(u16),
  Max(u16),
}

impl From<tui::layout::Constraint> for Constraint {
   fn from (x: tui::layout::Constraint) -> Self {
      match x {
         tui::layout::Constraint::Percentage(x0) => Constraint::Percentage(x0),
         tui::layout::Constraint::Ratio(x0, x1) => Constraint::Ratio(x0, x1),
         tui::layout::Constraint::Length(x0) => Constraint::Length(x0),
         tui::layout::Constraint::Min(x0) => Constraint::Min(x0),
         tui::layout::Constraint::Max(x0) => Constraint::Max(x0),
      }
   }
}

impl From<Constraint> for tui::layout::Constraint {
   fn from (x: Constraint) -> Self {
      match x {
         Constraint::Percentage(x0) => tui::layout::Constraint::Percentage(x0),
         Constraint::Ratio(x0, x1) => tui::layout::Constraint::Ratio(x0, x1),
         Constraint::Length(x0) => tui::layout::Constraint::Length(x0),
         Constraint::Min(x0) => tui::layout::Constraint::Min(x0),
         Constraint::Max(x0) => tui::layout::Constraint::Max(x0),
      }
   }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub enum Direction {
  Horizontal,
  Vertical,
}

impl From<tui::layout::Direction> for Direction {
   fn from (x: tui::layout::Direction) -> Self {
      match x {
         tui::layout::Direction::Horizontal => Direction::Horizontal,
         tui::layout::Direction::Vertical => Direction::Vertical,
      }
   }
}

impl From<Direction> for tui::layout::Direction {
   fn from (x: Direction) -> Self {
      match x {
         Direction::Horizontal => tui::layout::Direction::Horizontal,
         Direction::Vertical => tui::layout::Direction::Vertical,
      }
   }
}

#[derive(Clone, Copy)]
#[repr(C)]
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

impl From<tui::style::Color> for Color {
   fn from (x: tui::style::Color) -> Self {
      match x {
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
         tui::style::Color::Rgb(x0, x1, x2) => Color::Rgb(x0, x1, x2),
         tui::style::Color::Indexed(x0) => Color::Indexed(x0),
      }
   }
}

impl From<Color> for tui::style::Color {
   fn from (x: Color) -> Self {
      match x {
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
         Color::Rgb(x0, x1, x2) => tui::style::Color::Rgb(x0, x1, x2),
         Color::Indexed(x0) => tui::style::Color::Indexed(x0),
      }
   }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub enum ColorOpt {
  Some(Color),
  None,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Style {
  pub fg: ColorOpt,
  pub bg: ColorOpt,
  pub add_modifier: u16,
  pub sub_modifier: u16,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub enum KeyCode {
  Backspace,
  Enter,
  Left,
  Right,
  Up,
  Down,
  Home,
  End,
  PageUp,
  PageDown,
  Tab,
  BackTab,
  Delete,
  Insert,
  F(u8),
  Char(u32),
  Null,
  Esc,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct KeyMods {
  pub shift: u8,
  pub control: u8,
  pub alt: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct KeyEvent {
  pub code: KeyCode,
  pub modifiers: KeyMods,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub enum MouseButton {
  Left,
  Right,
  Middle,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub enum MouseEventKind {
  Down(MouseButton),
  Up(MouseButton),
  Drag(MouseButton),
  Moved,
  ScrollDown,
  ScrollUp,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct MouseEvent {
  pub kind: MouseEventKind,
  pub column: u16,
  pub row: u16,
  pub modifiers: KeyMods,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub enum Event {
  Key(KeyEvent),
  Mouse(MouseEvent),
  Resize(u16, u16),
  Finished,
}

