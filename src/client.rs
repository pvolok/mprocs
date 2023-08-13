use crossterm::{
  cursor::SetCursorStyle,
  event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream},
  execute,
  terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
  },
};
use futures::{FutureExt, StreamExt};
use termwiz::escape::csi::CursorStyle;
use tokio::select;
use tui::backend::{Backend, CrosstermBackend};

use crate::protocol::{CltToSrv, SrvToClt};

pub async fn client_main(
  tx: tokio::sync::mpsc::Sender<CltToSrv>,
  rx: tokio::sync::mpsc::UnboundedReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let res1 = match enable_raw_mode() {
    Ok(_) => {
      let res1 = match execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        Clear(ClearType::All),
        EnableMouseCapture,
        // https://wezfurlong.org/wezterm/config/key-encoding.html#xterm-modifyotherkeys
        crossterm::style::Print("\x1b[>4;2m"),
      ) {
        Ok(_) => client_main_inner(tx, rx).await,
        Err(err) => Err(err.into()),
      };

      let res2 =
        execute!(std::io::stdout(), DisableMouseCapture, LeaveAlternateScreen);

      res1.and(res2.map_err(anyhow::Error::from))
    }
    Err(err) => Err(err.into()),
  };

  let res2 = disable_raw_mode().map_err(anyhow::Error::from);

  res1.and(res2)
}

async fn client_main_inner(
  tx: tokio::sync::mpsc::Sender<CltToSrv>,
  mut rx: tokio::sync::mpsc::UnboundedReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let mut backend = CrosstermBackend::new(std::io::stdout());

  let init_size = backend.size()?;
  tx.send(CltToSrv::Init {
    width: init_size.width,
    height: init_size.height,
  })
  .await?;

  let mut term_events = EventStream::new();
  loop {
    enum LocalEvent {
      ServerMsg(Option<SrvToClt>),
      TermEvent(Option<std::io::Result<Event>>),
    }
    let event: LocalEvent = select! {
      msg = rx.recv().fuse() => LocalEvent::ServerMsg(msg),
      event = term_events.next().fuse() => LocalEvent::TermEvent(event),
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
        Some(Ok(event)) => tx.send(CltToSrv::Key(event)).await?,
        _ => break,
      },
    }
  }

  Ok(())
}
