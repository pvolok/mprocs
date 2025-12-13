use anyhow::{anyhow, bail};
use crossterm::event::{
  KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MediaKeyCode,
  ModifierKeyCode, MouseButton, MouseEvent, MouseEventKind,
};

use crate::term::internal::InternalTermEvent as E;

pub struct InputParser {
  buf: Vec<u8>,
}

impl InputParser {
  pub fn new() -> Self {
    Self { buf: Vec::new() }
  }

  pub fn parse_input<F>(&mut self, input: &[u8], is_raw_mode: bool, mut f: F)
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
              if let Some(b'M') = buf.get(i) {
                // ESC [ M CB Cx Cy
                // http://www.xfree86.org/current/ctlseqs.html#Mouse%20Tracking
                i += 1; // M
                if i + 3 >= buf.len() {
                  break;
                }
                let b = buf[i].saturating_sub(32);
                let x = u16::from(buf[i + 1].saturating_sub(32)) - 1;
                let y = u16::from(buf[i + 2].saturating_sub(32)) - 1;
                i += 3;
                consumed = i;
                match parse_cb(b) {
                  Ok((kind, modifiers)) => {
                    f(E::Mouse(MouseEvent {
                      kind,
                      modifiers,
                      column: x,
                      row: y,
                    }));
                  }
                  Err(err) => {
                    log::error!("'CSI M bxy' error: {}", err);
                  }
                }
              } else {
                // Most of CSI
                let len = parse_csi(&buf[i..], is_raw_mode, &mut f);
                i += len;
                consumed = i;
              }
            }
            b'\x1B' => {
              f(E::Key(KeyEvent::new(KeyCode::Esc, Mods::NONE)));
              consumed = i;
            }
            c => {
              match parse_char_key(c, &buf[i - 1..]) {
                Ok((used, key)) => {
                  i += used;
                  if let Some(mut key) = key {
                    key.modifiers |= Mods::ALT;
                    f(E::Key(key));
                  }
                }
                Err(err) => {
                  log::error!("Parse esc-char key error: {}", err);
                }
              }
              consumed = i;
            }
          }
        }
        c => {
          match parse_char_key(c, &buf[i - 1..]) {
            Ok((used, key)) => {
              i += used;
              if let Some(key) = key {
                f(E::Key(key));
              }
            }
            Err(err) => {
              log::error!("Parse char key error: {}", err);
            }
          }
          consumed = i;
        }
      }
    }

    let buf_len = self.buf.len();
    self.buf.copy_within(consumed..buf_len, 0);
    self.buf.truncate(buf_len - consumed);
  }
}

fn parse_char_key(
  c: u8,
  buf: &[u8],
) -> anyhow::Result<(usize, Option<KeyEvent>)> {
  use crossterm::event::KeyModifiers as Mods;

  let mut i = 0;
  let key = match c {
    b'\r' => KeyEvent::new(KeyCode::Enter, Mods::NONE),
    b'\n' => KeyEvent::new(KeyCode::Enter, Mods::NONE),
    b'\t' => KeyEvent::new(KeyCode::Tab, Mods::NONE),
    b'\x7F' => KeyEvent::new(KeyCode::Backspace, Mods::NONE),
    c @ b'\x01'..=b'\x1A' => {
      KeyEvent::new(KeyCode::Char((c - 0x1 + b'a') as char), Mods::CONTROL)
    }
    c @ b'\x1C'..=b'\x1F' => {
      KeyEvent::new(KeyCode::Char((c - 0x1C + b'4') as char), Mods::CONTROL)
    }
    b'\0' => KeyEvent::new(KeyCode::Char(' '), Mods::CONTROL),
    first_byte => {
      let char_len = utf8_char_len(first_byte);
      if char_len == 0 {
        // Ignore invalid byte.
        return Ok((0, None));
      } else if char_len - 1 <= buf.len() {
        let char = str::from_utf8(&buf[..char_len]);
        i = char_len - 1;
        match char {
          Ok(s) => {
            let char = s.chars().next().unwrap();
            KeyEvent::new(KeyCode::Char(char), Mods::NONE)
          }
          Err(_) => {
            bail!("Invalid utf-8 char");
          }
        }
      } else {
        bail!("Not enough bytes");
      }
    }
  };
  Ok((i, Some(key)))
}

