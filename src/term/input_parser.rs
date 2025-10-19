use crossterm::event::{KeyCode, KeyEvent};

use crate::term::internal::InternalTermEvent as E;

pub struct InputParser {
  buf: Vec<u8>,
}

impl InputParser {
  pub fn new() -> Self {
    Self { buf: Vec::new() }
  }

  pub fn parse_input<F>(&mut self, input: &[u8], mut f: F)
  where
    F: FnMut(E),
  {
    let more = false;
    let mut i = 0;
    let mut consumed = 0;

    use crossterm::event::KeyModifiers as Mods;

    self.buf.extend_from_slice(input);
    let buf = &self.buf;

    while i < buf.len() {
      let next_char = buf[i];
      i += 1;
      match next_char {
        b'\x1B' => {
          if i >= buf.len() {
            if more {
              break;
            } else {
              f(E::Key(KeyEvent::new(KeyCode::Esc, Mods::NONE)));
              consumed = i;
              continue;
            }
          }
          let next_char = buf[i];
          i += 1;
          match next_char {
            b'O' => {
              if i >= buf.len() {
                break;
              }
              let next_char = buf[i];
              i += 1;
              match next_char {
                b'A' => {
                  consumed = i;
                  f(E::Key(KeyEvent::new(KeyCode::Up, Mods::NONE)));
                }
                b'B' => {
                  consumed = i;
                  f(E::Key(KeyEvent::new(KeyCode::Down, Mods::NONE)));
                }
                b'C' => {
                  consumed = i;
                  f(E::Key(KeyEvent::new(KeyCode::Right, Mods::NONE)));
                }
                b'D' => {
                  consumed = i;
                  f(E::Key(KeyEvent::new(KeyCode::Left, Mods::NONE)));
                }
                b'F' => {
                  consumed = i;
                  f(E::Key(KeyEvent::new(KeyCode::End, Mods::NONE)));
                }
                b'H' => {
                  consumed = i;
                  f(E::Key(KeyEvent::new(KeyCode::Home, Mods::NONE)));
                }
                val @ b'P'..=b'S' => {
                  consumed = i;
                  f(E::Key(KeyEvent::new(
                    KeyCode::F(1 + val - b'P'),
                    Mods::NONE,
                  )));
                }
                _ => {
                  consumed = i;
                  log::error!("Input parsing error.");
                }
              }
            }
            b'[' => {
              let len = parse_csi(&buf[i..], &mut f);
              i += len;
              consumed = i;
            }
            b'\x1B' => {
              f(E::Key(KeyEvent::new(KeyCode::Esc, Mods::NONE)));
              consumed = i;
            }
            _ => {
              log::error!("TODO: Handle ESC[..");
              consumed = i;
            }
          }
        }
        b'\r' => {
          f(E::Key(KeyEvent::new(KeyCode::Enter, Mods::NONE)));
          consumed = i;
        }
        b'\n' => {
          // TODO: Check for Ctrl-J.
          f(E::Key(KeyEvent::new(KeyCode::Enter, Mods::NONE)));
          consumed = i;
        }
        b'\t' => {
          f(E::Key(KeyEvent::new(KeyCode::Tab, Mods::NONE)));
          consumed = i;
        }
        b'\x7F' => {
          f(E::Key(KeyEvent::new(KeyCode::Backspace, Mods::NONE)));
          consumed = i;
        }
        c @ b'\x01'..=b'\x1A' => {
          f(E::Key(KeyEvent::new(
            KeyCode::Char((c - 0x1 + b'a') as char),
            Mods::CONTROL,
          )));
          consumed = i;
        }
        c @ b'\x1C'..=b'\x1F' => {
          f(E::Key(KeyEvent::new(
            KeyCode::Char((c - 0x1C + b'4') as char),
            Mods::CONTROL,
          )));
          consumed = i;
        }
        b'\0' => {
          f(E::Key(KeyEvent::new(KeyCode::Char(' '), Mods::CONTROL)));
          consumed = i;
        }
        first_byte => {
          let char_len = utf8_char_len(first_byte);
          if char_len == 0 {
            // Ignore invalid byte.
            consumed = i;
          } else if i - 1 + char_len <= buf.len() {
            let char = str::from_utf8(&buf[i - 1..i - 1 + char_len]);
            match char {
              Ok(s) => {
                let char = s.chars().next().unwrap();
                f(E::Key(KeyEvent::new(KeyCode::Char(char), Mods::NONE)));
              }
              Err(_) => {
                // Invalid utf-8 char.
              }
            }
            consumed = i - 1 + char_len;
          } else {
            // Not enough bytes.
          }
        }
      }
    }

    let buf_len = self.buf.len();
    self.buf.copy_within(consumed..buf_len, 0);
    self.buf.truncate(buf_len - consumed);
  }
}

fn parse_csi<F>(buf: &[u8], mut f: F) -> usize
where
  F: FnMut(E),
{
  //
  // Parse
  //
  let mut i = 0;
  while i < buf.len() && (0x30..=0x3f).contains(&buf[i]) {
    i += 1;
  }
  let params = &buf[..i];
  while i < buf.len() && (0x20..=0x2f).contains(&buf[i]) {
    i += 1;
  }
  let _intermediates = &buf[params.len()..i];

  let final_ = if i < buf.len() {
    if (0x40..=0x7E).contains(&buf[i]) {
      i += 1;
      buf[i - 1]
    } else {
      log::error!("TODO: CSI is incomplete.");
      return i;
    }
  } else {
    log::error!("CSI sequence has wrong final.");
    return i;
  };

  //
  // Handle
  //

  match final_ {
    b'u' => {
      // Kitty keyboard protocol reply.
      if params.starts_with(b"?") {
        if let Some(flags_char) = params.get(1) {
          let flags = flags_char.wrapping_sub(b'0');
          f(E::ReplyKittyKeyboard(flags));
        }
      }
    }
    _ => (),
  }

  i
}

fn utf8_char_len(first_byte: u8) -> usize {
  match first_byte {
    // https://en.wikipedia.org/wiki/UTF-8#Description
    (0x00..=0x7F) => 1, // 0xxxxxxx
    (0xC0..=0xDF) => 2, // 110xxxxx 10xxxxxx
    (0xE0..=0xEF) => 3, // 1110xxxx 10xxxxxx 10xxxxxx
    (0xF0..=0xF7) => 4, // 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
    (0x80..=0xBF) | (0xF8..=0xFF) => 0,
  }
}
