use tokio::sync::mpsc::UnboundedReceiver;

use crate::daemon::receiver::MsgReceiver;
use crate::daemon::sender::MsgSender;
use crate::error::ResultLogger;
use crate::kernel::kernel_message::{
  KernelCommand, SharedVt, TaskContext, TaskSender,
};
use crate::kernel::task::{TaskCmd, TaskDef, TaskId, TaskStatus};
use crate::kernel::task_screen::{FramedScreenNotify, TaskScreenCmd};
use crate::mprocs::app::ClientId;
use crate::protocol::{CltToSrv, SrvToClt};
use crate::term::{ScreenDiffer, Size, TermEvent, Winsize};

pub enum ConsoleMsg {
  ClientAttached { id: ClientId, sender: TaskSender },
  ClientDetached { id: ClientId },
  ClientKey { id: ClientId, event: TermEvent },
}

pub enum ClientCmd {
  Quit,
}

pub fn spawn_client_task(
  pc: &TaskContext,
  console_task_id: TaskId,
  console_vt: SharedVt,
  console_sender: TaskSender,
  client_id: ClientId,
  size: Size,
  sock_sender: MsgSender<SrvToClt>,
  sock_receiver: MsgReceiver<CltToSrv>,
) -> TaskId {
  pc.spawn_async(
    TaskDef {
      status: TaskStatus::Running,
      ..Default::default()
    },
    move |pc, receiver| async move {
      client_worker(
        pc,
        receiver,
        console_task_id,
        console_vt,
        console_sender,
        client_id,
        size,
        sock_sender,
        sock_receiver,
      )
      .await;
    },
  )
}

async fn client_worker(
  pc: TaskContext,
  mut receiver: UnboundedReceiver<TaskCmd>,
  console_task_id: TaskId,
  console_vt: SharedVt,
  console_sender: TaskSender,
  client_id: ClientId,
  size: Size,
  mut sock_sender: MsgSender<SrvToClt>,
  mut sock_receiver: MsgReceiver<CltToSrv>,
) {
  let my_task_id = pc.task_id;
  let my_sender = pc.get_task_sender(my_task_id);

  console_sender.send(TaskCmd::msg(TaskScreenCmd::Observe {
    size: Winsize {
      x: size.width,
      y: size.height,
      x_px: 0,
      y_px: 0,
    },
    sender: my_sender.clone(),
  }));

  console_sender.send(TaskCmd::msg(ConsoleMsg::ClientAttached {
    id: client_id,
    sender: my_sender,
  }));

  let mut differ = ScreenDiffer::new();

  loop {
    enum Next {
      Cmd(Option<TaskCmd>),
      Sock(Option<Result<CltToSrv, bincode::Error>>),
    }
    let next = tokio::select! {
      cmd = receiver.recv() => Next::Cmd(cmd),
      msg = sock_receiver.recv() => Next::Sock(msg),
    };

    match next {
      Next::Cmd(None) => break,
      Next::Cmd(Some(cmd)) => match cmd {
        TaskCmd::Start | TaskCmd::Stop | TaskCmd::Kill => (),
        TaskCmd::Msg(msg) => {
          let msg = match msg.downcast::<FramedScreenNotify>() {
            Ok(notify) => {
              match *notify {
                FramedScreenNotify::ObserveStarted { .. }
                | FramedScreenNotify::Render { .. } => {
                  render_to_socket(&console_vt, &mut differ, &mut sock_sender)
                    .await;
                }
                FramedScreenNotify::Bell { .. } => (),
              }
              continue;
            }
            Err(msg) => msg,
          };
          let msg = match msg.downcast::<ClientCmd>() {
            Ok(cmd) => match *cmd {
              ClientCmd::Quit => {
                let _ = sock_sender.send(SrvToClt::Quit).await;
                break;
              }
            },
            Err(msg) => msg,
          };
          let _ = msg;
          log::error!("ClientTask received unknown Msg");
        }
      },
      Next::Sock(None) | Next::Sock(Some(Err(_))) => break,
      Next::Sock(Some(Ok(msg))) => match msg {
        CltToSrv::Init { .. } => (),
        CltToSrv::Rpc(_) => (),
        CltToSrv::Key(TermEvent::Resize(width, height)) => {
          pc.send(KernelCommand::TaskCmd(
            console_task_id,
            TaskCmd::msg(TaskScreenCmd::Resize {
              size: Winsize {
                x: width,
                y: height,
                x_px: 0,
                y_px: 0,
              },
              observer_id: my_task_id,
            }),
          ));
        }
        CltToSrv::Key(event) => {
          console_sender.send(TaskCmd::msg(ConsoleMsg::ClientKey {
            id: client_id,
            event,
          }));
        }
      },
    }
  }

  pc.send(KernelCommand::TaskCmd(
    console_task_id,
    TaskCmd::msg(TaskScreenCmd::Unobserve {
      observer_id: my_task_id,
    }),
  ));
  console_sender
    .send(TaskCmd::msg(ConsoleMsg::ClientDetached { id: client_id }));
  pc.send(KernelCommand::RemoveTask(my_task_id));
}

async fn render_to_socket(
  console_vt: &SharedVt,
  differ: &mut ScreenDiffer,
  sender: &mut MsgSender<SrvToClt>,
) {
  let out = {
    let vt = match console_vt.read() {
      Ok(v) => v,
      Err(_) => return,
    };
    let grid = vt.screen().grid();
    let mut out = String::new();
    differ.diff(&mut out, grid).log_ignore();
    out
  };
  let _ = sender.send(SrvToClt::Print(out)).await;
  let _ = sender.send(SrvToClt::Flush).await;
}
