use std::fmt;

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

  "capslock" => KeyCode::CapsLock,
  "scrolllock" => KeyCode::ScrollLock,
  "numlock" => KeyCode::NumLock,
  "printscreen" => KeyCode::PrintScreen,
  "pause" => KeyCode::Pause,
  "menu" => KeyCode::Menu,
  "keypadbegin" => KeyCode::KeypadBegin,

  "mediaplay" => KeyCode::Media(MediaKeyCode::Play),
  "mediapause" => KeyCode::Media(MediaKeyCode::Pause),
  "mediaplaypause" => KeyCode::Media(MediaKeyCode::PlayPause),
  "mediareverse" => KeyCode::Media(MediaKeyCode::Reverse),
  "mediastop" => KeyCode::Media(MediaKeyCode::Stop),
  "mediafastforward" => KeyCode::Media(MediaKeyCode::FastForward),
  "mediarewind" => KeyCode::Media(MediaKeyCode::Rewind),
  "medianext" => KeyCode::Media(MediaKeyCode::Next),
  "mediaprev" => KeyCode::Media(MediaKeyCode::Prev),
  "mediarecord" => KeyCode::Media(MediaKeyCode::Record),
  "volumedown" => KeyCode::Media(MediaKeyCode::VolumeDown),
  "volumeup" => KeyCode::Media(MediaKeyCode::VolumeUp),
  "volumemute" => KeyCode::Media(MediaKeyCode::VolumeMute),

  "leftshift" => KeyCode::Modifier(ModKeyCode::LeftShift),
  "leftcontrol" => KeyCode::Modifier(ModKeyCode::LeftControl),
  "leftalt" => KeyCode::Modifier(ModKeyCode::LeftAlt),
  "leftsuper" => KeyCode::Modifier(ModKeyCode::LeftSuper),
  "lefthyper" => KeyCode::Modifier(ModKeyCode::LeftHyper),
  "leftmeta" => KeyCode::Modifier(ModKeyCode::LeftMeta),
  "rightshift" => KeyCode::Modifier(ModKeyCode::RightShift),
  "rightcontrol" => KeyCode::Modifier(ModKeyCode::RightControl),
  "rightalt" => KeyCode::Modifier(ModKeyCode::RightAlt),
  "rightsuper" => KeyCode::Modifier(ModKeyCode::RightSuper),
  "righthyper" => KeyCode::Modifier(ModKeyCode::RightHyper),
  "rightmeta" => KeyCode::Modifier(ModKeyCode::RightMeta),
  // "isolevel3shift" => KeyCode::Modifier(ModKeyCode::IsoLevel3Shift),
  // "isolevel5shift" => KeyCode::Modifier(ModKeyCode::IsoLevel5Shift),

  "space" => KeyCode::Char(' '),

  "lt" => KeyCode::Char('<'),
  "gt" => KeyCode::Char('>'),
  "minus" => KeyCode::Char('-'),
};

static SPECIAL_CHARS: phf::Map<char, &str> = phf::phf_map! {
  ' ' => "Space",

  '<' => "LT",
  '>' => "GT",
  '-' => "Minus",
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
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
    KeySpec::parse(text).map(Into::into)
  }

  pub fn spec(self) -> KeySpec {
    KeySpec(self)
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KeySpec(pub Key);

impl KeySpec {
  pub fn parse(text: &str) -> anyhow::Result<Self> {
    KeyParser::parse(text).map(Self)
  }

  pub fn key(self) -> Key {
    self.0
  }
}

impl From<Key> for KeySpec {
  fn from(key: Key) -> Self {
    Self(key)
  }
}

impl From<KeySpec> for Key {
  fn from(spec: KeySpec) -> Self {
    spec.0
  }
}

impl fmt::Display for KeySpec {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut buf = String::new();
    let key = self.0;

    buf.push('<');

    let mods = key.mods;
    if mods.intersects(KeyMods::CONTROL) {
      buf.push_str("C-");
    }
    if mods.intersects(KeyMods::SHIFT) {
      buf.push_str("S-");
    }
    if mods.intersects(KeyMods::ALT) {
      buf.push_str("M-");
    }

    match key.code {
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
      KeyCode::Modifier(code) => {
        let s = match code {
          ModKeyCode::LeftShift => "LeftShift",
          ModKeyCode::LeftControl => "LeftControl",
          ModKeyCode::LeftAlt => "LeftAlt",
          ModKeyCode::LeftSuper => "LeftSuper",
          ModKeyCode::LeftHyper => "LeftHyper",
          ModKeyCode::LeftMeta => "LeftMeta",
          ModKeyCode::RightShift => "RightShift",
          ModKeyCode::RightControl => "RightControl",
          ModKeyCode::RightAlt => "RightAlt",
          ModKeyCode::RightSuper => "RightSuper",
          ModKeyCode::RightHyper => "RightHyper",
          ModKeyCode::RightMeta => "RightMeta",
          // ModKeyCode::IsoLevel3Shift => "IsoLevel3Shift",
          // ModKeyCode::IsoLevel5Shift => "IsoLevel5Shift",
        };
        buf.push_str(s);
      }
    }

    buf.push('>');

    f.write_str(&buf)
  }
}

impl Serialize for KeySpec {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    serializer.serialize_str(self.to_string().as_str())
  }
}