fn parse_csi<F>(buf: &[u8], is_raw_mode: bool, f: F) -> usize
where
  F: FnMut(E),
{
  let mut i = 0;
  while i < buf.len() && (0x30..=0x3f).contains(&buf[i]) {
    i += 1;
  }
  let params = &buf[..i];
  while i < buf.len() && (0x20..=0x2f).contains(&buf[i]) {
    i += 1;
  }
  let intermediates = &buf[params.len()..i];

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

  match parse_csi_impl(buf, params, intermediates, final_, is_raw_mode, f) {
    Ok(()) => (),
    Err(err) => log::error!("CSI error: {}", err),
  }

  i
}

fn parse_csi_impl<F>(
  buf: &[u8],
  params: &[u8],
  intermediates: &[u8],
  final_: u8,
  is_raw_mode: bool,
  mut f: F,
) -> anyhow::Result<()>
where
  F: FnMut(E),
{
  match final_ {
    b'A' | b'B' | b'C' | b'D' | b'F' | b'H' | b'P' | b'Q' | b'S' => {
      let code = match final_ {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'F' => KeyCode::End,
        b'H' => KeyCode::Home,
        b'P' => KeyCode::F(1),
        b'Q' => KeyCode::F(2),
        b'S' => KeyCode::F(4),
        _ => unreachable!("Unhanled CSI [A|B|C|...] key"),
      };

      let params = str::from_utf8(params)?;
      let mut params = params.split(';');

      params.next();

      let mods = if let Some(mods) = params.next() {
        parse_modifiers(mods.parse::<u8>()?)
      } else {
        KeyModifiers::NONE
      };

      f(E::Key(KeyEvent::new(code, mods)));
    }
    b'c' if params.starts_with(b"?") => {
      f(E::PrimaryDeviceAttributes);
    }
    b'Z' => {
      f(E::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)));
    }
    b'I' => {
      f(E::FocusGained);
    }
    b'O' => {
      f(E::FocusLost);
    }
    b'u' => {
      // Kitty keyboard protocol reply.
      if params.starts_with(b"?") {
        if let Some(flags_char) = params.get(1) {
          let flags = flags_char.wrapping_sub(b'0');
          f(E::ReplyKittyKeyboard(flags));
        }
      } else {
        // CSI unicode-key-code:alternate-key-codes ; modifiers:event-type ; text-as-codepoints u

        let (code, alt_code, mods_param, kind) = parse_csi_u(params)?;

        let mut mods = parse_modifiers(mods_param);
        let kind = parse_key_event_kind(kind);
        let state_from_mods = parse_modifiers_to_state(mods_param);

        let (mut keycode, state_from_keycode) = {
          if let Some((special_key_code, state)) =
            translate_functional_key_code(code)
          {
            (special_key_code, state)
          } else if let Some(c) = char::from_u32(code) {
            (
              match c {
                '\x1B' => KeyCode::Esc,
                '\r' => KeyCode::Enter,
                // Issue #371: \n = 0xA, which is also the keycode for Ctrl+J. The only reason we get
                // newlines as input is because the terminal converts \r into \n for us. When we
                // enter raw mode, we disable that, so \n no longer has any meaning - it's better to
                // use Ctrl+J. Waiting to handle it here means it gets picked up later
                '\n' if !is_raw_mode => KeyCode::Enter,
                '\t' => {
                  if mods.contains(KeyModifiers::SHIFT) {
                    KeyCode::BackTab
                  } else {
                    KeyCode::Tab
                  }
                }
                '\x7F' => KeyCode::Backspace,
                _ => KeyCode::Char(c),
              },
              KeyEventState::empty(),
            )
          } else {
            bail!(
              "Unhandled CSI-u {:?} {:?} {}",
              params,
              intermediates,
              final_
            );
          }
        };

        if let KeyCode::Modifier(modifier_keycode) = keycode {
          match modifier_keycode {
            ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt => {
              mods.set(KeyModifiers::ALT, true)
            }
            ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl => {
              mods.set(KeyModifiers::CONTROL, true)
            }
            ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift => {
              mods.set(KeyModifiers::SHIFT, true)
            }
            ModifierKeyCode::LeftSuper | ModifierKeyCode::RightSuper => {
              mods.set(KeyModifiers::SUPER, true)
            }
            ModifierKeyCode::LeftHyper | ModifierKeyCode::RightHyper => {
              mods.set(KeyModifiers::HYPER, true)
            }
            ModifierKeyCode::LeftMeta | ModifierKeyCode::RightMeta => {
              mods.set(KeyModifiers::META, true)
            }
            _ => {}
          }
        }

        // When the "report alternate keys" flag is enabled in the Kitty Keyboard Protocol
        // and the terminal sends a keyboard event containing shift, the sequence will
        // contain an additional codepoint separated by a ':' character which contains
        // the shifted character according to the keyboard layout.
        if mods.contains(KeyModifiers::SHIFT) {
          if let Some(shifted_c) = alt_code.and_then(char::from_u32) {
            keycode = KeyCode::Char(shifted_c);
            mods.set(KeyModifiers::SHIFT, false);
          }
        }

        let key_event = KeyEvent::new_with_kind_and_state(
          keycode,
          mods,
          kind,
          state_from_keycode | state_from_mods,
        );

        f(E::Key(key_event));
      }
    }
    b'm' | b'M' if params.starts_with(b"<") => {
      // SGR mouse
      // CSI < Cb ; Cx ; Cy (;) (M or m)
      let params_str = str::from_utf8(params)?;
      let mut params = params_str[1..].split(';');

      let b = params.next().ok_or_else(|| anyhow!("No Cb param"))?;
      let b = b.parse::<u8>()?;
      let (kind, modifiers) = parse_cb(b)?;
      let kind = if final_ == b'm' {
        match kind {
          MouseEventKind::Down(button) => MouseEventKind::Up(button),
          other => other,
        }
      } else {
        kind
      };

      let x = params.next().ok_or_else(|| anyhow!("No Cx param"))?;
      let x = x.parse::<u16>()? - 1;
      let y = params.next().ok_or_else(|| anyhow!("No Cy param"))?;
      let y = y.parse::<u16>()? - 1;

      f(E::Mouse(MouseEvent {
        kind,
        modifiers,
        column: x,
        row: y,
      }));
    }
    b'M' => {
      // rxvt mouse encoding:
      // CSI Cb ; Cx ; Cy ; M
      let params_str = str::from_utf8(params)?;
      let mut params = params_str.split(';');

      let b = params.next().ok_or_else(|| anyhow!("No rxvt Cb param"))?;
      let b = b.parse::<u8>()?.saturating_sub(32);
      let (kind, modifiers) = parse_cb(b)?;

      let x = params.next().ok_or_else(|| anyhow!("No rxvt Cx param"))?;
      let x = x.parse::<u16>()? - 1;
      let y = params.next().ok_or_else(|| anyhow!("No rxvt Cy param"))?;
      let y = y.parse::<u16>()? - 1;

      f(E::Mouse(MouseEvent {
        kind,
        modifiers,
        column: x,
        row: y,
      }));
    }
    b'R' => {
      // CSI Cy ; Cx R
      let params_str = str::from_utf8(params)?;
      let mut params = params_str.split(';');

      let y = params.next().ok_or_else(|| anyhow!("No CSI-R Cy param"))?;
      let y = y.parse::<u16>()?.saturating_sub(1);
      let x = params.next().ok_or_else(|| anyhow!("No CSI-R Cx param"))?;
      let x = x.parse::<u16>()?.saturating_sub(1);

      f(E::CursorPos(x, y));
    }
    b'~' => {
      let params_str = str::from_utf8(params)?;
      let mut params = params_str.split(';');

      let first = params
        .next()
        .ok_or_else(|| anyhow!("No key param in CSI ~"))?
        .parse::<u8>()?;

      let mods_param = params.next();
      let (mods, state) = if let Some(mods_param) = mods_param {
        let mask = mods_param.parse::<u8>()?;
        (parse_modifiers(mask), parse_modifiers_to_state(mask))
      } else {
        (KeyModifiers::NONE, KeyEventState::NONE)
      };

      if first == 27 {
        // modifyOtherKeys

        let code_param = params.next();
        let code = if let Some(code_param) = code_param {
          let code = code_param.parse::<u8>()?;
          KeyCode::Char(code.into())
        } else {
          bail!("Empty code in modifyOtherKeys")
        };

        f(E::Key(KeyEvent::new_with_kind_and_state(
          code,
          mods,
          KeyEventKind::Press,
          state,
        )));
      } else {
        let code = first;
        let code = match code {
          1 | 7 => KeyCode::Home,
          2 => KeyCode::Insert,
          3 => KeyCode::Delete,
          4 | 8 => KeyCode::End,
          5 => KeyCode::PageUp,
          6 => KeyCode::PageDown,
          v @ 11..=15 => KeyCode::F(v - 10),
          v @ 17..=21 => KeyCode::F(v - 11),
          v @ 23..=26 => KeyCode::F(v - 12),
          v @ 28..=29 => KeyCode::F(v - 15),
          v @ 31..=34 => KeyCode::F(v - 17),
          v => bail!("Wrong key param ({}) in CSI ~", v),
        };

        f(E::Key(KeyEvent::new_with_kind_and_state(
          code,
          mods,
          KeyEventKind::Press,
          state,
        )));
      }
    }
    _ => {
      log::debug!(
        "Unknown CSI: {} ({:?}, {:?}, {})",
        str::from_utf8(buf).unwrap_or("???"),
        params,
        intermediates,
        final_
      );
    }
  }
  Ok(())
}

