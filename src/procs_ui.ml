open Core_kernel

let make_text ~width proc (state : Proc.state) =
  let open LTerm_text in
  let space_left = width - 2 in
  let status, status_width =
    match state with
    | Running _ -> ([ B_fg LTerm_style.green; S "UP"; E_fg ], 2)
    | Stopping _ -> ([ B_fg LTerm_style.yellow; S "UP"; E_fg ], 2)
    | Stopped _ -> ([ B_fg LTerm_style.red; S "DOWN"; E_fg ], 4)
  in
  let space_left = space_left - status_width in

  let name = Proc.name proc in
  let name_len = Zed_utf8.length name in
  let max_name_len = space_left - 1 in
  let name =
    if name_len > max_name_len then
      Zed_utf8.sub name 0 (min (String.length name) (space_left - 1))
    else name
  in
  let space_left = space_left - Zed_utf8.length name in

  let fill = String.make space_left ' ' in

  List.concat [ [ S " "; S name; S fill ]; status; [ S " " ] ]
  |> LTerm_text.eval

class t =
  object (self)
    inherit LTerm_widget.vbox as super

    val mutable procs : (Proc.t * Proc.state * bool) array = [||]
    val mutable selected = -1

    val mutable focused = false

    val on_select : Proc_term.t Listeners.t = Listeners.create ()
    method on_select = on_select

    initializer
    State.focus
    |> Inc.map ~f:(function
         | Some w -> phys_equal (self :> LTerm_widget.t) w
         | None -> false)
    |> Inc.observe
    |> Inc.Observer.on_update_exn ~f:(fun upd ->
           let f =
             match upd with
             | Initialized f -> f
             | Changed (_, f) -> f
             | Invalidated -> false
           in
           focused <- f;
           self#queue_draw);

    Inc.bind2 State.procs (State.select_index_var |> Inc.Var.watch)
      ~f:(fun procs selected ->
        Array.mapi procs ~f:(fun index proc ->
            let open Inc.Let_syntax in
            proc.state_var |> Inc.Var.watch >>| fun state ->
            (proc, state, index = selected))
        |> Array.to_list |> Inc.all)
    |> Inc.observe
    |> Inc.Observer.on_update_exn ~f:(fun upd ->
           let v =
             match upd with
             | Initialized x -> x
             | Changed (_, x) -> x
             | Invalidated -> []
           in
           procs <- Array.of_list v;
           self#queue_draw);

    let (_ : unit) =
      State.select_index_var |> Inc.Var.watch |> Inc.observe
      |> Inc.Observer.on_update_exn ~f:(fun upd ->
             let index =
               match upd with
               | Initialized x -> x
               | Changed (_, x) -> x
               | Invalidated -> -1
             in
             selected <- index;
             self#queue_draw)
    in

    self#on_event (function
      | Key key -> (
          let _modifier =
            if key.control then Vterm.Control
            else if key.meta then Vterm.Alt
            else if key.shift then Vterm.Shift
            else Vterm.None
          in
          match LTerm_key.code key with
          | Char c -> (
              match CamomileLibrary.UChar.char_of c with
              | 'j' ->
                  self#select_next;
                  true
              | 'k' ->
                  self#select_prev;
                  true
              | 's' ->
                  Option.iter self#current ~f:Proc.start;
                  true
              | 'x' ->
                  (match self#current with
                  | Some proc -> Proc.stop proc
                  | None -> ());
                  true
              | _ -> false
              | exception CamomileLibrary.UChar.Out_of_range -> false)
          | LTerm_key.Escape -> false
          | LTerm_key.Enter -> false
          | _ -> false)
      | _ -> false);

    ()

    method! size_request = { rows = Array.length procs + 2; cols = 35 }

    method! draw (cx : LTerm_draw.context) _ =
      let cx_size = LTerm_draw.size cx in

      let offset =
        if selected >= cx_size.rows then selected - cx_size.rows + 1 else 0
      in

      for row = 0 to cx_size.rows do
        let index = offset + row in
        if index < Array.length procs then
          let proc, state, active = procs.(index) in
          let style =
            if active then !Theme.cur.item_focus else !Theme.cur.item
          in
          let text = make_text ~width:cx_size.cols proc state in
          LTerm_draw.draw_styled cx row 0 ~style text
        else ()
      done

    method select i =
      Inc.Var.set State.select_index_var i;
      Inc.stabilize ()

    method select_next =
      let next = selected + 1 in
      let next = if next >= Array.length procs then 0 else next in
      self#select next

    method select_prev =
      let next = selected - 1 in
      let next = if next < 0 then Array.length procs - 1 else next in
      self#select next

    method private current : Proc.t option =
      if selected >= 0 && selected < Array.length procs then
        let proc, _, _ = procs.(selected) in
        Some proc
      else None
  end
