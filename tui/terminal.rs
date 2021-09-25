use std::{cell::RefCell, io};

use crossterm::{
  terminal::{EnterAlternateScreen, LeaveAlternateScreen},
  ExecutableCommand,
};
use ocaml::{Error, Pointer};
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

#[ocaml::func]
pub fn tui_terminal_create() -> Result<(), Error> {
  let stdout = io::stdout();
  let backend = CrosstermBackend::new(stdout);
  let terminal = Terminal::new(backend)?;

  let term = Term { terminal };

  TERM.with(|cell| {
    let _term = cell.replace(Some(term));
  });

  Ok(())
}

#[ocaml::func]
pub fn tui_terminal_destroy() {
  TERM.with(|cell| {
    let _term = cell.replace(None);
  });
}

#[ocaml::func]
pub fn tui_terminal_enable_raw_mode() -> Result<(), Error> {
  Ok(crossterm::terminal::enable_raw_mode()?)
}

#[ocaml::func]
pub fn tui_terminal_disable_raw_mode() -> Result<(), Error> {
  Ok(crossterm::terminal::disable_raw_mode()?)
}

#[ocaml::func]
pub fn tui_terminal_clear(mut _term: Pointer<Term>) -> Result<(), Error> {
  use_term(|term| Ok(term.terminal.clear()?))
}

#[ocaml::func]
pub fn tui_enter_alternate_screen() -> Result<(), Error> {
  io::stdout().execute(EnterAlternateScreen)?;
  Ok(())
}

#[ocaml::func]
pub fn tui_leave_alternate_screen() -> Result<(), Error> {
  io::stdout().execute(LeaveAlternateScreen)?;
  Ok(())
}
