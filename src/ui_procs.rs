use std::io;

use tui::{
  backend::CrosstermBackend,
  layout::Rect,
  style::{Color, Modifier, Style},
  text::{Span, Spans},
  widgets::{List, ListItem, ListState},
  Frame,
};

use crate::{
  proc::Proc,
  state::{Scope, State},
  theme::Theme,
};

type Backend = CrosstermBackend<io::Stdout>;

pub fn render_procs(area: Rect, frame: &mut Frame<Backend>, state: &mut State) {
  let theme = Theme::default();
  let theme = &theme;

  let active = state.scope == Scope::Procs;

  let mut list_state = ListState::default();
  list_state.select(Some(state.selected));
  let items = state
    .procs
    .iter_mut()
    .enumerate()
    .map(|(i, proc)| {
      create_proc_item(proc, i == state.selected, area.width - 2, theme)
    })
    .collect::<Vec<_>>();

  let title = {
    let mut spans = vec![Span::styled("Processes", theme.pane_title(active))];
    if state.quitting {
      spans.push(Span::from(" "));
      spans.push(Span::styled(
        "QUITTING",
        Style::default()
          .fg(Color::Black)
          .bg(Color::Red)
          .add_modifier(Modifier::BOLD),
      ));
    }
    spans
  };

  let items = List::new(items)
    .block(theme.pane(active).title(title))
    .style(Style::default().fg(Color::White));
  frame.render_stateful_widget(items, area, &mut list_state);
}

fn create_proc_item<'a>(
  proc: &mut Proc,
  is_cur: bool,
  width: u16,
  theme: &Theme,
) -> ListItem<'a> {
  let status = if proc.is_up() {
    Span::styled(
      " UP ",
      Style::default()
        .fg(Color::LightGreen)
        .add_modifier(Modifier::BOLD),
    )
  } else {
    Span::styled(" DOWN ", Style::default().fg(Color::LightRed))
  };

  let mark = if is_cur {
    Span::raw("•")
  } else {
    Span::raw(" ")
  };

  let mut name = proc.name.clone();
  let name_max = width as usize - mark.width() - status.width();
  let name_len = name.chars().count();
  if name_len > name_max {
    name.truncate(
      name
        .char_indices()
        .nth(name_max)
        .map_or(name.len(), |(n, _)| n),
    )
  }
  if name_len < name_max {
    for _ in name_len..name_max {
      name.push(' ');
    }
  }
  let name = Span::raw(name);

  ListItem::new(Spans::from(vec![mark, name, status]))
    .style(theme.get_procs_item(is_cur))
}
