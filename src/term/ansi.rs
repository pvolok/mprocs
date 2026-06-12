use std::fmt::Write;

use super::{attrs::Attrs, color::Color, screen::Screen};

/// Render the screen contents as ANSI-styled text, one line per row,
/// trailing whitespace trimmed.
pub fn render_screen_ansi(screen: &Screen) -> String {
  let size = screen.size();
  let mut out = String::new();
  let mut brush = Attrs::default();

  for row in 0..size.height {
    if row > 0 {
      let _ = write!(out, "\r\n");
    }
    let mut line = String::new();
    let mut line_brush = brush;

    for col in 0..size.width {
      let cell = match screen.cell(row, col) {
        Some(c) => c,
        None => continue,
      };
      let attrs = *cell.attrs();

      if line_brush != attrs {
        let _ = write!(line, "\x1b[");
        let mut first = true;
        let mut sep = |w: &mut String| {
          if first {
            first = false;
            Ok(())
          } else {
            write!(w, ";")
          }
        };
        if line_brush.fgcolor != attrs.fgcolor {
          let _ = sep(&mut line);
          match attrs.fgcolor {
            Color::Default => {
              let _ = write!(line, "39");
            }
            Color::Idx(idx) => {
              let _ = write!(line, "38;5;{}", idx);
            }
            Color::Rgb(r, g, b) => {
              let _ = write!(line, "38;2;{r};{g};{b}");
            }
          }
        }
        if line_brush.bgcolor != attrs.bgcolor {
          let _ = sep(&mut line);
          match attrs.bgcolor {
            Color::Default => {
              let _ = write!(line, "49");
            }
            Color::Idx(idx) => {
              let _ = write!(line, "48;5;{}", idx);
            }
            Color::Rgb(r, g, b) => {
              let _ = write!(line, "48;2;{r};{g};{b}");
            }
          }
        }
        if line_brush.bold() != attrs.bold() {
          let _ = sep(&mut line);
          let v = if attrs.bold() { 1 } else { 22 };
          let _ = write!(line, "{v}");
        }
        if line_brush.italic() != attrs.italic() {
          let _ = sep(&mut line);
          let v = if attrs.italic() { 3 } else { 23 };
          let _ = write!(line, "{v}");
        }
        if line_brush.underline() != attrs.underline() {
          let _ = sep(&mut line);
          let v = if attrs.underline() { 4 } else { 24 };
          let _ = write!(line, "{v}");
        }
        if line_brush.inverse() != attrs.inverse() {
          let _ = sep(&mut line);
          let v = if attrs.inverse() { 7 } else { 27 };
          let _ = write!(line, "{v}");
        }
        let _ = write!(line, "m");
        line_brush = attrs;
      }

      let c = if cell.width() > 0 {
        cell.contents()
      } else {
        " "
      };
      line.push_str(c);
    }

    // Trim trailing default-attrs spaces from each line
    out.push_str(line.trim_end());
    brush = line_brush;
  }

  // Reset attributes at the end
  if brush != Attrs::default() {
    let _ = write!(out, "\x1b[0m");
  }

  out
}