impl<'de> Deserialize<'de> for KeySpec {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let text = String::deserialize(deserializer)?;
    KeySpec::parse(text.as_str())
      .map_err(|err| serde::de::Error::custom(err.to_string()))
  }
}

pub mod key_spec {
  use super::{Key, KeySpec};
  use serde::{Deserialize, Serialize};

  pub fn serialize<S: serde::Serializer>(
    key: &Key,
    serializer: S,
  ) -> Result<S::Ok, S::Error> {
    KeySpec::from(*key).serialize(serializer)
  }

  pub fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
  ) -> Result<Key, D::Error> {
    KeySpec::deserialize(deserializer).map(Into::into)
  }
}

#[derive(
  Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Serialize,
)]
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

#[derive(
  Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Serialize,
)]
pub enum KeyEventKind {
  Press,
  Repeat,
  Release,
}

bitflags! {
  #[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Serialize,
  )]
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

#[derive(
  Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Serialize,
)]
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
#[derive(
  Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Serialize,
)]
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
  // IsoLevel3Shift,
  // IsoLevel5Shift,
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
      } else if let Some(n) = code
        .strip_prefix('f')
        .or_else(|| code.strip_prefix('F'))
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|n| *n > 0)
      {
        KeyCode::F(n)
      } else {
        let mut chars = code.chars();
        match (chars.next(), chars.next()) {
          (Some(ch), None) => KeyCode::Char(ch),
          _ => bail!("Wrong key code: \"{}\"", code),
        }
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
    let bytes = self.text.as_bytes();
    let mut pos = self.pos;
    while pos + 1 < bytes.len() && bytes[pos + 1] == b'-' {
      match bytes[pos] {
        b'c' | b'C' => mods = mods.union(KeyMods::CONTROL),
        b's' | b'S' => mods = mods.union(KeyMods::SHIFT),
        b'm' | b'M' => mods = mods.union(KeyMods::ALT),
        b => bail!("Wrong key modifier: \"{}\"", b as char),
      }
      pos += 2;
    }
    self.pos = pos;
    Ok(mods)
  }
}

#[cfg(test)]
mod tests {
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
    assert_eq!(
      Key::parse("<F13>").unwrap(),
      Key::new(KeyCode::F(13), KeyMods::NONE)
    );

    assert_eq!(
      Key::parse("<CapsLock>").unwrap(),
      Key::new(KeyCode::CapsLock, KeyMods::NONE)
    );
    assert_eq!(
      Key::parse("<MediaPlayPause>").unwrap(),
      Key::new(KeyCode::Media(MediaKeyCode::PlayPause), KeyMods::NONE)
    );
    assert_eq!(
      Key::parse("<LeftSuper>").unwrap(),
      Key::new(KeyCode::Modifier(ModKeyCode::LeftSuper), KeyMods::NONE)
    );

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
      assert_eq!(Key::parse(key).unwrap().spec().to_string(), key);
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

    in_out("<F13>");
    in_out("<CapsLock>");
    in_out("<PrintScreen>");
    in_out("<KeypadBegin>");
    in_out("<MediaPlayPause>");
    in_out("<VolumeMute>");
    in_out("<LeftSuper>");
  }

  #[test]
  fn wire_keys_round_trip_losslessly() {
    let keys = [
      Key::new(KeyCode::CapsLock, KeyMods::NONE),
      Key::new(KeyCode::NumLock, KeyMods::NONE),
      Key::new(KeyCode::Media(MediaKeyCode::PlayPause), KeyMods::NONE),
      Key::new_with_kind(
        KeyCode::Modifier(ModKeyCode::LeftSuper),
        KeyMods::SUPER,
        KeyEventKind::Press,
      ),
      Key::new(KeyCode::F(13), KeyMods::CONTROL),
      Key::new(KeyCode::Char('我'), KeyMods::NONE),
      Key {
        code: KeyCode::Char('a'),
        mods: KeyMods::SHIFT,
        kind: KeyEventKind::Repeat,
        state: KeyEventState::CAPS_LOCK | KeyEventState::NUM_LOCK,
      },
    ];
    for key in keys {
      let bytes = bincode::serialize(&key).unwrap();
      let decoded: Key = bincode::deserialize(&bytes)
        .unwrap_or_else(|e| panic!("decode of {key:?} failed: {e}"));
      assert_eq!(decoded, key);
    }
  }

  #[test]
  fn key_serialization_is_structural() {
    let key = Key::parse("<C-a>").unwrap();
    let yaml = serde_yaml::to_string(&key).unwrap();
    assert!(yaml.contains("code:"));
    assert!(yaml.contains("mods:"));
    assert_eq!(serde_yaml::from_str::<Key>(&yaml).unwrap(), key);
  }

  #[test]
  fn key_spec_serialization_round_trips_and_is_strict() {
    let key = Key::parse("<C-a>").unwrap();
    let yaml = serde_yaml::to_string(&key.spec()).unwrap();
    assert_eq!(yaml, "<C-a>\n");
    assert_eq!(serde_yaml::from_str::<KeySpec>(&yaml).unwrap().key(), key);
    assert!(serde_yaml::from_str::<Key>("<Bogus>").is_err());
    assert!(serde_yaml::from_str::<KeySpec>("<Bogus>").is_err());
  }
}
