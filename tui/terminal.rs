use std::io;

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

unsafe extern "C" fn finalize<T>(value: ocaml::Raw) {
    let ptr = value.as_pointer::<T>();
    ptr.drop_in_place();
}

#[ocaml::func]
pub fn tui_terminal_create() -> Result<Pointer<Term>, Error> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    let term = Term { terminal };

    let ptr: Pointer<Term> = Pointer::alloc_final(term, Some(finalize::<Term>), None);
    Ok(ptr)
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
pub fn tui_terminal_clear(mut term: Pointer<Term>) -> Result<(), Error> {
    Ok(term.as_mut().terminal.clear()?)
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
