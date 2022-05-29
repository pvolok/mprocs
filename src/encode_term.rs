use std::fmt::Write;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::key::Key;

pub const CSI: &str = "\x1b[";
pub const SS3: &str = "\x1bO";

/// Specifies terminal modes/configuration that can influence how a KeyCode
/// is encoded when being sent to and application via the pty.
#[derive(Debug, Clone, Copy)]
pub struct KeyCodeEncodeModes {
  pub enable_csi_u_key_encoding: bool,
  pub application_cursor_keys: bool,
  pub newline_mode: bool,
}

impl Default for KeyCodeEncodeModes {
  fn default() -> Self {
    KeyCodeEncodeModes {
      enable_csi_u_key_encoding: false,
      application_cursor_keys: false,
      newline_mode: false,
    }
  }
}

/// Returns the xterm compatible byte sequence that represents this KeyCode
/// and Modifier combination.
pub fn encode_key(key: &Key, modes: KeyCodeEncodeModes) -> Result<String> {
  use KeyCode::*;

  let code = key.code().clone();
  let mods = key.mods().clone();

  let mut buf = String::new();

  let code = normalize_shift_to_upper_case(code, &mods);
  // Normalize the modifier state for Char's that are uppercase; remove
  // the SHIFT modifier so that reduce ambiguity below
  let mods = match code {
    Char(c)
      if (c.is_ascii_punctuation() || c.is_ascii_uppercase())
        && mods.contains(KeyModifiers::SHIFT) =>
    {
      mods.clone().difference(KeyModifiers::SHIFT)
    }
    _ => mods,
  };

  // Normalize Backspace and Delete
  let code = match code {
    Char('\x7f') => KeyCode::Backspace,
    Char('\x08') => KeyCode::Delete,
    c => c,
  };

  // TODO: also respect self.application_keypad

  match code {
    Char(c)
      if is_ambiguous_ascii_ctrl(c)
        && mods.contains(KeyModifiers::CONTROL)
        && modes.enable_csi_u_key_encoding =>
    {
      csi_u_encode(&mut buf, c, mods, modes.enable_csi_u_key_encoding)?;
    }
    Char(c)
      if c.is_ascii_uppercase() && mods.contains(KeyModifiers::CONTROL) =>
    {
      csi_u_encode(&mut buf, c, mods, modes.enable_csi_u_key_encoding)?;
    }

    Char(c)
      if mods.contains(KeyModifiers::CONTROL) && ctrl_mapping(c).is_some() =>
    {
      let c = ctrl_mapping(c).unwrap();
      if mods.contains(KeyModifiers::ALT) {
        buf.push(0x1b as char);
      }
      buf.push(c);
    }

    // When alt is pressed, send escape first to indicate to the peer that
    // ALT is pressed.  We do this only for ascii alnum characters because
    // eg: on macOS generates altgr style glyphs and keeps the ALT key
    // in the modifier set.  This confuses eg: zsh which then just displays
    // <fffffffff> as the input, so we want to avoid that.
    Char(c)
      if (c.is_ascii_alphanumeric() || c.is_ascii_punctuation())
        && mods.contains(KeyModifiers::ALT) =>
    {
      buf.push(0x1b as char);
      buf.push(c);
    }

    Enter | Esc | Backspace => {
      let c = match code {
        Enter => '\r',
        Esc => '\x1b',
        // Backspace sends the default VERASE which is confusingly
        // the DEL ascii codepoint
        Backspace => '\x7f',
        _ => unreachable!(),
      };
      if mods.contains(KeyModifiers::SHIFT)
        || mods.contains(KeyModifiers::CONTROL)
      {
        csi_u_encode(&mut buf, c, mods, modes.enable_csi_u_key_encoding)?;
      } else {
        if mods.contains(KeyModifiers::ALT) {
          buf.push(0x1b as char);
        }
        buf.push(c);
        if modes.newline_mode && code == Enter {
          buf.push(0x0a as char);
        }
      }
    }

    Tab => {
      if mods.contains(KeyModifiers::ALT) {
        buf.push(0x1b as char);
      }
      let mods = mods & !KeyModifiers::ALT;
      if mods == KeyModifiers::CONTROL {
        buf.push_str("\x1b[9;5u");
      } else if mods == KeyModifiers::CONTROL | KeyModifiers::SHIFT {
        buf.push_str("\x1b[1;5Z");
      } else if mods == KeyModifiers::SHIFT {
        buf.push_str("\x1b[Z");
      } else {
        buf.push('\t');
      }
    }

    Char(c) => {
      if mods.is_empty() {
        buf.push(c);
      } else {
        csi_u_encode(&mut buf, c, mods, modes.enable_csi_u_key_encoding)?;
      }
    }

    Home | End | Up | Down | Right | Left => {
      let (force_app, c) = match code {
        Up => (false, 'A'),
        Down => (false, 'B'),
        Right => (false, 'C'),
        Left => (false, 'D'),
        Home => (false, 'H'),
        End => (false, 'F'),
        _ => unreachable!(),
      };

      let csi_or_ss3 = if force_app
        || (
          modes.application_cursor_keys
          // Strict reading of DECCKM suggests that application_cursor_keys
          // only applies when DECANM and DECKPAM are active, but that seems
          // to break unmodified cursor keys in vim
          /* && self.dec_ansi_mode && self.application_keypad */
        ) {
        // Use SS3 in application mode
        SS3
      } else {
        // otherwise use regular CSI
        CSI
      };

      if mods.contains(KeyModifiers::ALT)
        || mods.contains(KeyModifiers::SHIFT)
        || mods.contains(KeyModifiers::CONTROL)
      {
        write!(buf, "{}1;{}{}", CSI, 1 + encode_modifiers(mods), c)?;
      } else {
        write!(buf, "{}{}", csi_or_ss3, c)?;
      }
    }

    PageUp | PageDown | Insert | Delete => {
      let c = match code {
        Insert => 2,
        Delete => 3,
        PageUp => 5,
        PageDown => 6,
        _ => unreachable!(),
      };

      if mods.contains(KeyModifiers::ALT)
        || mods.contains(KeyModifiers::SHIFT)
        || mods.contains(KeyModifiers::CONTROL)
      {
        write!(buf, "\x1b[{};{}~", c, 1 + encode_modifiers(mods))?;
      } else {
        write!(buf, "\x1b[{}~", c)?;
      }
    }

    F(n) => {
      if mods.is_empty() && n < 5 {
        // F1-F4 are encoded using SS3 if there are no modifiers
        write!(
          buf,
          "{}",
          match n {
            1 => "\x1bOP",
            2 => "\x1bOQ",
            3 => "\x1bOR",
            4 => "\x1bOS",
            _ => unreachable!("wat?"),
          }
        )?;
      } else {
        // Higher numbered F-keys plus modified F-keys are encoded
        // using CSI instead of SS3.
        let intro = match n {
          1 => "\x1b[11",
          2 => "\x1b[12",
          3 => "\x1b[13",
          4 => "\x1b[14",
          5 => "\x1b[15",
          6 => "\x1b[17",
          7 => "\x1b[18",
          8 => "\x1b[19",
          9 => "\x1b[20",
          10 => "\x1b[21",
          11 => "\x1b[23",
          12 => "\x1b[24",
          _ => panic!("unhandled fkey number {}", n),
        };
        let encoded_mods = encode_modifiers(mods);
        if encoded_mods == 0 {
          // If no modifiers are held, don't send the modifier
          // sequence, as the modifier encoding is a CSI-u extension.
          write!(buf, "{}~", intro)?;
        } else {
          write!(buf, "{};{}~", intro, 1 + encoded_mods)?;
        }
      }
    }

    BackTab | Null => todo!(),
  };

  Ok(buf)
}

