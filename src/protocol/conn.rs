use anyhow::bail;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{FramedRead, FramedWrite};

use crate::protocol::ctl::{
  Bye, CtlMsg, Hello, PROTOCOL_VERSION, codes, local_hello,
};
use crate::protocol::wire::{FrameCodec, KIND_CTL, KIND_OUT, RawFrame};

type DynWrite = dyn AsyncWrite + Unpin + Send + 'static;
type DynRead = dyn AsyncRead + Unpin + Send + 'static;

#[derive(Debug)]
pub enum Msg {
  Ctl(CtlMsg),
  Out(Bytes),
}

pub struct ConnSender {
  writer: FramedWrite<Box<DynWrite>, FrameCodec>,
}

impl ConnSender {
  pub fn new<W: AsyncWrite + Unpin + Send + 'static>(write: W) -> Self {
    let write: Box<DynWrite> = Box::new(write);
    ConnSender {
      writer: FramedWrite::new(write, FrameCodec::new()),
    }
  }

  pub async fn send_ctl(&mut self, msg: CtlMsg) -> anyhow::Result<()> {
    let payload = serde_json::to_vec(&msg)?;
    let frame = RawFrame {
      kind: KIND_CTL,
      payload: Bytes::from(payload),
    };
    self.writer.send(frame).await?;
    Ok(())
  }

  pub async fn send_out(&mut self, bytes: Bytes) -> anyhow::Result<()> {
    let frame = RawFrame {
      kind: KIND_OUT,
      payload: bytes,
    };
    self.writer.send(frame).await?;
    Ok(())
  }
}

pub struct ConnReceiver {
  reader: FramedRead<Box<DynRead>, FrameCodec>,
}

impl ConnReceiver {
  pub fn new<R: AsyncRead + Unpin + Send + 'static>(read: R) -> Self {
    let read: Box<DynRead> = Box::new(read);
    ConnReceiver {
      reader: FramedRead::new(read, FrameCodec::new()),
    }
  }

  /// Next message. `None` means the connection closed; `Some(Err(_))`
  /// means a protocol error and the connection must be dropped.
  /// Unknown frame kinds and unknown control message types are skipped.
  pub async fn recv(&mut self) -> Option<anyhow::Result<Msg>> {
    loop {
      let frame = match self.reader.next().await? {
        Ok(frame) => frame,
        Err(err) => return Some(Err(err.into())),
      };
      match frame.kind {
        KIND_CTL => {
          let value: Value = match serde_json::from_slice(&frame.payload) {
            Ok(value) => value,
            Err(err) => {
              return Some(Err(anyhow::anyhow!(
                "invalid control frame: {err}"
              )));
            }
          };
          match serde_json::from_value::<CtlMsg>(value.clone()) {
            Ok(msg) => return Some(Ok(Msg::Ctl(msg))),
            Err(err) => {
              // Unknown message types are future protocol additions;
              // a malformed known type is a peer bug.
              let unknown_type =
                value.get("type").and_then(|t| t.as_str()).is_some_and(|t| {
                  let known = ["hello", "request", "response", "event", "bye"];
                  !known.contains(&t)
                });
              if unknown_type {
                log::debug!("skipping unknown control message: {value}");
                continue;
              }
              return Some(Err(anyhow::anyhow!(
                "malformed control message: {err}"
              )));
            }
          }
        }
        KIND_OUT => return Some(Ok(Msg::Out(frame.payload))),
        kind => {
          log::debug!("skipping unknown frame kind {kind}");
          continue;
        }
      }
    }
  }

  pub async fn recv_ctl(&mut self) -> anyhow::Result<CtlMsg> {
    loop {
      match self.recv().await {
        Some(Ok(Msg::Ctl(msg))) => return Ok(msg),
        Some(Ok(Msg::Out(_))) => {
          bail!("`recv_ctl` got OUT frame");
        }
        Some(Err(err)) => return Err(err),
        None => bail!("connection closed"),
      }
    }
  }
}

pub async fn client_handshake(
  sender: &mut ConnSender,
  receiver: &mut ConnReceiver,
) -> anyhow::Result<Hello> {
  sender.send_ctl(CtlMsg::Hello(local_hello())).await?;
  match receiver.recv_ctl().await? {
    CtlMsg::Hello(hello) => {
      if hello.protocol != PROTOCOL_VERSION {
        bail!(
          "daemon ({}) speaks protocol {}, this binary speaks {}; \
           restart it with `dk server stop && dk up`",
          hello.app,
          hello.protocol,
          PROTOCOL_VERSION,
        );
      }
      Ok(hello)
    }
    CtlMsg::Bye(bye) => bail!("daemon refused connection: {}", bye_text(&bye)),
    msg => bail!("expected hello from daemon, got {msg:?}"),
  }
}

