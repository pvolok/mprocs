use crossterm::event::Event;
use tui::{prelude::Rect, Frame};

use crate::{app::LoopAction, protocol::ProxyBackend, state::State};

pub trait Modal: Send {
  fn boxed(self) -> Box<dyn Modal>;

  fn handle_input(
    &mut self,
    state: &mut State,
    loop_action: &mut LoopAction,
    event: &Event,
  ) -> bool;

  fn get_size(&mut self, frame_area: Rect) -> (u16, u16);

  fn area(&mut self, frame_area: Rect) -> Rect {
    let (w, h) = self.get_size(frame_area);

    let y = frame_area.height.saturating_sub(h) / 2;
    let x = frame_area.width.saturating_sub(w) / 2;

    let w = w.min(frame_area.width);
    let h = h.min(frame_area.height);

    Rect {
      x,
      y,
      width: w,
      height: h,
    }
  }

  fn render(&mut self, frame: &mut Frame<ProxyBackend>);
}
