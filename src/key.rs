use anyhow::bail;
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

static KEYS: phf::Map<&'static str, KeyCode> = phf::phf_map! {
  "bs" => KeyCode::Backspace,
  "enter" => KeyCode::Enter,
  "left" => KeyCode::Left,
  "right" => KeyCode::Right,
  "up" => KeyCode::Up,
  "down" => KeyCode::Down,
  "home" => KeyCode::Home,
  "end" => KeyCode::End,
  "pageup" => KeyCode::PageUp,
  "pagedown" => KeyCode::PageDown,
  "tab" => KeyCode::Tab,
  "del" => KeyCode::Delete,
  "insert" => KeyCode::Insert,
  "nul" => KeyCode::Null,
  "esc" => KeyCode::Esc,

  "space" => KeyCode::Char(' '),

  "lt" => KeyCode::Char('<'),
  "gt" => KeyCode::Char('>'),
  "minus" => KeyCode::Char('-'),

  "f1" => KeyCode::F(1),
  "f2" => KeyCode::F(2),
  "f3" => KeyCode::F(3),
  "f4" => KeyCode::F(4),
  "f5" => KeyCode::F(5),
  "f6" => KeyCode::F(6),
  "f7" => KeyCode::F(7),
  "f8" => KeyCode::F(8),
  "f9" => KeyCode::F(9),
  "f10" => KeyCode::F(10),
  "f11" => KeyCode::F(11),
  "f12" => KeyCode::F(12),
};

static SPECIAL_CHARS: phf::Map<char, &str> = phf::phf_map! {
  ' ' => "Space",

  '<' => "LT",
  '>' => "GT",
  '-' => "Minus",
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Key {
  pub code: KeyCode,
  pub mods: KeyMods,
  pub kind: KeyEventKind,
  pub state: KeyEventState,
}

impl Key {
  pub fn new(code: KeyCode, mods: KeyMods) -> Key {
    Self {
      code,
      mods,
      kind: KeyEventKind::Press,
      state: KeyEventState::empty(),
    }
  }

  pub const fn new_with_kind(
    code: KeyCode,
    mods: KeyMods,
    kind: KeyEventKind,
  ) -> Key {
    Key {
      code,
      mods,
      kind,
      state: KeyEventState::empty(),
    }
  }

  pub fn parse(text: &str) -> anyhow::Result<Key> {
    KeyParser::parse(text)
  }

  pub fn code(&self) -> KeyCode {
    self.code
  }

  pub fn mods(&self) -> KeyMods {
    self.mods
  }
}

impl From<KeyCode> for Key {
  fn from(code: KeyCode) -> Self {
    Key::new(code, KeyMods::NONE)
  }
}

impl ToString for Key {
  fn to_string(&self) -> String {
    let mut buf = String::new();

    buf.push('<');

    let mods = self.mods;
    if mods.intersects(KeyMods::CONTROL) {
      buf.push_str("C-");
    }
    if mods.intersects(KeyMods::SHIFT) {
      buf.push_str("S-");
    }
    if mods.intersects(KeyMods::ALT) {
      buf.push_str("M-");
    }

    match self.code {
      KeyCode::Backspace => buf.push_str("BS"),
      KeyCode::Enter => buf.push_str("Enter"),
      KeyCode::Left => buf.push_str("Left"),
      KeyCode::Right => buf.push_str("Right"),
      KeyCode::Up => buf.push_str("Up"),
      KeyCode::Down => buf.push_str("Down"),
      KeyCode::Home => buf.push_str("Home"),
      KeyCode::End => buf.push_str("End"),
      KeyCode::PageUp => buf.push_str("PageUp"),
      KeyCode::PageDown => buf.push_str("PageDown"),
      KeyCode::Tab => buf.push_str("Tab"),
      KeyCode::Delete => buf.push_str("Del"),
      KeyCode::Insert => buf.push_str("Insert"),
      KeyCode::F(n) => {
        buf.push('F');
        buf.push_str(n.to_string().as_str());
      }
      KeyCode::Char(ch) => {
        if let Some(s) = SPECIAL_CHARS.get(&ch) {
          buf.push_str(s)
        } else {
          buf.push(ch)
        }
      }
      KeyCode::Null => buf.push_str("Nul"),
      KeyCode::Esc => buf.push_str("Esc"),
      KeyCode::CapsLock => buf.push_str("CapsLock"),
      KeyCode::ScrollLock => buf.push_str("ScrollLock"),
      KeyCode::NumLock => buf.push_str("NumLock"),
      KeyCode::PrintScreen => buf.push_str("PrintScreen"),
      KeyCode::Pause => buf.push_str("Pause"),
      KeyCode::Menu => buf.push_str("Menu"),
      KeyCode::KeypadBegin => buf.push_str("KeypadBegin"),
      KeyCode::Media(code) => {
        let s = match code {
          MediaKeyCode::Play => "MediaPlay",
          MediaKeyCode::Pause => "MediaPause",
          MediaKeyCode::PlayPause => "MediaPlayPause",
          MediaKeyCode::Reverse => "MediaReverse",
          MediaKeyCode::Stop => "MediaStop",
          MediaKeyCode::FastForward => "MediaFastForward",
          MediaKeyCode::Rewind => "MediaRewind",
          MediaKeyCode::Next => "MediaNext",
          MediaKeyCode::Prev => "MediaPrev",
          MediaKeyCode::Record => "MediaRecord",
          MediaKeyCode::VolumeDown => "VolumeDown",
          MediaKeyCode::VolumeUp => "VolumeUp",
          MediaKeyCode::VolumeMute => "VolumeMute",
        };
        buf.push_str(s);
      }
      KeyCode::Modifier(_code) => {
        // TODO
        buf.push_str("Nul");
      }
    }

    buf.push('>');

    buf
  }
}

impl Serialize for Key {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    serializer.serialize_str(self.to_string().as_str())
  }
}

