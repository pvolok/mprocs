use std::io::{stdout, Write};

use crossterm::{
  cursor::SetCursorStyle, event::Event, execute, queue, terminal::ClearType,
};

#[cfg(unix)]
use crate::term::term_driver::TermDriver;
use crate::{
  host::{receiver::MsgReceiver, sender::MsgSender},
  protocol::{CltToSrv, CursorStyle, SrvToClt},
};

pub async fn client_main(
  sender: MsgSender<CltToSrv>,
  receiver: MsgReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let mut term_driver = TermDriver::create()?;

  let result = client_main_loop(&mut term_driver, sender, receiver).await;

  if let Err(err) = term_driver.destroy() {
    log::error!("Term driver destroy error: {:?}", err);
  }

  result
}

async fn client_main_loop(
  term_driver: &mut TermDriver,
  mut sender: MsgSender<CltToSrv>,
  mut receiver: MsgReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let (width, height) = crossterm::terminal::size()?;
  sender.send(CltToSrv::Init { width, height })?;

  #[derive(Debug)]
  enum LocalEvent {
    ServerMsg(Option<SrvToClt>),
    TermEvent(std::io::Result<Option<Event>>),
  }

  loop {
    let event = tokio::select! {
      msg = receiver.recv() => {
        LocalEvent::ServerMsg(msg.transpose().ok().flatten())
      }
      evt = term_driver.input() => {
        LocalEvent::TermEvent(evt)
      }
    };
    match event {
      LocalEvent::ServerMsg(msg) => match msg {
        Some(msg) => match msg {
          SrvToClt::Print(text) => {
            queue!(std::io::stdout(), crossterm::style::Print(text))?;
          }
          SrvToClt::SetAttr(attr) => {
            queue!(std::io::stdout(), crossterm::style::SetAttribute(attr))?;
          }
          SrvToClt::SetFg(fg) => {
            queue!(
              std::io::stdout(),
              crossterm::style::SetForegroundColor(fg.into())
            )?;
          }
          SrvToClt::SetBg(bg) => {
            queue!(
              std::io::stdout(),
              crossterm::style::SetBackgroundColor(bg.into())
            )?;
          }
          SrvToClt::SetCursor { x, y } => {
            execute!(stdout(), crossterm::cursor::MoveTo(x, y))?;
          }
          SrvToClt::ShowCursor => {
            execute!(stdout(), crossterm::cursor::Show)?;
          }
          SrvToClt::HideCursor => {
            execute!(stdout(), crossterm::cursor::Hide)?;
          }
          SrvToClt::CursorShape(cursor_style) => {
            let cursor_style = match cursor_style {
              CursorStyle::Default => SetCursorStyle::DefaultUserShape,
              CursorStyle::BlinkingBlock => SetCursorStyle::BlinkingBlock,
              CursorStyle::SteadyBlock => SetCursorStyle::SteadyBlock,
              CursorStyle::BlinkingUnderline => {
                SetCursorStyle::BlinkingUnderScore
              }
              CursorStyle::SteadyUnderline => SetCursorStyle::SteadyUnderScore,
              CursorStyle::BlinkingBar => SetCursorStyle::BlinkingBar,
              CursorStyle::SteadyBar => SetCursorStyle::SteadyBar,
            };
            execute!(std::io::stdout(), cursor_style)?;
          }
          SrvToClt::Clear => {
            execute!(stdout(), crossterm::terminal::Clear(ClearType::All))?;
          }
          SrvToClt::Flush => {
            stdout().flush()?;
          }
          SrvToClt::Quit => break,
        },
        _ => break,
      },
      LocalEvent::TermEvent(event) => match event? {
        Some(event) => sender.send(CltToSrv::Key(event))?,
        _ => break,
      },
    }
  }

  Ok(())
}
