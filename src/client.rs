use std::io::{stdout, Write};

use crossterm::{
  cursor::SetCursorStyle,
  event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
  },
  execute, queue,
  terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
  },
};
use futures::StreamExt;
use scopeguard::defer;
use tokio::select;

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

  // https://wezfurlong.org/wezterm/config/key-encoding.html#xterm-modifyotherkeys
  // If xterm modifyOtherKeys is enabled in iTerm2 then Ctrl prefixed key
  // presses are not captured. That is while using crossterm.
  // But termwiz works well, even though it seems to be using modifyOtherKeys
  // also.
  // PushKeyboardEnhancementFlags fixes this issue in iTerm2.
  let (otherkeys_on, otherkeys_off) = ("\x1b[>4;2m", "\x1b[>4;0m");

  execute!(
    std::io::stdout(),
    EnterAlternateScreen,
    Clear(ClearType::All),
    EnableMouseCapture,
    crossterm::style::Print(otherkeys_on),
    PushKeyboardEnhancementFlags(
      KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
    )
  )?;

  defer!(execute!(
    std::io::stdout(),
    PopKeyboardEnhancementFlags,
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
  let (width, height) = crossterm::terminal::size()?;
  sender.send(CltToSrv::Init { width, height })?;

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
      LocalEvent::TermEvent(event) => match event {
        Some(Ok(event)) => sender.send(CltToSrv::Key(event))?,
        _ => break,
      },
    }
  }

  Ok(())
}