impl<'de> Deserialize<'de> for Key {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let text = String::deserialize(deserializer)?;
    Key::parse(text.as_str())
      .map_err(|err| serde::de::Error::custom(err.to_string()))
  }
}

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
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
  Delete,
  Insert,
  F(u8),
  Char(char),
  Null,
  Esc,

  CapsLock,
  ScrollLock,
  NumLock,
  PrintScreen,
  Pause,
  Menu,
  /// The "Begin" key (on X11 mapped to the 5 key when Num Lock is turned on).
  KeypadBegin,
  Media(MediaKeyCode),
  Modifier(ModKeyCode),
}

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum KeyEventKind {
  Press,
  Repeat,
  Release,
}

bitflags! {
  #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
  pub struct KeyEventState: u8 {
    /// The key event origins from the keypad.
    const KEYPAD = 0b0000_0001;
    /// Caps Lock was enabled for this key event.
    ///
    /// **Note:** this is set for the initial press of Caps Lock itself.
    const CAPS_LOCK = 0b0000_0010;
    /// Num Lock was enabled for this key event.
    ///
    /// **Note:** this is set for the initial press of Num Lock itself.
    const NUM_LOCK = 0b0000_0100;
    const NONE = 0b0000_0000;
  }
}

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum MediaKeyCode {
  Play,
  Pause,
  PlayPause,
  Reverse,
  Stop,
  FastForward,
  Rewind,
  Next,
  Prev,
  Record,
  VolumeDown,
  VolumeUp,
  VolumeMute,
}

/// Represents a modifier key (as part of [`KeyCode::Modifier`]).
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum ModKeyCode {
  LeftShift,
  LeftControl,
  LeftAlt,
  LeftSuper,
  LeftHyper,
  LeftMeta,
  RightShift,
  RightControl,
  RightAlt,
  RightSuper,
  RightHyper,
  RightMeta,
  IsoLevel3Shift,
  IsoLevel5Shift,
}

bitflags! {
  #[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
  pub struct KeyMods: u8 {
    const SHIFT = 0b0000_0001;
    const CONTROL = 0b0000_0010;
    const ALT = 0b0000_0100;
    const SUPER = 0b0000_1000;
    const HYPER = 0b0001_0000;
    const META = 0b0010_0000;
    const NONE = 0b0000_0000;
  }
}

struct KeyParser<'a> {
  text: &'a str,
  pos: usize,
}

