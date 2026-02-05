use tui_input::Input;
use unicode_width::UnicodeWidthStr;

use crate::{
  app::LoopAction,
  event::AppEvent,
  kernel::kernel_message::ProcContext,
  key::{Key, KeyCode, KeyMods},
  state::State,
  term::{
    line_symbols::{HORIZONTAL, VERTICAL_LEFT, VERTICAL_RIGHT},
    TermEvent,
  },
  vt100::{
    attrs::Attrs,
    grid::{Pos, Rect},
    Color, Grid,
  },
  widgets::{
    list::ListState,
    text_input::{render_text_input, to_input_request},
  },
};

use super::modal::Modal;

pub struct CommandsMenuModal {
  pc: ProcContext,
  input: Input,
  list_state: ListState,
  items: Vec<CommandInfo>,
}

impl CommandsMenuModal {
  pub fn new(pc: ProcContext) -> Self {
    CommandsMenuModal {
      pc,
      input: Input::default(),
      list_state: ListState::default(),
      items: get_commands(""),
    }
  }
}

impl Modal for CommandsMenuModal {
  fn handle_input(
    &mut self,
    _state: &mut State,
    loop_action: &mut LoopAction,
    event: &TermEvent,
  ) -> bool {
    match event {
      TermEvent::Key(Key {
        code: KeyCode::Enter,
        mods,
        ..
      }) if mods.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        if let Some((_, _, event)) = self.items.get(self.list_state.selected())
        {
          self.pc.send_self_custom(event.clone());
        }
        // Skip because AddProc event will immediately rerender.
        return true;
      }
      TermEvent::Key(Key {
        code: KeyCode::Esc,
        mods,
        ..
      }) if mods.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        loop_action.render();
        return true;
      }
      // List bindings
      TermEvent::Key(Key {
        code: KeyCode::Char('n'),
        mods,
        ..
      }) if mods == &KeyMods::CONTROL => {
        let index = self.list_state.selected();
        let index = if index >= self.items.len() - 1 {
          0
        } else {
          index + 1
        };
        self.list_state.select(index);
        loop_action.render();
        return true;
      }
      TermEvent::Key(Key {
        code: KeyCode::Char('p'),
        mods,
        ..
      }) if mods == &KeyMods::CONTROL => {
        let index = self.list_state.selected();
        let index = if index == 0 {
          self.items.len() - 1
        } else {
          index - 1
        };
        self.list_state.select(index);
        loop_action.render();
        return true;
      }
      _ => (),
    }

    let req = to_input_request(event);
    if let Some(req) = req {
      let res = self.input.handle(req);
      if let Some(res) = res {
        if res.value {
          self.items = get_commands(self.input.value());
        }
      }
      loop_action.render();
      return true;
    }

    match event {
      TermEvent::FocusGained => false,
      TermEvent::FocusLost => false,
      // Block keys
      TermEvent::Key(_) => true,
      // Block mouse
      TermEvent::Mouse(_) => true,
      // Block paste
      TermEvent::Paste(_) => true,
      TermEvent::Resize(_, _) => false,
    }
  }

  fn get_size(&mut self, _: Rect) -> (u16, u16) {
    (60, 30)
  }

  fn render(&mut self, grid: &mut Grid) {
    let area = self.area(Rect {
      x: 0,
      y: 0,
      width: grid.size().width,
      height: grid.size().height,
    });

    grid.draw_block(
      area.into(),
      crate::vt100::grid::BorderType::Plain,
      Attrs::default(),
    );

    let inner = area.inner(1);
    let list_area = Rect {
      x: inner.x,
      y: inner.y,
      width: inner.width,
      height: inner.height.saturating_sub(2),
    };
    let above_input = Rect {
      x: inner.x,
      y: (inner.y + inner.height).saturating_sub(2),
      width: inner.width,
      height: 1,
    };

    grid.fill_area(inner.into(), ' ', Attrs::default());

    for (i, (cmd, desc, _event)) in self.items.iter().enumerate() {
      let mut row_area = Rect {
        x: list_area.x,
        y: list_area.y + i as u16,
        width: list_area.width,
        height: 1,
      };
      row_area.x = grid
        .draw_text(
          row_area,
          if self.list_state.selected() == i {
            ">"
          } else {
            " "
          },
          Attrs::default(),
        )
        .right();
      row_area.x = grid
        .draw_text(row_area, *cmd, Attrs::default().fg(Color::WHITE))
        .right();
      row_area.x = grid.draw_text(row_area, " ", Attrs::default()).right();
      row_area.x = grid.draw_text(row_area, " ", Attrs::default()).right();
      row_area.x = grid
        .draw_text(
          row_area,
          &desc,
          Attrs::default().fg(Color::BRIGHT_BLACK).set_italic(true),
        )
        .right();
    }

    let input_label = "Run command";
    grid.draw_text(above_input, input_label, Attrs::default());

    grid.draw_text(
      Rect::new(area.x, above_input.y, 1, 1),
      VERTICAL_RIGHT,
      Attrs::default(),
    );
    grid.draw_text(
      Rect::new(above_input.right(), above_input.y, 1, 1),
      VERTICAL_LEFT,
      Attrs::default(),
    );
    let line_width =
      above_input.width.saturating_sub(input_label.width() as u16);
    grid.draw_text(
      Rect::new(
        above_input.x + above_input.width - line_width,
        above_input.y,
        line_width,
        1,
      ),
      HORIZONTAL.repeat(line_width as usize).as_str(),
      Attrs::default(),
    );

    let mut cursor = (0u16, 0u16);
    render_text_input(
      &mut self.input,
      Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
      grid,
      &mut cursor,
    );

    grid.cursor_pos = Some(Pos {
      col: cursor.0,
      row: cursor.1,
    });
  }
}