fn parse_csi_u(params: &[u8]) -> anyhow::Result<(u32, Option<u32>, u8, u8)> {
  let params = str::from_utf8(params)?;
  let mut params = params.split(';');

  let code_param = params.next().ok_or_else(|| anyhow!("No code param"))?;
  let mut code_param = code_param.split(':');
  let code = code_param.next().ok_or_else(|| anyhow!("No code param"))?;
  let code = code.parse::<u32>()?;

  let alt_code = code_param.next().map(|c| c.parse::<u32>()).transpose()?;

  let mods_param =
    params.next().ok_or_else(|| anyhow!("No modifiers param"))?;
  let mut mods_param = mods_param.split(':');
  let mods = mods_param
    .next()
    .ok_or_else(|| anyhow!("No modifiers param"))?;
  let mods = mods.parse::<u8>()?;
  let kind = mods_param.next().map_or(Ok(1), |n| n.parse::<u8>())?;

  Ok((code, alt_code, mods, kind))
}

fn parse_modifiers(mask: u8) -> KeyModifiers {
  let modifier_mask = mask.saturating_sub(1);
  let mut modifiers = KeyModifiers::empty();
  if modifier_mask & 1 != 0 {
    modifiers |= KeyModifiers::SHIFT;
  }
  if modifier_mask & 2 != 0 {
    modifiers |= KeyModifiers::ALT;
  }
  if modifier_mask & 4 != 0 {
    modifiers |= KeyModifiers::CONTROL;
  }
  if modifier_mask & 8 != 0 {
    modifiers |= KeyModifiers::SUPER;
  }
  if modifier_mask & 16 != 0 {
    modifiers |= KeyModifiers::HYPER;
  }
  if modifier_mask & 32 != 0 {
    modifiers |= KeyModifiers::META;
  }
  modifiers
}