pub async fn server_handshake(
  sender: &mut ConnSender,
  receiver: &mut ConnReceiver,
) -> anyhow::Result<Hello> {
  match receiver.recv_ctl().await? {
    CtlMsg::Hello(hello) => {
      if hello.protocol != PROTOCOL_VERSION {
        let bye = Bye {
          code: codes::UNSUPPORTED_PROTOCOL.to_string(),
          message: format!(
            "daemon speaks protocol {}, client ({}) speaks {}",
            PROTOCOL_VERSION, hello.app, hello.protocol,
          ),
        };
        let _ = sender.send_ctl(CtlMsg::Bye(bye)).await;
        bail!(
          "client ({}) speaks unsupported protocol {}",
          hello.app,
          hello.protocol
        );
      }
      sender.send_ctl(CtlMsg::Hello(local_hello())).await?;
      Ok(hello)
    }
    msg => bail!("expected hello from client, got {msg:?}"),
  }
}

fn bye_text(bye: &Bye) -> String {
  if bye.message.is_empty() {
    bye.code.clone()
  } else {
    format!("{} ({})", bye.message, bye.code)
  }
}

#[cfg(test)]
mod tests {
  use bytes::BytesMut;
  use tokio::io::AsyncWriteExt;
  use tokio_util::codec::Encoder;

  use super::*;
  use crate::protocol::ctl::{Request, RpcError};
  use crate::protocol::rpc::DkRequest;

  fn pair() -> (ConnSender, ConnReceiver, ConnSender, ConnReceiver) {
    let (client, server) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(client);
    let (server_read, server_write) = tokio::io::split(server);
    (
      ConnSender::new(client_write),
      ConnReceiver::new(client_read),
      ConnSender::new(server_write),
      ConnReceiver::new(server_read),
    )
  }

  async fn raw_pair(frames: Vec<RawFrame>) -> ConnReceiver {
    let (client, server) = tokio::io::duplex(64 * 1024);
    let (_client_read, mut client_write) = tokio::io::split(client);
    let (server_read, _server_write) = tokio::io::split(server);
    let mut buf = BytesMut::new();
    let mut codec = FrameCodec::new();
    for frame in frames {
      codec.encode(frame, &mut buf).unwrap();
    }
    client_write.write_all(&buf).await.unwrap();
    client_write.shutdown().await.unwrap();
    ConnReceiver::new(server_read)
  }

  fn ctl_frame(json: &str) -> RawFrame {
    RawFrame {
      kind: KIND_CTL,
      payload: Bytes::copy_from_slice(json.as_bytes()),
    }
  }

  #[tokio::test]
  async fn handshake_completes_both_ways() {
    let (mut cs, mut cr, mut ss, mut sr) = pair();
    let server =
      tokio::spawn(
        async move { server_handshake(&mut ss, &mut sr).await.unwrap() },
      );
    let server_hello = client_handshake(&mut cs, &mut cr).await.unwrap();
    let client_hello = server.await.unwrap();
    assert_eq!(server_hello.protocol, PROTOCOL_VERSION);
    assert_eq!(client_hello.protocol, PROTOCOL_VERSION);
    assert!(client_hello.app.starts_with("dk "));
  }

  #[tokio::test]
  async fn server_rejects_unsupported_protocol_with_bye() {
    let (mut cs, mut cr, mut ss, mut sr) = pair();
    let server =
      tokio::spawn(
        async move { server_handshake(&mut ss, &mut sr).await.is_err() },
      );
    cs.send_ctl(CtlMsg::Hello(Hello {
      protocol: 999,
      app: "dk future".to_string(),
      features: vec![],
    }))
    .await
    .unwrap();
    assert!(server.await.unwrap());
    match cr.recv_ctl().await.unwrap() {
      CtlMsg::Bye(bye) => {
        assert_eq!(bye.code, codes::UNSUPPORTED_PROTOCOL);
      }
      msg => panic!("expected bye, got {msg:?}"),
    }
  }

