use portable_pty::ExitStatus;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncReadExt, DuplexStream};
use tokio::sync::oneshot::Receiver;
use tokio::task::spawn_blocking;

pub struct Inst {
  rx_exit: Receiver<Option<ExitStatus>>,
  io: DuplexStream,
}

impl Inst {
  pub fn spawn(cmd: CommandBuilder) -> anyhow::Result<Self> {
    let pty_system = native_pty_system();

    let pair = pty_system.openpty(PtySize {
      rows: 24,
      cols: 80,
      pixel_width: 0,
      pixel_height: 0,
    })?;

    let (tx_exit, rx_exit) =
      tokio::sync::oneshot::channel::<Option<ExitStatus>>();
    let slave = pair.slave;
    let child = spawn_blocking(move || slave.spawn_command(cmd));
    tokio::spawn(async move {
      let status = async move {
        let mut child = child.await.ok()?.ok()?;

        // Block until program exits
        let status = spawn_blocking(move || child.wait()).await.ok()?.ok()?;
        Some(status)
      };
      let status = status.await;
      let _send_result = tx_exit.send(status);
    });

    let master = pair.master;
    let mut reader = master.try_clone_reader().unwrap();
    let mut writer = master.try_clone_writer().unwrap();
    let (server, client) = tokio::io::duplex(256);
    let (mut server_read, mut server_write) = tokio::io::split(server);
    tokio::spawn(async move {
      loop {
        let mut buf = [0; 256];
        let read_result = spawn_blocking(move || {
          let result = reader.read(&mut buf);
          result.map(|c| (reader, c))
        })
        .await
        .unwrap();
        match read_result {
          Ok((reader_back, count)) => {
            reader = reader_back;
            let _write_result = server_write.write_all(&buf[..count]).await;
          }
          _ => break,
        }
      }
      Ok::<_, anyhow::Error>(())
    });
    tokio::spawn(async move {
      loop {
        let mut buf = [0; 256];
        let count = server_read.read(&mut buf).await;
        match count {
          Ok(count) => {
            writer = spawn_blocking(move || {
              writer.write_all(&buf[..count]).unwrap();
              writer
            })
            .await
            .unwrap();
          }
          _ => break,
        }
      }
    });

    let inst = Inst {
      rx_exit,
      io: client,
    };
    Ok(inst)
  }
}

pub struct Proc {
  inst: Inst,
}