fn parse_modifiers_to_state(mask: u8) -> KeyEventState {
  let modifier_mask = mask.saturating_sub(1);
  let mut state = KeyEventState::empty();
  if modifier_mask & 64 != 0 {
    state |= KeyEventState::CAPS_LOCK;
  }
  if modifier_mask & 128 != 0 {
    state |= KeyEventState::NUM_LOCK;
  }
  state
}

fn parse_key_event_kind(kind: u8) -> KeyEventKind {
  match kind {
    1 => KeyEventKind::Press,
    2 => KeyEventKind::Repeat,
    3 => KeyEventKind::Release,
    _ => KeyEventKind::Press,
  }
}

fn translate_functional_key_code(
  codepoint: u32,
) -> Option<(KeyCode, KeyEventState)> {
  if let Some(keycode) = match codepoint {
    57399 => Some(KeyCode::Char('0')),
    57400 => Some(KeyCode::Char('1')),
    57401 => Some(KeyCode::Char('2')),
    57402 => Some(KeyCode::Char('3')),
    57403 => Some(KeyCode::Char('4')),
    57404 => Some(KeyCode::Char('5')),
    57405 => Some(KeyCode::Char('6')),
    57406 => Some(KeyCode::Char('7')),
    57407 => Some(KeyCode::Char('8')),
    57408 => Some(KeyCode::Char('9')),
    57409 => Some(KeyCode::Char('.')),
    57410 => Some(KeyCode::Char('/')),
    57411 => Some(KeyCode::Char('*')),
    57412 => Some(KeyCode::Char('-')),
    57413 => Some(KeyCode::Char('+')),
    57414 => Some(KeyCode::Enter),
    57415 => Some(KeyCode::Char('=')),
    57416 => Some(KeyCode::Char(',')),
    57417 => Some(KeyCode::Left),
    57418 => Some(KeyCode::Right),
    57419 => Some(KeyCode::Up),
    57420 => Some(KeyCode::Down),
    57421 => Some(KeyCode::PageUp),
    57422 => Some(KeyCode::PageDown),
    57423 => Some(KeyCode::Home),
    57424 => Some(KeyCode::End),
    57425 => Some(KeyCode::Insert),
    57426 => Some(KeyCode::Delete),
    57427 => Some(KeyCode::KeypadBegin),
    _ => None,
  } {
    return Some((keycode, KeyEventState::KEYPAD));
  }

  if let Some(keycode) = match codepoint {
    57358 => Some(KeyCode::CapsLock),
    57359 => Some(KeyCode::ScrollLock),
    57360 => Some(KeyCode::NumLock),
    57361 => Some(KeyCode::PrintScreen),
    57362 => Some(KeyCode::Pause),
    57363 => Some(KeyCode::Menu),
    57376 => Some(KeyCode::F(13)),
    57377 => Some(KeyCode::F(14)),
    57378 => Some(KeyCode::F(15)),
    57379 => Some(KeyCode::F(16)),
    57380 => Some(KeyCode::F(17)),
    57381 => Some(KeyCode::F(18)),
    57382 => Some(KeyCode::F(19)),
    57383 => Some(KeyCode::F(20)),
    57384 => Some(KeyCode::F(21)),
    57385 => Some(KeyCode::F(22)),
    57386 => Some(KeyCode::F(23)),
    57387 => Some(KeyCode::F(24)),
    57388 => Some(KeyCode::F(25)),
    57389 => Some(KeyCode::F(26)),
    57390 => Some(KeyCode::F(27)),
    57391 => Some(KeyCode::F(28)),
    57392 => Some(KeyCode::F(29)),
    57393 => Some(KeyCode::F(30)),
    57394 => Some(KeyCode::F(31)),
    57395 => Some(KeyCode::F(32)),
    57396 => Some(KeyCode::F(33)),
    57397 => Some(KeyCode::F(34)),
    57398 => Some(KeyCode::F(35)),
    57428 => Some(KeyCode::Media(MediaKeyCode::Play)),
    57429 => Some(KeyCode::Media(MediaKeyCode::Pause)),
    57430 => Some(KeyCode::Media(MediaKeyCode::PlayPause)),
    57431 => Some(KeyCode::Media(MediaKeyCode::Reverse)),
    57432 => Some(KeyCode::Media(MediaKeyCode::Stop)),
    57433 => Some(KeyCode::Media(MediaKeyCode::FastForward)),
    57434 => Some(KeyCode::Media(MediaKeyCode::Rewind)),
    57435 => Some(KeyCode::Media(MediaKeyCode::TrackNext)),
    57436 => Some(KeyCode::Media(MediaKeyCode::TrackPrevious)),
    57437 => Some(KeyCode::Media(MediaKeyCode::Record)),
    57438 => Some(KeyCode::Media(MediaKeyCode::LowerVolume)),
    57439 => Some(KeyCode::Media(MediaKeyCode::RaiseVolume)),
    57440 => Some(KeyCode::Media(MediaKeyCode::MuteVolume)),
    57441 => Some(KeyCode::Modifier(ModifierKeyCode::LeftShift)),
    57442 => Some(KeyCode::Modifier(ModifierKeyCode::LeftControl)),
    57443 => Some(KeyCode::Modifier(ModifierKeyCode::LeftAlt)),
    57444 => Some(KeyCode::Modifier(ModifierKeyCode::LeftSuper)),
    57445 => Some(KeyCode::Modifier(ModifierKeyCode::LeftHyper)),
    57446 => Some(KeyCode::Modifier(ModifierKeyCode::LeftMeta)),
    57447 => Some(KeyCode::Modifier(ModifierKeyCode::RightShift)),
    57448 => Some(KeyCode::Modifier(ModifierKeyCode::RightControl)),
    57449 => Some(KeyCode::Modifier(ModifierKeyCode::RightAlt)),
    57450 => Some(KeyCode::Modifier(ModifierKeyCode::RightSuper)),
    57451 => Some(KeyCode::Modifier(ModifierKeyCode::RightHyper)),
    57452 => Some(KeyCode::Modifier(ModifierKeyCode::RightMeta)),
    57453 => Some(KeyCode::Modifier(ModifierKeyCode::IsoLevel3Shift)),
    57454 => Some(KeyCode::Modifier(ModifierKeyCode::IsoLevel5Shift)),
    _ => None,
  } {
    return Some((keycode, KeyEventState::empty()));
  }

  None
}