  #[tokio::test]
  async fn client_rejects_mismatched_server() {
    let (mut cs, mut cr, mut ss, mut sr) = pair();
    let server = tokio::spawn(async move {
      match sr.recv_ctl().await.unwrap() {
        CtlMsg::Hello(_) => (),
        msg => panic!("expected hello, got {msg:?}"),
      }
      ss.send_ctl(CtlMsg::Hello(Hello {
        protocol: 999,
        app: "dk future".to_string(),
        features: vec![],
      }))
      .await
      .unwrap();
    });
    let err = client_handshake(&mut cs, &mut cr).await.unwrap_err();
    assert!(err.to_string().contains("protocol 999"), "{err}");
    server.await.unwrap();
  }

  #[tokio::test]
  async fn rpc_round_trip() {
    let (mut cs, mut cr, mut ss, mut sr) = pair();
    let server = tokio::spawn(async move {
      let request = match sr.recv_ctl().await.unwrap() {
        CtlMsg::Request(request) => request,
        msg => panic!("expected request, got {msg:?}"),
      };
      let req = DkRequest::from_wire(&request.method, request.params).unwrap();
      assert_eq!(
        req,
        DkRequest::Start {
          pattern: "/web".to_string()
        }
      );
      ss.send_ctl(CtlMsg::ok(request.id, serde_json::json!({})))
        .await
        .unwrap();
    });
    let (method, params) = DkRequest::Start {
      pattern: "/web".to_string(),
    }
    .to_wire();
    cs.send_ctl(CtlMsg::Request(Request {
      id: 7,
      method: method.to_string(),
      params,
    }))
    .await
    .unwrap();
    match cr.recv_ctl().await.unwrap() {
      CtlMsg::Response(response) => {
        assert_eq!(response.id, 7);
        assert_eq!(response.error, None);
      }
      msg => panic!("expected response, got {msg:?}"),
    }
    server.await.unwrap();
  }

  #[tokio::test]
  async fn error_response_round_trips() {
    let (mut cs, _cr, _ss, mut sr) = pair();
    cs.send_ctl(CtlMsg::err(3, RpcError::new(codes::NO_MATCH, "nope")))
      .await
      .unwrap();
    match sr.recv_ctl().await.unwrap() {
      CtlMsg::Response(response) => {
        let error = response.error.unwrap();
        assert_eq!(error.code, codes::NO_MATCH);
        assert_eq!(error.message, "nope");
      }
      msg => panic!("expected response, got {msg:?}"),
    }
  }

  #[tokio::test]
  async fn out_frames_pass_through_unescaped() {
    let (mut cs, _cr, _ss, mut sr) = pair();
    let ansi = Bytes::from_static(b"\x1b[2J\x1b[Hhello \xf0\x9f\x91\x8b");
    cs.send_out(ansi.clone()).await.unwrap();
    match sr.recv().await.unwrap().unwrap() {
      Msg::Out(bytes) => assert_eq!(bytes, ansi),
      msg => panic!("expected out frame, got {msg:?}"),
    }
  }

  #[tokio::test]
  async fn unknown_message_type_is_skipped() {
    let mut receiver = raw_pair(vec![
      ctl_frame(r#"{"type":"hologram","data":[1,2,3]}"#),
      ctl_frame(r#"{"type":"bye","code":"quit"}"#),
    ])
    .await;
    match receiver.recv().await.unwrap().unwrap() {
      Msg::Ctl(CtlMsg::Bye(bye)) => assert_eq!(bye.code, "quit"),
      msg => panic!("expected bye, got {msg:?}"),
    }
  }

  #[tokio::test]
  async fn unknown_frame_kind_is_skipped() {
    let mut receiver = raw_pair(vec![
      RawFrame {
        kind: 0x42,
        payload: Bytes::from_static(b"future data"),
      },
      ctl_frame(r#"{"type":"bye","code":"quit"}"#),
    ])
    .await;
    match receiver.recv().await.unwrap().unwrap() {
      Msg::Ctl(CtlMsg::Bye(bye)) => assert_eq!(bye.code, "quit"),
      msg => panic!("expected bye, got {msg:?}"),
    }
  }

  #[tokio::test]
  async fn malformed_known_message_is_fatal() {
    let mut receiver =
      raw_pair(vec![ctl_frame(r#"{"type":"request","id":"abc"}"#)]).await;
    assert!(receiver.recv().await.unwrap().is_err());
  }

  #[tokio::test]
  async fn invalid_json_is_fatal() {
    let mut receiver = raw_pair(vec![ctl_frame(r#"{"type": "#)]).await;
    assert!(receiver.recv().await.unwrap().is_err());
  }

  #[tokio::test]
  async fn close_yields_none() {
    let mut receiver = raw_pair(vec![]).await;
    assert!(receiver.recv().await.is_none());
  }
}
