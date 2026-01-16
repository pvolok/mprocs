use tui::widgets::{Clear, Paragraph, StatefulWidget, Widget};
use tui_input::Input;

pub struct TextInput<'a> {
  input: &'a mut Input,
}

impl<'a> TextInput<'a> {
  pub fn new(input: &'a mut Input) -> Self {
    TextInput { input }
  }
}

impl<'a> StatefulWidget for TextInput<'a> {
  type State = (u16, u16);

  fn render(
    self,
    area: tui::prelude::Rect,
    buf: &mut tui::prelude::Buffer,
    cursor_pos: &mut Self::State,
  ) {
    let input = self.input;
    let value = input.value();

    let left_trim = input.cursor().saturating_sub(area.width as usize);
    let (value, cursor) = if left_trim > 0 {
      let start = unicode_segmentation::UnicodeSegmentation::grapheme_indices(
        value, true,
      )
      .nth(left_trim)
      .map_or_else(|| value.len(), |(len, _)| len);
      (&value[start..], input.cursor() - left_trim)
    } else {
      (value, input.cursor())
    };

    // TODO: render directly
    let txt = Paragraph::new(value);
    Clear.render(area, buf);
    txt.render(area, buf);

    *cursor_pos = (area.x + cursor as u16, area.y);
  }
}