type CommandInfo = (&'static str, String, AppEvent);

fn get_commands(search: &str) -> Vec<CommandInfo> {
  let events = [
    // ("quit-or-ask", AppEvent::QuitOrAsk),
    ("quit", AppEvent::Quit),
    ("force-quit", AppEvent::ForceQuit),
    ("toggle-focus", AppEvent::ToggleFocus),
    ("focus-term", AppEvent::FocusTerm),
    ("zoom", AppEvent::Zoom),
    ("show-commands-menu", AppEvent::ShowCommandsMenu),
    ("next-proc", AppEvent::NextProc),
    ("prev-proc", AppEvent::PrevProc),
    ("start-proc", AppEvent::StartProc),
    ("term-proc", AppEvent::TermProc),
    ("kill-proc", AppEvent::KillProc),
    ("restart-proc", AppEvent::RestartProc),
    ("restart-all", AppEvent::RestartAll),
    ("duplicate-proc", AppEvent::DuplicateProc),
    ("force-restart-proc", AppEvent::ForceRestartProc),
    ("force-restart-all", AppEvent::ForceRestartAll),
    ("show-add-proc", AppEvent::ShowAddProc),
    ("show-rename-proc", AppEvent::ShowRenameProc),
    ("show-remove-proc", AppEvent::ShowRemoveProc),
    ("close-current-modal", AppEvent::CloseCurrentModal),
    ("scroll-down", AppEvent::ScrollDown),
    ("scroll-up", AppEvent::ScrollUp),
    ("copy-mode-enter", AppEvent::CopyModeEnter),
    ("copy-mode-leave", AppEvent::CopyModeLeave),
    ("copy-mode-end", AppEvent::CopyModeEnd),
    ("copy-mode-copy", AppEvent::CopyModeCopy),
  ];

  let mut result = Vec::new();
  for (cmd, event) in events {
    let desc = event.desc();
    if cmd.contains(search) || desc.contains(search) {
      result.push((cmd, desc, event));
    }
  }

  result
}