fn encode_modifiers(mods: KeyModifiers) -> u8 {
  let mut number = 0;
  if mods.contains(KeyModifiers::SHIFT) {
    number |= 1;
  }
  if mods.contains(KeyModifiers::ALT) {
    number |= 2;
  }
  if mods.contains(KeyModifiers::CONTROL) {
    number |= 4;
  }
  number
}

/// characters that when masked for CTRL could be an ascii control character
/// or could be a key that a user legitimately wants to process in their
/// terminal application
fn is_ambiguous_ascii_ctrl(c: char) -> bool {
  match c {
    'i' | 'I' | 'm' | 'M' | '[' | '{' | '@' => true,
    _ => false,
  }
}

/// Map c to its Ctrl equivalent.
/// In theory, this mapping is simply translating alpha characters
/// to upper case and then masking them by 0x1f, but xterm inherits
/// some built-in translation from legacy X11 so that are some
/// aliased mappings and a couple that might be technically tied
/// to US keyboard layout (particularly the punctuation characters
/// produced in combination with SHIFT) that may not be 100%
/// the right thing to do here for users with non-US layouts.
fn ctrl_mapping(c: char) -> Option<char> {
  Some(match c {
    '@' | '`' | ' ' | '2' => '\x00',
    'A' | 'a' => '\x01',
    'B' | 'b' => '\x02',
    'C' | 'c' => '\x03',
    'D' | 'd' => '\x04',
    'E' | 'e' => '\x05',
    'F' | 'f' => '\x06',
    'G' | 'g' => '\x07',
    'H' | 'h' => '\x08',
    'I' | 'i' => '\x09',
    'J' | 'j' => '\x0a',
    'K' | 'k' => '\x0b',
    'L' | 'l' => '\x0c',
    'M' | 'm' => '\x0d',
    'N' | 'n' => '\x0e',
    'O' | 'o' => '\x0f',
    'P' | 'p' => '\x10',
    'Q' | 'q' => '\x11',
    'R' | 'r' => '\x12',
    'S' | 's' => '\x13',
    'T' | 't' => '\x14',
    'U' | 'u' => '\x15',
    'V' | 'v' => '\x16',
    'W' | 'w' => '\x17',
    'X' | 'x' => '\x18',
    'Y' | 'y' => '\x19',
    'Z' | 'z' => '\x1a',
    '[' | '3' | '{' => '\x1b',
    '\\' | '4' | '|' => '\x1c',
    ']' | '5' | '}' => '\x1d',
    '^' | '6' | '~' => '\x1e',
    '_' | '7' | '/' => '\x1f',
    '8' | '?' => '\x7f', // `Delete`
    _ => return None,
  })
}

