use tui::{
  layout::{Margin, Rect},
  style::{Color, Style},
  text::{Line, Span, Text},
  widgets::{Clear, Paragraph},
  Frame,
};

use crate::{
  encode_term::print_key,
  event::AppEvent,
  keymap::{Keymap, KeymapGroup},
  state::State,
  theme::Theme,
};

pub fn render_keymap(
  area: Rect,
  frame: &mut Frame,
  state: &mut State,
  keymap: &Keymap,
) {
  let theme = Theme::default();

  let block = theme
    .pane(false)
    .title(Span::styled("Help", theme.pane_title(false)));
  frame.render_widget(Clear, area);
  frame.render_widget(block, area);

  let group = state.get_keymap_group();
  let items = match group {
    KeymapGroup::Procs => vec![
      AppEvent::ToggleFocus,
      AppEvent::Quit,
      AppEvent::NextProc,
      AppEvent::PrevProc,
      AppEvent::StartProc,
      AppEvent::TermProc,
      AppEvent::RestartProc,
      AppEvent::ToggleKeymapWindow,
    ],
    KeymapGroup::Term => vec![AppEvent::ToggleFocus],
    KeymapGroup::Copy => vec![
      AppEvent::CopyModeEnd,
      AppEvent::CopyModeCopy,
      AppEvent::CopyModeLeave,
    ],
  };
  let line = items
    .into_iter()
    .filter_map(|event| Some((keymap.resolve_key(group, &event)?, event)))
    .flat_map(|(key, event)| {
      vec![
        Span::raw(" <"),
        Span::styled(print_key(key), Style::default().fg(Color::Yellow)),
        Span::raw(": "),
        Span::raw(event.desc()),
        Span::raw("> "),
      ]
    })
    .collect::<Vec<_>>();

  let line = Line::from(line);
  let line = Text::from(vec![line]);

  let p = Paragraph::new(line);
  frame.render_widget(
    p,
    area.inner(&Margin {
      vertical: 1,
      horizontal: 1,
    }),
  );
}
