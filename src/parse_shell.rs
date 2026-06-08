use anyhow::{Result, bail};

/// Split a command string into argv, honoring single quotes, double quotes,
/// and backslash escapes.
pub fn split_argv(s: &str) -> Result<Vec<String>> {
  let mut args = Vec::new();
  let mut cur = String::new();
  let mut in_arg = false;
  let mut chars = s.chars().peekable();

  while let Some(c) = chars.next() {
    match c {
      c if c.is_whitespace() => {
        if in_arg {
          args.push(std::mem::take(&mut cur));
          in_arg = false;
        }
      }
      '\'' => {
        in_arg = true;
        for c2 in chars.by_ref() {
          if c2 == '\'' {
            break;
          }
          cur.push(c2);
        }
      }
      '"' => {
        in_arg = true;
        while let Some(c2) = chars.next() {
          match c2 {
            '"' => break,
            '\\' => match chars.peek() {
              Some('"') | Some('\\') => cur.push(chars.next().unwrap()),
              _ => cur.push('\\'),
            },
            _ => cur.push(c2),
          }
        }
      }
      '\\' => {
        in_arg = true;
        if let Some(next) = chars.next() {
          cur.push(next);
        }
      }
      _ => {
        in_arg = true;
        cur.push(c);
      }
    }
  }
  if in_arg {
    args.push(cur);
  }
  if args.is_empty() {
    bail!("cmd string is empty");
  }
  Ok(args)
}