fn csi_u_encode(
  buf: &mut String,
  c: char,
  mods: KeyModifiers,
  enable_csi_u_key_encoding: bool,
) -> Result<()> {
  if enable_csi_u_key_encoding {
    write!(buf, "\x1b[{};{}u", c as u32, 1 + encode_modifiers(mods))?;
  } else {
    let c = if mods.contains(KeyModifiers::CONTROL) && ctrl_mapping(c).is_some()
    {
      ctrl_mapping(c).unwrap()
    } else {
      c
    };
    if mods.contains(KeyModifiers::ALT) {
      buf.push(0x1b as char);
    }
    write!(buf, "{}", c)?;
  }
  Ok(())
}

/// if SHIFT is held and we have KeyCode::Char('c') we want to normalize
/// that keycode to KeyCode::Char('C'); that is what this function does.
/// In theory we should give the same treatment to keys like `[` -> `{`
/// but that assumes something about the keyboard layout and is probably
/// better done in the gui frontend rather than this layer.
/// In fact, this function might be better off if it lived elsewhere.
pub fn normalize_shift_to_upper_case(
  code: KeyCode,
  modifiers: &KeyModifiers,
) -> KeyCode {
  if modifiers.contains(KeyModifiers::SHIFT) {
    match code {
      KeyCode::Char(c) if c.is_ascii_lowercase() => KeyCode::Char(c),
      _ => code,
    }
  } else {
    code
  }
}

pub fn print_key(key: KeyEvent) -> String {
  let mut buf = String::new();

  if key.modifiers.contains(KeyModifiers::CONTROL) {
    buf.push_str("C-");
  }
  if key.modifiers.contains(KeyModifiers::SHIFT) {
    buf.push_str("S-");
  }
  if key.modifiers.contains(KeyModifiers::ALT) {
    buf.push_str("M-");
  }

  match key.code {
    KeyCode::Backspace => buf.push_str("Backspace"),
    KeyCode::Enter => buf.push_str("Enter"),
    KeyCode::Left => buf.push_str("Left"),
    KeyCode::Right => buf.push_str("Right"),
    KeyCode::Up => buf.push_str("Up"),
    KeyCode::Down => buf.push_str("Down"),
    KeyCode::Home => buf.push_str("Home"),
    KeyCode::End => buf.push_str("End"),
    KeyCode::PageUp => buf.push_str("PgUp"),
    KeyCode::PageDown => buf.push_str("PgDn"),
    KeyCode::Tab => buf.push_str("Tab"),
    KeyCode::BackTab => buf.push_str("BackTab"),
    KeyCode::Delete => buf.push_str("Del"),
    KeyCode::Insert => buf.push_str("Ins"),
    KeyCode::F(n) => buf.push_str(&format!("F{}", n)),
    KeyCode::Char(ch) => buf.push(ch),
    KeyCode::Null => buf.push_str("Null"),
    KeyCode::Esc => buf.push_str("Esc"),
  }

  return buf;
}
