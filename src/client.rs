use std::io::{stdout, Write};

use crate::key::{Key, KeyEventKind};
use crate::term::term_driver::TermDriver;
use crate::term::TermEvent;
use crate::{
  host::{receiver::MsgReceiver, sender::MsgSender},
  protocol::{CltToSrv, SrvToClt},
};

pub async fn client_main(
  sender: MsgSender<CltToSrv>,
  receiver: MsgReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let mut term_driver = TermDriver::create()?;

  client_main_loop(&mut term_driver, sender, receiver).await
}

async fn client_main_loop(
  term_driver: &mut TermDriver,
  mut sender: MsgSender<CltToSrv>,
  mut receiver: MsgReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let size = term_driver.size()?;
  sender
    .send(CltToSrv::Init {
      width: size.width,
      height: size.height,
    })
    .await?;

  #[derive(Debug)]
  enum LocalEvent {
    ServerMsg(Option<SrvToClt>),
    TermEvent(std::io::Result<Option<TermEvent>>),
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
        Some(TermEvent::Key(Key {
          kind: KeyEventKind::Release,
          ..
        })) => (),
        Some(event) => sender.send(CltToSrv::Key(event)).await?,
        _ => break,
      },
    }
  }

  Ok(())
}