impl KeyParser<'_> {
  fn parse(text: &str) -> anyhow::Result<Key> {
    let mut parser = KeyParser { text, pos: 0 };

    parser.expect("<")?;
    let mods = parser.take_mods()?;
    let code = {
      let code = parser.take_key()?;
      if let Some(code) = KEYS.get(code.to_ascii_lowercase().as_str()) {
        *code
      } else if code.len() == 1 {
        KeyCode::Char(code.chars().next().unwrap())
      } else {
        bail!("Wrong key code: \"{}\"", code);
      }
    };
    parser.expect(">")?;

    Ok(Key::new(code, mods))
  }

  fn expect(&mut self, s: &str) -> anyhow::Result<()> {
    let next_pos = self.pos + s.len();
    if next_pos > self.text.len() {
      bail!("Expected \"{}\"", s);
    }
    let subtext = &self.text[self.pos..next_pos];
    if subtext != s {
      bail!("Expected \"{}\"", s);
    }

    self.pos = next_pos;
    Ok(())
  }

  fn take_key(&mut self) -> anyhow::Result<&str> {
    let mut next_pos = self.pos;
    let chars = self.text[self.pos..].chars();
    for ch in chars {
      if ch == '>' || ch == ' ' {
        break;
      } else if ch.is_control() {
        bail!(
          "Unexpected control characted in key code position: 0x{:X}.",
          ch as usize
        )
      } else {
        next_pos += ch.len_utf8();
      }
    }
    let start = self.pos;
    self.pos = next_pos;
    Ok(&self.text[start..next_pos])
  }

  fn take_mods(&mut self) -> anyhow::Result<KeyMods> {
    let mut mods = KeyMods::NONE;
    let mut pos = self.pos;
    while pos + 1 < self.text.len() && &self.text[pos + 1..pos + 2] == "-" {
      match &self.text[pos..pos + 1] {
        "c" | "C" => mods = mods.union(KeyMods::CONTROL),
        "s" | "S" => mods = mods.union(KeyMods::SHIFT),
        "m" | "M" => mods = mods.union(KeyMods::ALT),
        ch => bail!("Wrong key modifier: \"{}\"", ch),
      }
      pos += 2;
    }
    self.pos = pos;
    Ok(mods)
  }
}

#[cfg(test)]
mod tests {
  use assert_matches::assert_matches;

  use super::*;

  #[test]
  fn parse() {
    assert_eq!(
      Key::parse("<Tab>").unwrap(),
      Key::new(KeyCode::Tab, KeyMods::NONE)
    );
    assert_eq!(
      Key::parse("<C-Enter>").unwrap(),
      Key::new(KeyCode::Enter, KeyMods::CONTROL)
    );
    assert_eq!(
      Key::parse("<C-Esc>").unwrap(),
      Key::new(KeyCode::Esc, KeyMods::CONTROL)
    );

    assert_eq!(
      Key::parse("<F1>").unwrap(),
      Key::new(KeyCode::F(1), KeyMods::NONE)
    );
    assert_eq!(
      Key::parse("<f12>").unwrap(),
      Key::new(KeyCode::F(12), KeyMods::NONE)
    );
    assert_matches!(Key::parse("<F13>"), Err(_));

    assert_eq!(
      Key::parse("<a>").unwrap(),
      Key::new(KeyCode::Char('a'), KeyMods::NONE)
    );
    assert_eq!(
      Key::parse("<C-a>").unwrap(),
      Key::new(KeyCode::Char('a'), KeyMods::CONTROL)
    );
    assert_eq!(
      Key::parse("<C-M-a>").unwrap(),
      Key::new(KeyCode::Char('a'), KeyMods::CONTROL | KeyMods::ALT)
    );
  }

  #[test]
  fn parse_and_print() {
    fn in_out(key: &str) {
      assert_eq!(Key::parse(key).unwrap().to_string(), key);
    }

    in_out("<BS>");
    in_out("<Enter>");
    in_out("<Left>");
    in_out("<Right>");
    in_out("<Up>");
    in_out("<Down>");
    in_out("<Home>");
    in_out("<End>");
    in_out("<PageUp>");
    in_out("<PageDown>");
    in_out("<Tab>");
    in_out("<Del>");
    in_out("<Insert>");
    in_out("<Nul>");
    in_out("<Esc>");

    in_out("<a>");
    in_out("<A>");

    in_out("<C-a>");
    in_out("<C-M-a>");
    in_out("<C-Enter>");

    in_out("<Minus>");
    in_out("<LT>");
    in_out("<GT>");
  }
}
