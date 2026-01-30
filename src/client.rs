use std::io::{stdout, Write};

use crossterm::event::Event;

use crate::term::term_driver::TermDriver;
use crate::{
  host::{receiver::MsgReceiver, sender::MsgSender},
  protocol::{CltToSrv, SrvToClt},
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
            std::io::stdout().write_all(text.as_bytes())?;
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
