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
  mutable size : int * int;
  mutable state : state;
  on_state_change : state Listeners.t;
  proc_dispose : Dispose.t;
  on_rerender : unit Listeners.t;
}

let init_state proc =
  Dispose.dispose proc.proc_dispose;
  match proc.state with
  | Running (Simple ps) | Stopping (Simple ps) ->
      Listeners.addl ps.on_update
        (Listeners.call proc.on_rerender)
        proc.proc_dispose
  | Running (Vterm pt) | Stopping (Vterm pt) ->
      Listeners.addl pt.on_damage
        (fun _ -> Listeners.call proc.on_rerender ())
        proc.proc_dispose
  | Stopped _ -> ()

let update_state proc state =
  let prev_state = proc.state in
  proc.state <- state;
  match (prev_state, state) with
  | (Running k0 | Stopping k0), (Running k1 | Stopping k1) when k0 == k1 -> ()
  | _ -> init_state proc

let create_ps_ ~cmd ~on_stopped =
  let ps = Proc_simple.run cmd in
  (*let kind = Simple ps in*)
  let state = Running (Simple ps) in
  Lwt.on_any ps.process#status
    (fun process_status -> on_stopped process_status)
    (fun ex ->
      [%log warn "%s" (Printexc.to_string ex)];
      on_stopped (Unix.WSTOPPED (-420)));
  state

let create_pt_ ~cmd ~size ~on_stopped =
  let pt = Proc_term.run cmd ~size in
  let kind = Vterm pt in
  let state = Running kind in
  Lwt.on_any pt.stopped
    (fun process_status -> on_stopped process_status)
    (fun ex ->
      [%log warn "%s" (Printexc.to_string ex)];
      on_stopped (Unix.WSTOPPED (-420)));
  state

let create_ ~cmd ~size ~on_stopped =
  let is_tty = cmd.Cmd.tty in
  if is_tty then create_pt_ ~cmd ~size ~on_stopped
  else create_ps_ ~cmd ~on_stopped

let create ~cmd ~name ~size () =
  let on_state_change = Listeners.create () in
  let proc_dispose = Dispose.create () in
  let state =
    create_ ~cmd ~size ~on_stopped:(fun s ->
        Listeners.call on_state_change (Stopped s))
  in
  let on_rerender = Listeners.create () in
  let proc =
    { name; cmd; size; state; on_state_change; proc_dispose; on_rerender }
  in
  let _id : Listeners.id =
    Listeners.add on_state_change (fun state -> update_state proc state)
  in
  init_state proc;
  proc

let name proc = proc.name

let state proc = proc.state

let start proc =
  match proc.state with
  | Stopped _ ->
      let state =
        create_ ~cmd:proc.cmd ~size:proc.size ~on_stopped:(fun s ->
            Listeners.call proc.on_state_change (Stopped s))
      in
      Listeners.call proc.on_state_change state
  | Stopping _ | Running _ -> ()

let stopped proc =
  match proc.state with
  | Running _ | Stopping _ ->
      let promise, resolver = Lwt.wait () in
      let id = ref None in
      id :=
        Listeners.add proc.on_state_change (function
          | Running _ | Stopping _ -> ()
          | Stopped s ->
              Option.iter (Listeners.rem proc.on_state_change) !id;
              Lwt.wakeup_later resolver s)
        |> Option.some;
      promise
  | Stopped s -> Lwt.return s

let resize ~w ~h proc =
  proc.size <- (w, h);
  match proc.state with
  | Stopped _ -> ()
  | Running kind | Stopping kind -> (
      match kind with
      | Simple _ -> ()
      | Vterm pt -> Proc_term.resize ~rows:h ~cols:w pt)

let stop proc =
  match state proc with
  | Stopped _ -> ()
  | Stopping kind | Running kind -> (
      match kind with
      | Simple ps -> Proc_simple.stop ps
      | Vterm pt -> Proc_term.stop pt)

let send_key proc (key : Tui.Event.KeyEvent.t) =
  match proc.state with
  | Running kind | Stopping kind -> (
      match kind with
      | Simple ps -> Proc_simple.send_key ps key
      | Vterm pt -> Proc_term.send_key pt key)
  | Stopped _ -> ()
