use anyhow::bail;
use crossterm::event::{KeyCode, KeyModifiers};
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
  '<' => "LT",
  '>' => "GT",
  '-' => "Minus",
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Key {
  code: KeyCode,
  mods: KeyModifiers,
}

impl Key {
  pub fn new(code: KeyCode, mods: KeyModifiers) -> Key {
    Self { code, mods }
  }

  pub fn parse(text: &str) -> anyhow::Result<Key> {
    KeyParser::parse(text)
  }

  pub fn code(&self) -> KeyCode {
    self.code
  }

  pub fn mods(&self) -> KeyModifiers {
    self.mods
  }

  pub fn set_mods(mut self, mods: KeyModifiers) -> Self {
    self.mods = mods;
    self
  }
}

impl From<KeyCode> for Key {
  fn from(code: KeyCode) -> Self {
    Key::new(code, KeyModifiers::NONE)
  }
}

impl ToString for Key {
  fn to_string(&self) -> String {
    let mut buf = String::new();

    buf.push('<');

    let mods = self.mods;
    if mods.intersects(KeyModifiers::CONTROL) {
      buf.push_str("C-");
    }
    if mods.intersects(KeyModifiers::SHIFT) {
      buf.push_str("S-");
    }
    if mods.intersects(KeyModifiers::ALT) {
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
      KeyCode::BackTab => buf.push_str("S-Tab"),
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
      KeyCode::Media(_code) => {
        // TODO
        buf.push_str("Nul");
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

  fn take_mods(&mut self) -> anyhow::Result<KeyModifiers> {
    let mut mods = KeyModifiers::NONE;
    let mut pos = self.pos;
    while pos + 1 < self.text.len() && &self.text[pos + 1..pos + 2] == "-" {
      match &self.text[pos..pos + 1] {
        "c" | "C" => mods = mods.union(KeyModifiers::CONTROL),
        "s" | "S" => mods = mods.union(KeyModifiers::SHIFT),
        "m" | "M" => mods = mods.union(KeyModifiers::ALT),
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
      Key::new(KeyCode::Tab, KeyModifiers::NONE)
    );
    assert_eq!(
      Key::parse("<C-Enter>").unwrap(),
      Key::new(KeyCode::Enter, KeyModifiers::CONTROL)
    );
    assert_eq!(
      Key::parse("<C-Esc>").unwrap(),
      Key::new(KeyCode::Esc, KeyModifiers::CONTROL)
    );

    assert_eq!(
      Key::parse("<F1>").unwrap(),
      Key::new(KeyCode::F(1), KeyModifiers::NONE)
    );
    assert_eq!(
      Key::parse("<f12>").unwrap(),
      Key::new(KeyCode::F(12), KeyModifiers::NONE)
    );
    assert_matches!(Key::parse("<F13>"), Err(_));

    assert_eq!(
      Key::parse("<a>").unwrap(),
      Key::new(KeyCode::Char('a'), KeyModifiers::NONE)
    );
    assert_eq!(
      Key::parse("<C-a>").unwrap(),
      Key::new(KeyCode::Char('a'), KeyModifiers::CONTROL)
    );
    assert_eq!(
      Key::parse("<C-M-a>").unwrap(),
      Key::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL | KeyModifiers::ALT
      )
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
