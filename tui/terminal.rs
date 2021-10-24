use std::{cell::RefCell, io};

use crossterm::{
  terminal::{EnterAlternateScreen, LeaveAlternateScreen},
  ExecutableCommand,
};
use tui::{backend::CrosstermBackend, Terminal};

type Backend = CrosstermBackend<io::Stdout>;

pub struct Term {
  pub terminal: Terminal<Backend>,
}

thread_local! {
    pub static TERM : RefCell<Option<Term>> = RefCell::new(None);
}

pub fn use_term<F, R>(f: F) -> R
where
  F: FnOnce(&mut Term) -> R,
{
  TERM.with(|cell| {
    let mut term = cell.borrow_mut();
    let term = term.as_mut().unwrap();
    f(term)
  })
}

#[no_mangle]
pub extern "C" fn tui_terminal_create() {
  let stdout = io::stdout();
  let backend = CrosstermBackend::new(stdout);
  let terminal = Terminal::new(backend).unwrap();

  let term = Term { terminal };

  TERM.with(|cell| {
    let _term = cell.replace(Some(term));
  });
}

#[no_mangle]
pub extern "C" fn tui_terminal_destroy() {
  TERM.with(|cell| {
    let _term = cell.replace(None);
  });
}

#[no_mangle]
pub extern "C" fn tui_enable_raw_mode() {
  crossterm::terminal::enable_raw_mode().unwrap()
}

#[no_mangle]
pub extern "C" fn tui_disable_raw_mode() {
  crossterm::terminal::disable_raw_mode().unwrap()
}

#[no_mangle]
pub extern "C" fn tui_clear() {
  use_term(|term| term.terminal.clear().unwrap())
}

#[no_mangle]
pub extern "C" fn tui_enter_alternate_screen() {
  io::stdout().execute(EnterAlternateScreen).unwrap();
}

#[no_mangle]
pub extern "C" fn tui_leave_alternate_screen() {
  io::stdout().execute(LeaveAlternateScreen).unwrap();
}
