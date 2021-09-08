type kind =
  | Simple of Proc_simple.t
  | Vterm of Proc_term.t

type state =
  | Running of kind
  | Stopping of kind
  | Stopped of Unix.process_status

type t = {
  name : string;
  cmd : Cmd.t;
  kind_var : kind Lwd.var;
  mutable auto_restart : bool;
  state_var : state Lwd.var;
}

let create_ps_ ~cmd ~state_var =
  let ps = Proc_simple.run cmd in
  let kind = Simple ps in
  let state_var =
    match state_var with
    | Some state_var -> state_var
    | None -> Lwd.var (Running (Simple ps))
  in
  Lwt.on_success ps.process#status (fun process_status ->
      Lwd.set state_var (Stopped process_status));
  (kind, state_var)

let create_pt_ ~cmd ~state_var =
  let pt = Proc_term.run cmd in
  let kind = Vterm pt in
  let state_var =
    match state_var with
    | Some state_var -> state_var
    | None -> Lwd.var (Running kind)
  in
  Lwt.on_success pt.stopped (fun process_status ->
      Lwd.set state_var (Stopped process_status));
  (kind, state_var)

let create_kind_ ~cmd ~state_var =
  let is_tty = cmd.Cmd.tty in
  if is_tty then create_pt_ ~cmd ~state_var else create_ps_ ~cmd ~state_var

let create ~cmd ~name () =
  let kind, state_var = create_kind_ ~cmd ~state_var:None in
  { name; cmd; kind_var = Lwd.var kind; auto_restart = false; state_var }

let name proc = proc.name

let state proc =
  let kind = Lwd.peek proc.kind_var in
  match kind with
  | Simple ps -> (
      match ps.process#state with
      | Running -> Running kind
      | Exited process_status -> Stopped process_status)
  | Vterm pt -> (
      match Lwt.poll pt.stopped with
      | Some status -> Stopped status
      | None -> Running kind)

let start proc =
  match state proc with
  | Stopped _ ->
      let kind, _ =
        create_kind_ ~cmd:proc.cmd ~state_var:(Some proc.state_var)
      in
      Lwd.set proc.kind_var kind;
      Lwd.set proc.state_var (Running kind)
  | Stopping _ | Running _ -> ()

let stopped proc =
  match Lwd.peek proc.kind_var with
  | Simple ps -> ps.process#status
  | Vterm pt -> Proc_term.stopped pt

let stop proc =
  match state proc with
  | Stopped _ -> ()
  | Stopping kind | Running kind -> (
      match kind with
      | Simple ps -> Proc_simple.stop ps
      | Vterm pt -> Proc_term.stop pt)

let kind' proc = Lwd.get proc.kind_var

let send_key t (key : Nottui.Ui.key) =
  match Lwd.peek t.kind_var with
  | Simple ps -> Proc_simple.send_key ps key
  | Vterm pt -> Proc_term.send_key pt key
