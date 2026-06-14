use anyhow::bail;
use tokio::io::AsyncWriteExt;

use crate::protocol::{
  ConnReceiver, ConnSender, CtlMsg, Event, Msg, Request, RpcRequest,
  client_handshake, codes, ctl::EVENT_INPUT,
};
use crate::term::TermEvent;
use crate::term::key::{Key, KeyEventKind};
use crate::term_driver::TermDriver;

pub async fn client_main(
  mut sender: ConnSender,
  mut receiver: ConnReceiver,
) -> anyhow::Result<()> {
  client_handshake(&mut sender, &mut receiver).await?;

  let mut term_driver = TermDriver::create()?;
  let result = client_loop(&mut term_driver, sender, receiver).await;
  drop(term_driver);
  result
}

async fn client_loop(
  term_driver: &mut TermDriver,
  mut sender: ConnSender,
  mut receiver: ConnReceiver,
) -> anyhow::Result<()> {
  let size = term_driver.size()?;
  let (method, params) = RpcRequest::Attach {
    width: size.width,
    height: size.height,
  }
  .to_wire();
  sender
    .send_ctl(CtlMsg::Request(Request {
      id: 1,
      method,
      params,
    }))
    .await?;

  #[derive(Debug)]
  enum LocalEvent {
    ServerMsg(Option<anyhow::Result<Msg>>),
    TermEvent(std::io::Result<Option<TermEvent>>),
  }

  let mut stdout = tokio::io::stdout();

  loop {
    let event = tokio::select! {
      msg = receiver.recv() => LocalEvent::ServerMsg(msg),
      evt = term_driver.input() => LocalEvent::TermEvent(evt),
    };
    match event {
      LocalEvent::ServerMsg(msg) => match msg {
        Some(Ok(Msg::Out(bytes))) => {
          stdout.write_all(&bytes).await?;
          stdout.flush().await?;
        }
        Some(Ok(Msg::Ctl(msg))) => match msg {
          CtlMsg::Response(response) => {
            if let Some(error) = response.error {
              bail!("attach failed: {error}");
            }
          }
          CtlMsg::Bye(bye) => {
            if bye.code == codes::QUIT {
              break;
            }
            bail!("server closed the session: {}", bye.code);
          }
          msg @ (CtlMsg::Hello(_) | CtlMsg::Request(_) | CtlMsg::Event(_)) => {
            log::debug!("ignoring server message {msg:?}");
          }
        },
        Some(Err(err)) => return Err(err),
        None => break,
      },
      LocalEvent::TermEvent(event) => match event? {
        Some(TermEvent::Key(Key {
          kind: KeyEventKind::Release,
          ..
        })) => (),
        Some(event) => {
          sender
            .send_ctl(CtlMsg::Event(Event {
              name: EVENT_INPUT.to_string(),
              params: serde_json::to_value(&event)?,
            }))
            .await?;
        }
        _ => break,
      },
    }
  }

  let _ = stdout.flush().await;

  Ok(())
}
