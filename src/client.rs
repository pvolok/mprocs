use crossterm::{
  cursor::SetCursorStyle,
  event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream},
  execute,
  terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
  },
};
use futures::StreamExt;
use scopeguard::defer;
use tokio::select;
use tui::backend::{Backend, CrosstermBackend};

use crate::{
  error::ResultLogger,
  host::{receiver::MsgReceiver, sender::MsgSender},
  protocol::{CltToSrv, CursorStyle, SrvToClt},
};

pub async fn client_main(
  sender: MsgSender<CltToSrv>,
  receiver: MsgReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  enable_raw_mode()?;

  defer!(disable_raw_mode().log_ignore());

  // If xterm modifyOtherKeys is enabled in iTerm2 then Ctrl prefixed key
  // presses are not captured. That is while using crossterm.
  // But termwiz works well, even though it seems to be using modifyOtherKeys
  // also.
  let (otherkeys_on, otherkeys_off) =
    if std::env::var("TERM_PROGRAM").unwrap_or_default() == "iTerm.app" {
      ("", "")
    } else {
      ("\x1b[>4;2m", "\x1b[>4;0m")
    };

  execute!(
    std::io::stdout(),
    EnterAlternateScreen,
    Clear(ClearType::All),
    EnableMouseCapture,
    // https://wezfurlong.org/wezterm/config/key-encoding.html#xterm-modifyotherkeys
    crossterm::style::Print(otherkeys_on),
  )?;

  defer!(execute!(
    std::io::stdout(),
    crossterm::style::Print(otherkeys_off),
    DisableMouseCapture,
    LeaveAlternateScreen
  )
  .log_ignore());

  client_main_loop(sender, receiver).await
}

async fn client_main_loop(
  mut sender: MsgSender<CltToSrv>,
  mut receiver: MsgReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let mut backend = CrosstermBackend::new(std::io::stdout());

  let init_size = backend.size()?;
  sender.send(CltToSrv::Init {
    width: init_size.width,
    height: init_size.height,
  })?;

  let mut term_events = EventStream::new();
  loop {
    #[derive(Debug)]
    enum LocalEvent {
      ServerMsg(Option<SrvToClt>),
      TermEvent(Option<std::io::Result<Event>>),
    }
    let event: LocalEvent = select! {
      msg = receiver.recv() => LocalEvent::ServerMsg(msg.transpose()?),
      event = term_events.next() => LocalEvent::TermEvent(event),
    };
    match event {
      LocalEvent::ServerMsg(msg) => match msg {
        Some(msg) => match msg {
          SrvToClt::Draw { cells } => {
            let cells = cells
              .iter()
              .map(|(a, b, cell)| (*a, *b, tui::buffer::Cell::from(cell)))
              .collect::<Vec<_>>();
            backend.draw(cells.iter().map(|(a, b, cell)| (*a, *b, cell)))?
          }
          SrvToClt::SetCursor { x, y } => backend.set_cursor(x, y)?,
          SrvToClt::ShowCursor => backend.show_cursor()?,
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
          SrvToClt::HideCursor => backend.hide_cursor()?,
          SrvToClt::Clear => backend.clear()?,
          SrvToClt::Flush => backend.flush()?,
          SrvToClt::Quit => break,
        },
        _ => break,
      },
      LocalEvent::TermEvent(event) => match event {
        Some(Ok(event)) => sender.send(CltToSrv::Key(event))?,
        _ => break,
      },
    }
  }

  Ok(())
}