/// Cb is the byte of a mouse input that contains the button being used, the key modifiers being
/// held and whether the mouse is dragging or not.
///
/// Bit layout of cb, from low to high:
///
/// - button number
/// - button number
/// - shift
/// - meta (alt)
/// - control
/// - mouse is dragging
/// - button number
/// - button number
fn parse_cb(cb: u8) -> anyhow::Result<(MouseEventKind, KeyModifiers)> {
  let button_number = (cb & 0b0000_0011) | ((cb & 0b1100_0000) >> 4);
  let dragging = cb & 0b0010_0000 == 0b0010_0000;

  let kind = match (button_number, dragging) {
    (0, false) => MouseEventKind::Down(MouseButton::Left),
    (1, false) => MouseEventKind::Down(MouseButton::Middle),
    (2, false) => MouseEventKind::Down(MouseButton::Right),
    (0, true) => MouseEventKind::Drag(MouseButton::Left),
    (1, true) => MouseEventKind::Drag(MouseButton::Middle),
    (2, true) => MouseEventKind::Drag(MouseButton::Right),
    (3, false) => MouseEventKind::Up(MouseButton::Left),
    (3, true) | (4, true) | (5, true) => MouseEventKind::Moved,
    (4, false) => MouseEventKind::ScrollUp,
    (5, false) => MouseEventKind::ScrollDown,
    (6, false) => MouseEventKind::ScrollLeft,
    (7, false) => MouseEventKind::ScrollRight,
    // We do not support other buttons.
    _ => bail!("Failed to parse Cb param button"),
  };

  let mut modifiers = KeyModifiers::empty();

  if cb & 0b0000_0100 == 0b0000_0100 {
    modifiers |= KeyModifiers::SHIFT;
  }
  if cb & 0b0000_1000 == 0b0000_1000 {
    modifiers |= KeyModifiers::ALT;
  }
  if cb & 0b0001_0000 == 0b0001_0000 {
    modifiers |= KeyModifiers::CONTROL;
  }

  Ok((kind, modifiers))
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
